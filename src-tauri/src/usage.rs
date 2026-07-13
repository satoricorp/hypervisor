//! M6: token + cost ledger from transcripts.
//!
//! On-demand only — this reads full transcript files, so it must NEVER run on
//! the 2s registry tick (call it when the usage view opens / the ticker
//! refreshes). claude code and codex record real token usage; cursor and
//! opencode don't expose it reliably, so they're omitted (best-effort per PLAN).

use crate::adapters::{file_mtime, home_dir, now_secs};
use chrono::{DateTime, Local, NaiveDate};
use serde::Serialize;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

/// USD per 1M tokens. **EDIT THESE when vendor prices change** — this table is
/// the single source of truth for the ledger (no network price fetch in v1).
#[derive(Clone, Copy)]
struct Price {
    input: f64,
    cache_write: f64,
    cache_read: f64,
    output: f64,
}

/// Match the most specific token first. Unknown models price at 0 (tokens still
/// count) rather than guess — the UI shows the model so a gap is visible.
fn price_for(model: &str) -> Price {
    let m = model.to_ascii_lowercase();
    if m.contains("opus") || m.contains("fable") {
        // Opus-class (Fable 5 treated the same until a public price ships).
        Price { input: 15.0, cache_write: 18.75, cache_read: 1.5, output: 75.0 }
    } else if m.contains("sonnet") {
        Price { input: 3.0, cache_write: 3.75, cache_read: 0.3, output: 15.0 }
    } else if m.contains("haiku") {
        Price { input: 1.0, cache_write: 1.25, cache_read: 0.1, output: 5.0 }
    } else if m.contains("gpt-5-codex") || m.contains("gpt-5") || m.contains("codex") {
        Price { input: 1.25, cache_write: 1.25, cache_read: 0.125, output: 10.0 }
    } else if m.contains("o4-mini") || m.contains("o4") {
        Price { input: 1.1, cache_write: 1.1, cache_read: 0.275, output: 4.4 }
    } else {
        Price { input: 0.0, cache_write: 0.0, cache_read: 0.0, output: 0.0 }
    }
}

#[derive(Clone, Copy, Default, Serialize, PartialEq, Debug)]
pub struct Tokens {
    pub input: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub output: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input + self.cache_write + self.cache_read + self.output
    }
    fn add(&mut self, o: &Tokens) {
        self.input += o.input;
        self.cache_write += o.cache_write;
        self.cache_read += o.cache_read;
        self.output += o.output;
    }
}

/// Cost in USD for `tokens` billed at `model`'s rates.
pub fn cost(model: &str, t: &Tokens) -> f64 {
    let p = price_for(model);
    (t.input as f64 * p.input
        + t.cache_write as f64 * p.cache_write
        + t.cache_read as f64 * p.cache_read
        + t.output as f64 * p.output)
        / 1_000_000.0
}

#[derive(Serialize)]
pub struct UsageRow {
    pub harness: String,
    pub model: String,
    pub tokens: Tokens,
    pub cost: f64,
}

/// Per-harness rollup with billing status (subscription vs API key).
#[derive(Serialize)]
pub struct HarnessRow {
    pub harness: String,
    pub tokens: u64,
    /// API-equivalent cost of this harness's tokens (what it would cost billed).
    pub cost: f64,
    pub plan: String,
    pub subscription: bool,
}

#[derive(Serialize)]
pub struct UsageReport {
    // Today (the statusbar ticker also reads today_cost / today_tokens).
    pub today_cost: f64,
    pub today_tokens: u64,
    pub today_input: u64,
    pub today_output: u64,
    pub week_cost: f64,
    pub sessions_today: u32,
    // Whole scan window (~30d), api-equivalent.
    pub total_cost: f64,
    pub total_tokens: u64,
    /// Tokens on subscription-covered harnesses (marginal $0).
    pub subscription_tokens: u64,
    /// Cost by (harness, model), window, sorted by cost desc.
    pub rows: Vec<UsageRow>,
    /// Per-harness rollup with subscription/API status.
    pub harnesses: Vec<HarnessRow>,
    pub api_priced: bool,
}

/// Running accumulator shared across harness scans.
#[derive(Default)]
struct Acc {
    by_model: HashMap<(String, String), Tokens>,
    today: Tokens,
    today_cost: f64,
    week_cost: f64,
    today_sids: HashSet<String>,
}

impl Acc {
    fn record(
        &mut self,
        harness: &str,
        model: &str,
        sid: &str,
        t: Tokens,
        is_today: bool,
        is_week: bool,
    ) {
        self.by_model
            .entry((harness.to_string(), model.to_string()))
            .or_default()
            .add(&t);
        if is_week {
            self.week_cost += cost(model, &t);
        }
        if is_today {
            self.today.add(&t);
            self.today_cost += cost(model, &t);
            if !sid.is_empty() {
                self.today_sids.insert(sid.to_string());
            }
        }
    }
}

