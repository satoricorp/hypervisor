//! Shared command grammar (M7g) — one parser for every surface.
//!
//! `status` · `<letter>` approve · `N: <text>` prompt/deny · `nudge N` · else help.

use crate::stable_ids::StableIds;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Status,
    ApproveLetter(char),
    /// Prompt session N; if that session has a pending approval, deny with text.
    Prompt { n: u32, text: String },
    Nudge(u32),
    Help,
}

pub const HELP: &str =
    "status · a/b… approves · N: <prompt> · nudge N";

/// Case-insensitive, forgiving whitespace.
pub fn parse(input: &str) -> Command {
    let s = input.trim();
    if s.is_empty() {
        return Command::Help;
    }
    let lower = s.to_ascii_lowercase();
    if lower == "status" || lower == "help" || lower == "?" {
        return if lower == "status" {
            Command::Status
        } else {
            Command::Help
        };
    }
    // nudge N
    if let Some(rest) = lower.strip_prefix("nudge") {
        let rest = rest.trim();
        if let Ok(n) = rest.parse::<u32>() {
            if n >= 1 {
                return Command::Nudge(n);
            }
        }
        return Command::Help;
    }
    // bare letter
    if s.len() == 1 {
        let c = s.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return Command::ApproveLetter(c.to_ascii_uppercase());
        }
    }
    // N: text
    if let Some((num, text)) = s.split_once(':') {
        let num = num.trim();
        if let Ok(n) = num.parse::<u32>() {
            if n >= 1 {
                return Command::Prompt {
                    n,
                    text: text.trim().to_string(),
                };
            }
        }
    }
    Command::Help
}

#[derive(Debug, Clone)]
pub struct BoardRow {
    pub n: u32,
    pub sid: String,
    pub title: String,
    pub state: String,
    pub approval: Option<String>,
    pub letter: Option<char>,
}

/// `● 2 working · ● 1 done · ● 1 needs you` + one line per red.
pub fn format_status(rows: &[BoardRow]) -> String {
    let mut working = 0u32;
    let mut done = 0u32;
    let mut needs = 0u32;
    for r in rows {
        match r.state.as_str() {
            "working" | "stalled" => working += 1,
            "needs_you" => needs += 1,
            _ => done += 1,
        }
    }
    let mut out = format!("● {working} working · ● {done} done · ● {needs} needs you");
    for r in rows {
        if let Some(ref approval) = r.approval {
            let letter = r
                .letter
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into());
            out.push('\n');
            out.push_str(&format!(
                "{letter} · {} · {} — wants: {approval}",
                r.n, r.title
            ));
        }
    }
    out
}

/// Resolve + describe an execute plan (no I/O). Used by tests + hvscan/app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    PrintStatus,
    Approve { sid: String, letter: char },
    Deny { sid: String, n: u32, guidance: String },
    Prompt { sid: String, n: u32, text: String },
    Nudge { sid: String, n: u32 },
    Help,
    Err(String),
}

pub fn plan(cmd: &Command, rows: &[BoardRow], ids: &StableIds) -> Action {
    match cmd {
        Command::Status => Action::PrintStatus,
        Command::Help => Action::Help,
        Command::ApproveLetter(c) => {
            let c = c.to_ascii_uppercase();
            match ids.sid_for_letter(c) {
                Some(sid) => Action::Approve {
                    sid: sid.to_string(),
                    letter: c,
                },
                None => Action::Err(format!("no pending approval for letter {c}")),
            }
        }
        Command::Nudge(n) => match ids.sid_for_number(*n) {
            Some(sid) => Action::Nudge {
                sid: sid.to_string(),
                n: *n,
            },
            None => Action::Err(format!("no session numbered {n}")),
        },
        Command::Prompt { n, text } => {
            let Some(sid) = ids.sid_for_number(*n) else {
                return Action::Err(format!("no session numbered {n}"));
            };
            let row = rows.iter().find(|r| r.sid == sid);
            if row.map(|r| r.approval.is_some()).unwrap_or(false) {
                Action::Deny {
                    sid: sid.to_string(),
                    n: *n,
                    guidance: text.clone(),
                }
            } else {
                Action::Prompt {
                    sid: sid.to_string(),
                    n: *n,
                    text: text.clone(),
                }
            }
        }
    }
}

