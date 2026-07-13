//! Access view — key *presence* and subscription hints only.
//! Never reads or returns key/token values (principle 2).

use crate::adapters::home_dir;
use serde::Serialize;
use std::fs;
use std::process::Command;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AccessRow {
    pub label: String,
    pub kind: String,
    pub detail: String,
    pub present: bool,
}

/// Probe login-shell env for a var — exit/presence only, never the value.
fn env_present(name: &str) -> bool {
    let script = format!(
        r#"if [ -n "${{{}}}" ]; then echo 1; else echo 0; fi"#,
        name
    );
    let out = Command::new("/bin/zsh")
        .args(["-lic", &script])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim() == "1"
        }
        _ => false,
    }
}

/// `security find-generic-password` exit code only — never `-w` (no secret out).
fn keychain_present(service: &str) -> bool {
    Command::new("security")
        .args(["find-generic-password", "-s", service])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn claude_subscription() -> Option<AccessRow> {
    let path = format!("{}/.claude.json", home_dir());
    let data = fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&data).ok()?;
    let oa = v.get("oauthAccount")?;
    if !oa.is_object() {
        return None;
    }
    // Plan type strings only — never email / uuids / tokens.
    let org_type = oa
        .get("organizationType")
        .and_then(|x| x.as_str())
        .unwrap_or("claude");
    let tier = oa
        .get("organizationRateLimitTier")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let detail = if tier.is_empty() {
        org_type.to_string()
    } else {
        format!("{org_type} · {tier}")
    };
    Some(AccessRow {
        label: "claude subscription".into(),
        kind: "subscription".into(),
        detail,
        present: true,
    })
}

fn codex_subscription() -> Option<AccessRow> {
    let path = format!("{}/.codex/auth.json", home_dir());
    let data = fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&data).ok()?;
    let mode = v.get("auth_mode").and_then(|x| x.as_str())?;
    // chatgpt = ChatGPT subscription login; api_key = API-only.
    if mode == "chatgpt" {
        Some(AccessRow {
            label: "chatgpt / codex".into(),
            kind: "subscription".into(),
            detail: "auth_mode=chatgpt".into(),
            present: true,
        })
    } else if mode == "api_key" {
        Some(AccessRow {
            label: "codex api auth".into(),
            kind: "subscription".into(),
            detail: "auth_mode=api_key".into(),
            present: true,
        })
    } else {
        Some(AccessRow {
            label: "codex auth".into(),
            kind: "subscription".into(),
            detail: format!("auth_mode={mode}"),
            present: true,
        })
    }
}

/// Cursor: keychain token presence only (no value).
fn cursor_session() -> Option<AccessRow> {
    if keychain_present("cursor-access-token") {
        Some(AccessRow {
            label: "cursor session".into(),
            kind: "keychain".into(),
            detail: "cursor-access-token".into(),
            present: true,
        })
    } else {
        None
    }
}

/// M6v2: is `harness` on a subscription, and a short plan label for the ledger?
/// Reuses the same on-disk signals as the Access view (presence only).
pub fn billing_mode(harness: &str) -> (bool, String) {
    match harness {
        "claude code" | "claude" => match claude_subscription() {
            Some(r) => (true, claude_plan(&r.detail)),
            None => (false, "api key".into()),
        },
        "codex" => match codex_subscription() {
            Some(r) if r.detail.contains("chatgpt") => (true, "chatgpt".into()),
            _ => (false, "api key".into()),
        },
        "cursor" => (cursor_session().is_some(), "cursor".into()),
        "opencode" => (false, "byo key".into()),
        _ => (false, String::new()),
    }
}

/// tier "default_claude_max_20x" → "max 20x"; empty/plain → "max".
fn claude_plan(detail: &str) -> String {
    let tier = detail.rsplit('·').next().unwrap_or(detail).trim();
    let cleaned = tier
        .trim_start_matches("default_")
        .trim_start_matches("claude_")
        .replace('_', " ");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || cleaned == "claude" {
        "max".into()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod billing_tests {
    use super::claude_plan;
    #[test]
    fn claude_plan_is_clean() {
        assert_eq!(claude_plan("claude_max · default_claude_max_20x"), "max 20x");
        assert_eq!(claude_plan("claude"), "max");
    }
}

pub fn probe_access() -> Vec<AccessRow> {
    let mut rows = Vec::new();

    // API keys — presence only.
    let ant = env_present("ANTHROPIC_API_KEY");
    rows.push(AccessRow {
        label: "ANTHROPIC_API_KEY".into(),
        kind: "env".into(),
        detail: if ant {
            "login shell".into()
        } else {
            "not found".into()
        },
        present: ant,
    });

    let oai = env_present("OPENAI_API_KEY");
    // Presence of the key *name* in auth.json — never parse the value.
    let oai_file = {
        let path = format!("{}/.codex/auth.json", home_dir());
        fs::read_to_string(&path)
            .map(|d| d.contains("\"OPENAI_API_KEY\""))
            .unwrap_or(false)
    };
    let oai_present = oai || oai_file;
    rows.push(AccessRow {
        label: "OPENAI_API_KEY".into(),
        kind: if oai {
            "env".into()
        } else if oai_file {
            "codex auth.json".into()
        } else {
            "env".into()
        },
        detail: if oai_present {
            if oai {
                "login shell".into()
            } else {
                "key name present (value not read)".into()
            }
        } else {
            "not found".into()
        },
        present: oai_present,
    });

    let orouter = env_present("OPENROUTER_API_KEY");
    rows.push(AccessRow {
        label: "OPENROUTER_API_KEY".into(),
        kind: "env".into(),
        detail: if orouter {
            "login shell".into()
        } else {
            "not found".into()
        },
        present: orouter,
    });

    // Subscriptions — best-effort from on-disk config (no invented rows).
    if let Some(r) = claude_subscription() {
        rows.push(r);
    }
    if let Some(r) = codex_subscription() {
        rows.push(r);
    }
    if let Some(r) = cursor_session() {
        rows.push(r);
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_returns_rows_without_panic() {
        let rows = probe_access();
        // Always at least the three env probes.
        assert!(rows.len() >= 3);
        for r in &rows {
            // Never accidentally include a sk- prefix (key material).
            assert!(!r.detail.contains("sk-"));
            assert!(!r.detail.contains("sk-ant"));
        }
    }
}
