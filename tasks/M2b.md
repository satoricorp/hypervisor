# M2b — adoption: take control of sessions you didn't start

**You are building exactly one thing this session:** the ⏎-adopt flow. An
observe-tier claude code or codex session gets respawned under Hypervisor's
tmux via its harness's resume command — context preserved, control gained —
with a **fork guard** that refuses to adopt a session that may still be live
in another terminal.

Not in this session: opencode anything (M2c), approvals (M3), sqlite (M5).

## Why the fork guard exists (read this)

`claude --resume <sid>` on a session that another live process is still
writing creates a **fork**: two processes appending divergent histories to the
same conversation. The guard's job is to make adoption safe-by-default: if the
session file was written recently, the original process is probably alive, and
we refuse with a clear explanation instead of forking.

## Definition of done

1. Start `claude` in a plain terminal (no tmux), give it a prompt, let it
   finish and go idle for >60s. In the app: the session shows tier
   `observe-only`; select it, press ⏎ → toast confirms adoption,
   `tmux -L hypervisor ls` gains an `hv-*` session, the row's control chip
   flips to `⏻ runs in background`, and a prompt sent from the bar lands
   (dot yellow, last-sent updates).
2. Fork guard: repeat, but press ⏎ while the terminal session is actively
   generating (or within 60s of activity) → refused with a toast that states
   the idle time and why; `tmux -L hypervisor ls` unchanged.
3. Same as #1 for a codex session (`codex` in a terminal, then adopt).
4. `python3 spike/compare.py` still prints `OK`.
5. `npm run tauri dev` still boots. (Regression note: M1's second cargo binary
   broke this once — `default-run = "hypervisor"` in Cargo.toml fixed it.
   Don't remove it.)

## Backend steps

### 1. `src-tauri/src/control/adopt.rs`

```rust
#[tauri::command]
pub fn adopt_session(sid: String) -> Result<String, String>
```

Look the sid up in a fresh `registry::scan_sessions` snapshot to get
`harness`, `cwd`, `src` (file path), `mtime`. Then guards, in this order,
each with a human-readable error (they become toasts):

1. harness is `claude code` or `codex` — else
   `"cursor sessions are watch-only; claude.ai has no control path yet"`.
2. not already in owned.json — else `"already controlled by hypervisor"`.
3. **fork guard**: `idle = now - mtime`; if `idle < 60.0` →
   `"active {idle}s ago — it may still be open in another terminal. close it
   there, or let it go idle, then adopt."`

Adopt (reuse the M2 tmux helpers and the `/bin/zsh -lic` DECISION so PATH
resolves):

- claude code — the adapter sid **is** the full session uuid:
  `tmux -L hypervisor new-session -d -s hv-<id8> -c <cwd>
  "/bin/zsh -lic 'claude --resume <sid>'"`.
- codex — the adapter sid is only the **last 8 chars** of the rollout stem;
  `codex resume` needs the full uuid. Recover it from the `src` path: the
  filename is `rollout-<timestamp>-<uuid>.jsonl`, uuid = the final 36
  characters of the stem. Then `… "/bin/zsh -lic 'codex resume <uuid>'"`.
  (Verified on this machine: `codex resume [SESSION_ID]` resumes by uuid.)

On success: insert `{sid → hv-name}` into owned.json, save, and emit a fresh
`sessions:update` snapshot immediately so the control chip flips without
waiting for an fs event. Return the hv-name.

### 2. Known limitation — record, don't solve

After adoption, if the user goes back to the *original* terminal and types
there, the fork happens anyway. Detecting the original pty is out of scope;
note this under Evidence as a known limitation (a later milestone may add a
"last writer" indicator).

## Frontend steps

- The M0 UI already shows the observe placeholder ("⏎ adopts into hypervisor
  tmux…"). Replace the M2 stub toast with `invoke('adopt_session', { sid })`;
  toast the Ok value (`adopted as hv-x — session now runs in the background`)
  or the error string verbatim — the guard messages are written to be shown.
- No other UI changes. The tier chip updates by itself via the snapshot.

## Scope fence

- No opencode work at all (M2c exists for that). No approvals. No sqlite.
- `src-tauri/src/adapters/` untouched (compare.py must stay OK).
- tmux only on the `-L hypervisor` socket; the only file write is owned.json.
- Do not weaken the fork guard to make testing easier — wait the 60s.

## When done

1. Under **Evidence**: paste the adoption toast text, `tmux -L hypervisor ls`
   before/after, the fork-guard refusal text, and the codex adoption result.
   Note any `// DECISION:` comments and the known limitation above.
2. Tick M2b in `PLAN.md`.
3. Note "M2c task file needed" in Evidence.
4. Commit: `M2b: session adoption with live-writer fork guard`.

## Evidence

```
FORK_GUARD_REFUSAL:
active 22s ago — it may still be open in another terminal. close it there, or let it go idle, then adopt.

TMUX_BEFORE:
(no server / empty)

CLAUDE_ADOPT_TOAST: adopted as hv-2c056e72 — session now runs in the background
CLAUDE_ADOPT_SID: cf0a7d56-800d-4726-b9e8-e0e6304aa3eb
CLAUDE_PROMPT_AFTER_ADOPT: ok (send-keys to hv-2c056e72)

CODEX_ADOPT_TOAST: adopted as hv-52d0a237 — session now runs in the background
CODEX_ADOPT_SID: 369027be uuid=019f436d-586c-7490-af46-0b7e369027be

TMUX_AFTER:
hv-2c056e72: 1 windows (created Fri Jul 10 06:10:49 2026)
hv-52d0a237: 1 windows (created Fri Jul 10 06:10:49 2026)

$ python3 spike/compare.py
compared 18 sessions (18 python / 18 rust) · 0 lenient diffs
OK

$ cargo build --bin hypervisor   # ok (default-run = "hypervisor" intact)
$ bunx tsc --noEmit              # ok
$ cargo test --lib               # 3 passed
```

Verified via `control::adopt::tests::fork_guard_and_adopt_idle_sessions`
(same tmux helpers + resume commands as `adopt_session`; UI wires
`invoke('adopt_session')` and toasts Ok/Err verbatim).

`// DECISION:` comments:
- Adopt scan limit 64 (sidebar uses 8) so a visible row can't race out of the
  lookup window.
- `short_id` uses a process-local seq + low 32 bits so back-to-back adopts
  don't collide on `hv-*` names.
- Reuse `/bin/zsh -lic` (M2) so PATH resolves `claude`/`codex` on resume.

Known limitation: after adoption, if the user types in the *original*
terminal, a fork can still happen. Detecting the original pty is out of
scope; a later milestone may add a "last writer" indicator.

M2c task file needed.