pub fn echo_sent(n: u32, title: &str) -> String {
    format!("→ {n} · {title} — sent")
}

pub fn echo_approved(letter: char, n: u32, title: &str) -> String {
    format!("→ {letter} · {n} · {title} — approved")
}

pub fn echo_denied(n: u32, title: &str) -> String {
    format!("→ {n} · {title} — denied")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_arm() {
        assert_eq!(parse("status"), Command::Status);
        assert_eq!(parse(" STATUS "), Command::Status);
        assert_eq!(parse("a"), Command::ApproveLetter('A'));
        assert_eq!(parse("B"), Command::ApproveLetter('B'));
        assert_eq!(
            parse("3: say hi"),
            Command::Prompt {
                n: 3,
                text: "say hi".into()
            }
        );
        assert_eq!(
            parse("  12:  tighten  "),
            Command::Prompt {
                n: 12,
                text: "tighten".into()
            }
        );
        assert_eq!(parse("nudge 2"), Command::Nudge(2));
        assert_eq!(parse("Nudge  9"), Command::Nudge(9));
        assert_eq!(parse("xyz"), Command::Help);
        assert_eq!(parse(""), Command::Help);
    }

    #[test]
    fn letter_and_number_tokens_disjoint() {
        // Property: a single-char digit parses as Help (not ApproveLetter);
        // a single-char letter never parses as Prompt.
        for d in '1'..='9' {
            let cmd = parse(&d.to_string());
            assert!(
                !matches!(cmd, Command::ApproveLetter(_)),
                "digit {d} must not be an approve letter"
            );
        }
        for c in 'a'..='z' {
            assert_eq!(parse(&c.to_string()), Command::ApproveLetter(c.to_ascii_uppercase()));
            assert!(
                !matches!(parse(&c.to_string()), Command::Prompt { .. }),
                "letter must not be a prompt"
            );
        }
    }

    #[test]
    fn formatter_snapshot() {
        let rows = vec![
            BoardRow {
                n: 2,
                sid: "s2".into(),
                title: "fix flaky test".into(),
                state: "working".into(),
                approval: None,
                letter: None,
            },
            BoardRow {
                n: 3,
                sid: "s3".into(),
                title: "build script".into(),
                state: "needs_you".into(),
                approval: Some("Bash(./scripts_build.sh)".into()),
                letter: Some('A'),
            },
            BoardRow {
                n: 1,
                sid: "s1".into(),
                title: "done thing".into(),
                state: "done".into(),
                approval: None,
                letter: None,
            },
        ];
        let out = format_status(&rows);
        assert_eq!(
            out,
            "● 1 working · ● 1 done · ● 1 needs you\nA · 3 · build script — wants: Bash(./scripts_build.sh)"
        );
    }

    #[test]
    fn plan_prompt_vs_deny() {
        let mut ids = StableIds::new();
        let _ = ids.number_for("s1");
        let _ = ids.number_for("s2");
        let _ = ids.letter_for("oc:1", "s2");
        let rows = vec![
            BoardRow {
                n: 1,
                sid: "s1".into(),
                title: "ok".into(),
                state: "done".into(),
                approval: None,
                letter: None,
            },
            BoardRow {
                n: 2,
                sid: "s2".into(),
                title: "ask".into(),
                state: "needs_you".into(),
                approval: Some("Bash(x)".into()),
                letter: Some('A'),
            },
        ];
        assert_eq!(
            plan(&Command::Prompt { n: 1, text: "hi".into() }, &rows, &ids),
            Action::Prompt {
                sid: "s1".into(),
                n: 1,
                text: "hi".into()
            }
        );
        assert_eq!(
            plan(&Command::Prompt { n: 2, text: "nope".into() }, &rows, &ids),
            Action::Deny {
                sid: "s2".into(),
                n: 2,
                guidance: "nope".into()
            }
        );
        assert_eq!(
            plan(&Command::ApproveLetter('A'), &rows, &ids),
            Action::Approve {
                sid: "s2".into(),
                letter: 'A'
            }
        );
    }
}
