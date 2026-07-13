//! Content-free product analytics (tasks/POSTHOG.md).
//!
//! Capture from Rust only — main-window CSP stays zero-remote. Keys are baked
//! at compile time via `option_env!` (build.rs loads repo-root `.env` for
//! local staging; CI secrets for production releases). No key → inert.

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const QUEUE_CAP: usize = 256;
const FLUSH_SECS: u64 = 30;

/// Compile-time key/host from build.rs (`cargo:rustc-env`) or CI exports.
fn project_key() -> Option<&'static str> {
    option_env!("POSTHOG_PROJECT_KEY").filter(|s| !s.is_empty())
}

fn project_host() -> &'static str {
    option_env!("POSTHOG_HOST").unwrap_or("https://us.i.posthog.com")
}

pub fn configured() -> bool {
    project_key().is_some()
}

// ——— typed event schema (complete list — amend POSTHOG.md to add) ———

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnVia {
    New,
    Subagents,
}

impl SpawnVia {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Subagents => "subagents",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "subagents" => Self::Subagents,
            _ => Self::New,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalVia {
    Tab,
    Yolo,
    Remote,
    Notification,
}

impl ApprovalVia {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tab => "tab",
            Self::Yolo => "yolo",
            Self::Remote => "remote",
            Self::Notification => "notification",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Approve,
    Deny,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Deny => "deny",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptTier {
    Tmux,
    Api,
}

impl PromptTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tmux => "tmux",
            Self::Api => "api",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptVia {
    Desktop,
    Remote,
    Imessage,
}

impl PromptVia {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Remote => "remote",
            Self::Imessage => "imessage",
        }
    }
}

/// Slash-command names only — never arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandName {
    Rename,
    Broadcast,
    Compact,
    Archive,
    ArchiveIdle,
    New,
    Subagents,
    Kill,
    Yolo,
}

impl CommandName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rename => "/rename",
            Self::Broadcast => "/broadcast",
            Self::Compact => "/compact",
            Self::Archive => "/archive",
            Self::ArchiveIdle => "/archive-idle",
            Self::New => "/new",
            Self::Subagents => "/subagents",
            Self::Kill => "/kill",
            Self::Yolo => "/yolo",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HarnessCounts {
    pub claude: u32,
    pub codex: u32,
    pub cursor: u32,
    pub opencode: u32,
}

impl HarnessCounts {
    pub fn from_harness_labels<'a, I>(labels: I) -> Self
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut c = HarnessCounts {
            claude: 0,
            codex: 0,
            cursor: 0,
            opencode: 0,
        };
        for h in labels {
            match h {
                "claude code" | "claude" => c.claude += 1,
                "codex" => c.codex += 1,
                "cursor" => c.cursor += 1,
                "opencode" => c.opencode += 1,
                _ => {}
            }
        }
        c
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TelemetryEvent {
    AppOpened {
        version: String,
        harness_counts: HarnessCounts,
    },
    SessionSpawned {
        harness: String,
        via: SpawnVia,
    },
    SessionAdopted {
        harness: String,
    },
    ApprovalResolved {
        via: ApprovalVia,
        decision: Decision,
    },
    PromptSent {
        tier: PromptTier,
        via: PromptVia,
    },
    CommandUsed {
        name: CommandName,
    },
    TvToggled {
        on: bool,
    },
    SessionArchived {
        bulk: bool,
    },
    RemotePageOpened,
}

impl TelemetryEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::AppOpened { .. } => "app_opened",
            Self::SessionSpawned { .. } => "session_spawned",
            Self::SessionAdopted { .. } => "session_adopted",
            Self::ApprovalResolved { .. } => "approval_resolved",
            Self::PromptSent { .. } => "prompt_sent",
            Self::CommandUsed { .. } => "command_used",
            Self::TvToggled { .. } => "tv_toggled",
            Self::SessionArchived { .. } => "session_archived",
            Self::RemotePageOpened => "remote_page_opened",
        }
    }

    /// Properties are enums/counts only — no free-form user/session strings.
    pub fn properties(&self) -> Value {
        match self {
            Self::AppOpened {
                version,
                harness_counts,
            } => json!({
                "version": version,
                "harness_counts": harness_counts,
            }),
            Self::SessionSpawned { harness, via } => json!({
                "harness": harness_label(harness),
                "via": via.as_str(),
            }),
            Self::SessionAdopted { harness } => json!({
                "harness": harness_label(harness),
            }),
            Self::ApprovalResolved { via, decision } => json!({
                "via": via.as_str(),
                "decision": decision.as_str(),
            }),
            Self::PromptSent { tier, via } => json!({
                "tier": tier.as_str(),
                "via": via.as_str(),
            }),
            Self::CommandUsed { name } => json!({
                "name": name.as_str(),
            }),
            Self::TvToggled { on } => json!({ "on": on }),
            Self::SessionArchived { bulk } => json!({ "bulk": bulk }),
            Self::RemotePageOpened => json!({}),
        }
    }
}