pub fn scan(max_age_hours: f64, limit: usize) -> UsageReport {
    let mut acc = Acc::default();
    let today = Local::now().date_naive();
    let cutoff = now_secs() - max_age_hours * 3600.0;
    scan_claude(&mut acc, cutoff, limit, today);
    scan_codex(&mut acc, cutoff, limit, today);
    finalize(acc)
}

fn finalize(acc: Acc) -> UsageReport {
    // Cost by (harness, model), window.
    let mut rows: Vec<UsageRow> = acc
        .by_model
        .iter()
        .map(|((harness, model), tokens)| UsageRow {
            harness: harness.clone(),
            model: model.clone(),
            tokens: *tokens,
            cost: cost(model, tokens),
        })
        .collect();
    rows.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(Ordering::Equal));

    // Per-harness rollup (tokens + api-equivalent cost) + billing classification.
    let mut by_harness: HashMap<String, (Tokens, f64)> = HashMap::new();
    for ((harness, model), tokens) in &acc.by_model {
        let e = by_harness
            .entry(harness.clone())
            .or_insert((Tokens::default(), 0.0));
        e.0.add(tokens);
        e.1 += cost(model, tokens);
    }
    let mut harnesses: Vec<HarnessRow> = by_harness
        .into_iter()
        .map(|(harness, (tokens, c))| {
            let (subscription, plan) = crate::access::billing_mode(&harness);
            HarnessRow {
                harness,
                tokens: tokens.total(),
                cost: c,
                plan,
                subscription,
            }
        })
        .collect();
    harnesses.sort_by(|a, b| b.tokens.cmp(&a.tokens));

    let subscription_tokens: u64 = harnesses
        .iter()
        .filter(|h| h.subscription)
        .map(|h| h.tokens)
        .sum();
    let total_cost: f64 = rows.iter().map(|r| r.cost).sum();
    let total_tokens: u64 = rows.iter().map(|r| r.tokens.total()).sum();

    UsageReport {
        today_cost: acc.today_cost,
        today_tokens: acc.today.total(),
        today_input: acc.today.input,
        today_output: acc.today.output,
        week_cost: acc.week_cost,
        sessions_today: acc.today_sids.len() as u32,
        total_cost,
        total_tokens,
        subscription_tokens,
        rows,
        harnesses,
        api_priced: true,
    }
}

/// (is_today, is_within_last_7_days) for an ISO-8601 timestamp.
fn day_buckets(ts: Option<&str>, today: NaiveDate) -> (bool, bool) {
    match ts.and_then(|s| DateTime::parse_from_rfc3339(s).ok()) {
        Some(dt) => {
            let d = dt.with_timezone(&Local).date_naive();
            (d == today, d >= today - chrono::Duration::days(6))
        }
        None => (false, false),
    }
}

/// Pull the four token counts out of a claude `message.usage` object.
fn claude_tokens(usage: &Value) -> Tokens {
    let g = |k: &str| usage.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
    Tokens {
        input: g("input_tokens"),
        cache_write: g("cache_creation_input_tokens"),
        cache_read: g("cache_read_input_tokens"),
        output: g("output_tokens"),
    }
}

fn scan_claude(acc: &mut Acc, cutoff: f64, limit: usize, today: NaiveDate) {
    let pattern = format!("{}/.claude/projects/*/*.jsonl", home_dir());
    for path in recent_files(&pattern, cutoff, limit) {
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let v: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                continue;
            }
            let msg = match v.get("message") {
                Some(m) => m,
                None => continue,
            };
            let usage = match msg.get("usage") {
                Some(u) => u,
                None => continue,
            };
            let t = claude_tokens(usage);
            if t.total() == 0 {
                continue;
            }
            let model = msg
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or("claude")
                .to_string();
            let ts = v.get("timestamp").and_then(|t| t.as_str());
            let (today_msg, week_msg) = day_buckets(ts, today);
            let sid = v.get("sessionId").and_then(|s| s.as_str()).unwrap_or("");
            acc.record("claude code", &model, sid, t, today_msg, week_msg);
        }
    }
}

