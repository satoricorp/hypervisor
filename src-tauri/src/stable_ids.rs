//! Process-lifetime stable session numbers + approval letters (M7g).
//!
//! Numbers are assigned on first sight of a sid (monotonic, never reused).
//! Letters are assigned on first sight of an approval identity (opencode
//! request id / tmux fingerprint), stable while pending, never reused.
//! Letters are A–Z only — they cannot collide with digit session numbers
//! by construction.

use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct StableIds {
    next_n: u32,
    by_sid: HashMap<String, u32>,
    next_letter: u8, // 0 → 'A'
    /// approval identity → letter
    by_approval: HashMap<String, char>,
    /// letter → sid (for grammar `a` / `b` lookup)
    letter_to_sid: HashMap<char, String>,
    /// sid → letter while that approval is live
    sid_to_letter: HashMap<String, char>,
}

impl StableIds {
    pub fn new() -> Self {
        Self {
            next_n: 1,
            ..Self::default()
        }
    }

    /// Stable session number for `sid` — assigned once, never changes.
    pub fn number_for(&mut self, sid: &str) -> u32 {
        if let Some(&n) = self.by_sid.get(sid) {
            return n;
        }
        let n = self.next_n;
        self.next_n = self.next_n.saturating_add(1);
        self.by_sid.insert(sid.to_string(), n);
        n
    }

    pub fn number_of(&self, sid: &str) -> Option<u32> {
        self.by_sid.get(sid).copied()
    }

    pub fn sid_for_number(&self, n: u32) -> Option<&str> {
        self.by_sid
            .iter()
            .find(|(_, &v)| v == n)
            .map(|(k, _)| k.as_str())
    }

    /// Ensure every sid has a number. When `sorted` is true, assign in
    /// lexicographic sid order (deterministic for headless `hvscan cmd`).
    pub fn ensure_sids<I, S>(&mut self, sids: I, sorted: bool)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut v: Vec<String> = sids
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        if sorted {
            v.sort();
            v.dedup();
        }
        for sid in v {
            let _ = self.number_for(&sid);
        }
    }

    /// Approval identity key (opencode request id / tmux fingerprint).
    pub fn letter_for(&mut self, identity: &str, sid: &str) -> Option<char> {
        if let Some(&c) = self.by_approval.get(identity) {
            self.letter_to_sid.insert(c, sid.to_string());
            self.sid_to_letter.insert(sid.to_string(), c);
            return Some(c);
        }
        if self.next_letter >= 26 {
            return None; // exhausted A–Z
        }
        let c = (b'A' + self.next_letter) as char;
        self.next_letter += 1;
        self.by_approval.insert(identity.to_string(), c);
        self.letter_to_sid.insert(c, sid.to_string());
        self.sid_to_letter.insert(sid.to_string(), c);
        Some(c)
    }

    pub fn letter_of_sid(&self, sid: &str) -> Option<char> {
        self.sid_to_letter.get(sid).copied()
    }

    pub fn sid_for_letter(&self, c: char) -> Option<&str> {
        self.letter_to_sid
            .get(&c.to_ascii_uppercase())
            .map(|s| s.as_str())
    }

    /// Drop live sid→letter links for approvals that cleared; letters stay
    /// reserved (never reused).
    pub fn sync_approvals(&mut self, live: &HashMap<String, String>) {
        // live: sid → identity
        self.sid_to_letter
            .retain(|sid, _| live.contains_key(sid));
        self.letter_to_sid
            .retain(|_, sid| live.contains_key(sid));
        for (sid, identity) in live {
            let _ = self.letter_for(identity, sid);
        }
    }
}

/// Build approval identity from source fields.
pub fn approval_identity(
    sid: &str,
    source: &crate::approvals::ApprovalSource,
    fingerprint: Option<&str>,
    text: &str,
) -> String {
    match source {
        crate::approvals::ApprovalSource::Opencode { request_id, .. } => {
            format!("oc:{request_id}")
        }
        crate::approvals::ApprovalSource::Tmux => {
            format!(
                "tmux:{sid}:{}",
                fingerprint.unwrap_or(text)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_stable_across_calls() {
        let mut ids = StableIds::new();
        let a = ids.number_for("sid-a");
        let b = ids.number_for("sid-b");
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(ids.number_for("sid-a"), 1);
        assert_ne!(a, b);
    }

    #[test]
    fn letters_never_reuse_and_are_alpha() {
        let mut ids = StableIds::new();
        let a = ids.letter_for("oc:1", "s1").unwrap();
        let b = ids.letter_for("oc:2", "s2").unwrap();
        assert_eq!(a, 'A');
        assert_eq!(b, 'B');
        // clear live link for A, assign new — must get C not A
        ids.sync_approvals(&HashMap::from([("s2".into(), "oc:2".into())]));
        let c = ids.letter_for("oc:3", "s3").unwrap();
        assert_eq!(c, 'C');
        assert!(a.is_ascii_alphabetic());
        assert!(a.is_ascii_uppercase());
    }

    #[test]
    fn letters_cannot_collide_with_numbers() {
        // Property: letters are A–Z; numbers are u32 ≥ 1. Disjoint by type.
        let mut ids = StableIds::new();
        let n = ids.number_for("x");
        let letter = ids.letter_for("oc:x", "x").unwrap();
        assert!(n >= 1);
        assert!(letter.is_ascii_alphabetic());
        // Grammar parse distinguishes digit vs alpha tokens — see grammar tests.
        assert!(letter.to_digit(10).is_none());
    }
}