/// Collapse harness synonyms to the four schema labels (never a free path/title).
fn harness_label(h: &str) -> &'static str {
    match h {
        "claude code" | "claude" => "claude",
        "codex" => "codex",
        "cursor" => "cursor",
        "opencode" => "opencode",
        _ => "other",
    }
}

struct Queued {
    event: String,
    properties: Value,
    ts_ms: u64,
}

pub struct Telemetry {
    enabled: AtomicBool,
    distinct_id: Mutex<String>,
    queue: Mutex<VecDeque<Queued>>,
}

impl Telemetry {
    pub fn new(distinct_id: String, analytics_on: bool) -> Arc<Self> {
        let t = Arc::new(Self {
            enabled: AtomicBool::new(analytics_on),
            distinct_id: Mutex::new(distinct_id),
            queue: Mutex::new(VecDeque::with_capacity(64)),
        });
        if configured() {
            let flusher = Arc::clone(&t);
            thread::Builder::new()
                .name("hv-telemetry".into())
                .spawn(move || flusher.flush_loop())
                .ok();
        }
        t
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        if !on {
            if let Ok(mut q) = self.queue.lock() {
                q.clear();
            }
        }
    }

    pub fn set_distinct_id(&self, id: String) {
        if let Ok(mut g) = self.distinct_id.lock() {
            *g = id;
        }
    }

    pub fn capture(&self, event: TelemetryEvent) {
        if !configured() || !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let item = Queued {
            event: event.name().to_string(),
            properties: event.properties(),
            ts_ms: now_ms(),
        };
        if let Ok(mut q) = self.queue.lock() {
            if q.len() >= QUEUE_CAP {
                q.pop_front();
            }
            q.push_back(item);
        }
    }

    fn flush_loop(self: Arc<Self>) {
        loop {
            thread::sleep(Duration::from_secs(FLUSH_SECS));
            self.flush_once();
        }
    }

    fn flush_once(&self) {
        let Some(api_key) = project_key() else {
            return;
        };
        let batch: Vec<Queued> = {
            let Ok(mut q) = self.queue.lock() else {
                return;
            };
            q.drain(..).collect()
        };
        if batch.is_empty() {
            return;
        }
        let distinct_id = self
            .distinct_id
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "unknown".into());

        let events: Vec<Value> = batch
            .iter()
            .map(|e| {
                json!({
                    "event": e.event,
                    "distinct_id": distinct_id,
                    "properties": e.properties,
                    "timestamp": iso_from_ms(e.ts_ms),
                })
            })
            .collect();

        let body = json!({
            "api_key": api_key,
            "batch": events,
        });

        let url = format!("{}/batch/", project_host().trim_end_matches('/'));
        if !post_json(&url, &body) {
            // One silent retry, then drop — never block the app.
            thread::sleep(Duration::from_millis(400));
            let _ = post_json(&url, &body);
        }
    }
}