fn scan_codex(acc: &mut Acc, cutoff: f64, limit: usize, today: NaiveDate) {
    // Nested date dirs: ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl
    let pattern = format!("{}/.codex/sessions/*/*/*/*.jsonl", home_dir());
    for path in recent_files(&pattern, cutoff, limit) {
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        // total_token_usage is cumulative; keep the last one seen as the session
        // total. Model is best-effort; default to the common codex model.
        let mut last: Option<Tokens> = None;
        let mut model = "gpt-5-codex".to_string();
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let v: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if let Some(m) = find_str(&v, "model") {
                if !m.is_empty() {
                    model = m;
                }
            }
            if let Some(tot) = find_obj(&v, "total_token_usage") {
                last = Some(Tokens {
                    input: tot.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
                    cache_write: 0,
                    cache_read: tot
                        .get("cached_input_tokens")
                        .and_then(|x| x.as_u64())
                        .unwrap_or(0),
                    output: tot.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
                });
            }
        }
        if let Some(t) = last {
            if t.total() == 0 {
                continue;
            }
            let file_date = file_mtime(&path).and_then(|m| {
                chrono::DateTime::from_timestamp(m as i64, 0)
                    .map(|dt| dt.with_timezone(&Local).date_naive())
            });
            let (today_file, week_file) = match file_date {
                Some(d) => (d == today, d >= today - chrono::Duration::days(6)),
                None => (false, false),
            };
            let sid = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            acc.record("codex", &model, &sid, t, today_file, week_file);
        }
    }
}

/// Shallow search for a string field by key anywhere one level into `v`.
fn find_str(v: &Value, key: &str) -> Option<String> {
    if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
        return Some(s.to_string());
    }
    if let Some(obj) = v.as_object() {
        for (_, val) in obj {
            if let Some(s) = val.get(key).and_then(|x| x.as_str()) {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn find_obj<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    if let Some(o) = v.get(key) {
        return Some(o);
    }
    if let Some(obj) = v.as_object() {
        for (_, val) in obj {
            if let Some(o) = val.get(key) {
                return Some(o);
            }
        }
    }
    None
}

/// Files matching `pattern` with mtime ≥ cutoff, newest first, capped at `limit`.
fn recent_files(pattern: &str, cutoff: f64, limit: usize) -> Vec<PathBuf> {
    let mut files: Vec<(f64, PathBuf)> = glob::glob(pattern)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|p| {
            let m = file_mtime(&p)?;
            if m >= cutoff {
                Some((m, p))
            } else {
                None
            }
        })
        .collect();
    files.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
    files.truncate(limit);
    files.into_iter().map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_math_opus() {
        // 1M input + 1M output at opus rates = $15 + $75 = $90.
        let t = Tokens { input: 1_000_000, cache_write: 0, cache_read: 0, output: 1_000_000 };
        assert!((cost("claude-opus-4-8", &t) - 90.0).abs() < 1e-9);
    }

    #[test]
    fn cache_read_is_cheap() {
        // 1M cache-read at opus = $1.50; far below 1M fresh input ($15).
        let cr = Tokens { input: 0, cache_write: 0, cache_read: 1_000_000, output: 0 };
        let inp = Tokens { input: 1_000_000, cache_write: 0, cache_read: 0, output: 0 };
        assert!((cost("claude-opus-4-8", &cr) - 1.5).abs() < 1e-9);
        assert!(cost("claude-opus-4-8", &cr) < cost("claude-opus-4-8", &inp));
    }

    #[test]
    fn unknown_model_is_free_but_counts_tokens() {
        let t = Tokens { input: 1_000_000, cache_write: 0, cache_read: 0, output: 0 };
        assert_eq!(cost("some-future-model", &t), 0.0);
        assert_eq!(t.total(), 1_000_000);
    }

    #[test]
    fn claude_tokens_parse_and_bucket() {
        let usage: Value = serde_json::from_str(
            r#"{"input_tokens":7282,"cache_creation_input_tokens":3412,"cache_read_input_tokens":19944,"output_tokens":883}"#,
        )
        .unwrap();
        let t = claude_tokens(&usage);
        assert_eq!(t.input, 7282);
        assert_eq!(t.cache_write, 3412);
        assert_eq!(t.cache_read, 19944);
        assert_eq!(t.output, 883);
        assert_eq!(t.total(), 7282 + 3412 + 19944 + 883);

        let today = Local::now().date_naive();
        let now = Local::now().to_rfc3339();
        assert!(day_buckets(Some(&now), today).0);
        assert!(day_buckets(Some(&now), today).1);
        assert!(!day_buckets(Some("2020-01-01T00:00:00Z"), today).0);
        assert!(!day_buckets(Some("2020-01-01T00:00:00Z"), today).1);
        assert!(!day_buckets(None, today).0);
    }

    #[test]
    fn sonnet_cheaper_than_opus() {
        let t = Tokens { input: 500_000, cache_write: 0, cache_read: 0, output: 500_000 };
        assert!(cost("claude-sonnet-5", &t) < cost("claude-opus-4-8", &t));
    }
}