fn post_json(url: &str, body: &Value) -> bool {
    let result = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build()
        .post(url)
        .set("Content-Type", "application/json")
        .send_json(body);
    match result {
        Ok(resp) => (200..300).contains(&resp.status()),
        Err(_) => false,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn iso_from_ms(ms: u64) -> String {
    // PostHog accepts ISO-8601; second precision is enough.
    let secs = ms / 1000;
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_else(|| format!("{secs}"))
}

/// Random UUID v4 — no hardware ids, username, or hostname.
pub fn new_distinct_id() -> String {
    let mut bytes = [0u8; 16];
    if let Ok(mut f) = File::open("/dev/urandom") {
        let _ = f.read_exact(&mut bytes);
    } else {
        let n = now_ms();
        bytes[..8].copy_from_slice(&n.to_le_bytes());
        bytes[8..].copy_from_slice(&(std::process::id() as u64).to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11],
        bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

/// Global handle set once at startup — capture sites use `capture(...)`.
static TELEMETRY: Mutex<Option<Arc<Telemetry>>> = Mutex::new(None);

pub fn install(t: Arc<Telemetry>) {
    if let Ok(mut g) = TELEMETRY.lock() {
        *g = Some(t);
    }
}

pub fn capture(event: TelemetryEvent) {
    if let Ok(g) = TELEMETRY.lock() {
        if let Some(t) = g.as_ref() {
            t.capture(event);
        }
    }
}

pub fn set_enabled(on: bool) {
    if let Ok(g) = TELEMETRY.lock() {
        if let Some(t) = g.as_ref() {
            t.set_enabled(on);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_are_enums_and_counts_only() {
        let events = [
            TelemetryEvent::AppOpened {
                version: "0.1.0".into(),
                harness_counts: HarnessCounts {
                    claude: 1,
                    codex: 0,
                    cursor: 2,
                    opencode: 0,
                },
            },
            TelemetryEvent::SessionSpawned {
                harness: "claude code".into(),
                via: SpawnVia::New,
            },
            TelemetryEvent::SessionAdopted {
                harness: "codex".into(),
            },
            TelemetryEvent::ApprovalResolved {
                via: ApprovalVia::Tab,
                decision: Decision::Approve,
            },
            TelemetryEvent::PromptSent {
                tier: PromptTier::Tmux,
                via: PromptVia::Desktop,
            },
            TelemetryEvent::CommandUsed {
                name: CommandName::Rename,
            },
            TelemetryEvent::TvToggled { on: true },
            TelemetryEvent::SessionArchived { bulk: false },
            TelemetryEvent::RemotePageOpened,
        ];
        for e in &events {
            let p = e.properties();
            // No property value may look like a path or multi-word user text.
            walk_no_content(&p);
            assert!(
                matches!(
                    e.name(),
                    "app_opened"
                        | "session_spawned"
                        | "session_adopted"
                        | "approval_resolved"
                        | "prompt_sent"
                        | "command_used"
                        | "tv_toggled"
                        | "session_archived"
                        | "remote_page_opened"
                ),
                "unknown event name {}",
                e.name()
            );
        }
    }

    fn walk_no_content(v: &Value) {
        match v {
            Value::String(s) => {
                // Slash commands (/rename) are allowed; absolute paths are not.
                assert!(
                    !s.starts_with("/Users")
                        && !s.starts_with("/home")
                        && !s.starts_with("~")
                        && !s.contains("://"),
                    "path/url leaked: {s}"
                );
                assert!(
                    s.starts_with('/') || !s.contains(' '),
                    "free text leaked: {s}"
                );
            }
            Value::Array(a) => a.iter().for_each(walk_no_content),
            Value::Object(m) => m.values().for_each(walk_no_content),
            _ => {}
        }
    }

    #[test]
    fn harness_counts_from_labels() {
        let c = HarnessCounts::from_harness_labels([
            "claude code",
            "codex",
            "cursor",
            "opencode",
            "claude",
        ]);
        assert_eq!(c.claude, 2);
        assert_eq!(c.codex, 1);
        assert_eq!(c.cursor, 1);
        assert_eq!(c.opencode, 1);
    }

    #[test]
    fn command_names_are_slash_tokens() {
        for n in [
            CommandName::Rename,
            CommandName::Broadcast,
            CommandName::Compact,
            CommandName::Archive,
            CommandName::ArchiveIdle,
        ] {
            assert!(n.as_str().starts_with('/'));
        }
    }

    #[test]
    fn capture_noop_when_disabled() {
        let t = Telemetry::new("test-id".into(), false);
        t.capture(TelemetryEvent::RemotePageOpened);
        assert!(t.queue.lock().unwrap().is_empty());
    }
}
