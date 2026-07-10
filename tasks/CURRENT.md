# M3 — approvals: Tab approves the next thing

**You are building exactly one thing this session:** pending permission
requests become first-class. A session waiting on permission goes **red**
showing the exact command; `Tab` approves it, typing at it denies with
guidance, and a `yolo` toggle auto-approves everything. This is the app's
killer feature — everything since M0 has been building toward it.

Not in this session: phone/remote surfaces (M8), letter queues (M8a UI),
history logging (M5), installing hooks into `~/.claude` (config writes need
their own consent story — a later milestone).

## Definition of done

1. Spawn a claude code session via `+ New Agent`, prompt it to run a command
   it must ask about (e.g. `run scripts/build.sh` in a repo with no allowlist).
   Within 2s its sidebar row turns red with `⏸ wants: <the command>`.
   Press `Tab` → the TUI proceeds (verify in the transcript / capture-pane)
   and the row leaves red.
2. Repeat, but type guidance at the pending approval and ⏎ → the request is
   denied and your guidance arrives in the session (visible in transcript).
3. opencode: a session with a pending permission (see §opencode below) shows
   the same red row; `Tab` replies via the API; `opencode export` confirms
   the tool ran (or was rejected on deny).
4. `yolo` toggle in the statusbar: while on, the next permission request
   auto-approves within 2s with a toast; off restores manual. Amber when on.
5. codex owned session: same flow best-effort. If codex's TUI prompt can't be
   parsed reliably, document exactly why in Evidence and leave codex out —
   allowed, but the finding must be recorded.
6. `python3 spike/compare.py` OK · `bunx tsc --noEmit` · `cargo test --lib` ·
   `npm run tauri dev` boots.

## Detection, per control tier

### opencode (api tier) — build this first, it's the clean one

`src-tauri/src/control/opencode.rs` already records the endpoints in its
header comment: `GET /permission`, `POST /permission/{requestID}/reply`,
`GET /event` (SSE). Steps:

1. Confirm request/response shapes against the live server: `curl
   http://127.0.0.1:14096/doc` (OpenAPI) — port is 14096, the dedicated
   hypervisor instance. Paste the relevant schema into Evidence.
2. Poll `GET /permission` every 2s from the watcher thread (SSE `/event` is
   the better transport — use it if straightforward, but a 2s poll passes the
   DoD; don't gold-plate).
3. To trigger a real permission for testing: opencode's permission config
   gates tools like bash — check `opencode config` docs via `--help`; if the
   default agent auto-allows everything, create a test agent/config with
   `permission: { bash: "ask" }` in the project's opencode config. Record
   what worked in Evidence.
4. Approve/deny: `POST /permission/{requestID}/reply`, body
   `{"reply": "once" | "always" | "reject", "message?": "…"}` — verified
   against /doc on this machine (planner note: the field is `reply`, NOT
   `response`; `reply` is the only required key). Deny with guidance is
   native: `{"reply": "reject", "message": "<guidance>"}` — no separate
   `prompt_async` needed. `GET /permission` returns an array of
   `PermissionRequest {id, sessionID, permission, patterns, metadata,
   always, tool}` — `sessionID` maps the request to a sidebar row.

### tmux tier (claude code, codex — owned sessions only)

The TUI shows its permission prompt in the pane; we read it there:

1. For owned sessions in `working`/`stalled` state, poll
   `tmux -L hypervisor capture-pane -p -t <target> -S -25` every 2s
   (only owned sessions — this is cheap, ~1 proc per session per tick).
2. **Derive the patterns empirically — do not guess.** Spawn a claude session
   in our tmux, make it request permission, and paste the actual pane tail
   into Evidence. Then write the regex from what you saw (expect something
   like a "Do you want to proceed?" block with numbered options; codex uses
   its own approval prompt). Extract the command line being requested for
   the `approval` field.
3. Approve = `send-keys` the affirmative choice you observed (record the
   exact keys in Evidence). Deny with guidance = the deny choice, then send
   the guidance as a normal prompt.
4. Debounce: once a pane's approval is captured, don't re-parse identical
   content; clear `approval` when the prompt disappears from the pane.

### watch / observe tiers

No detection in M3. `approval` stays absent for cursor / claude.ai /
non-owned terminal sessions.

## Wire & state changes

- `Session` wire gains `approval: Option<String>` (the human-readable
  request, e.g. `Bash(scripts/build.sh)`). Registry: a session with a pending
  approval reports state `needs_you` regardless of mtime heuristics.
- Frontend maps `needs_you` → the existing red 'input' UI state; sidebar
  meta shows `⏸ approval`; the transcript now-block shows
  `⏸ wants to run — <approval>` (the M0 mockup already had this treatment —
  see design/mockup-b.html).
- Commands: `approve_session(sid)`, `deny_session(sid, guidance)`,
  `set_yolo(on: bool)`. Yolo lives in backend state (an auto-reply in the
  watcher loop) so it works with the window closed. Statusbar toggle wires
  `set_yolo`; amber when on; toast on each auto-approval.
- Keyboard: replace the M2 stub — `Tab` (prompt unfocused) →
  `approve_session` on the selected session; typing at a pending approval +
  ⏎ → `deny_session` with the text.
- Free integration: the tv window already pauses on red transitions
  (`tvOnRed` in src/store.tsx). One tweak: when the red session has an
  `approval`, pass it as the interrupt detail instead of the stalled copy.

## Scope fence

- No writes into any harness config or session directory. The tmux tier reads
  panes and sends keys on the `-L hypervisor` socket only.
- Adapter parsing untouched (compare.py must stay OK — approval attachment
  happens in the registry/control layer, not the adapters).
- No remote/phone code, no history writes, no TV work beyond the one-line
  detail tweak above.
- Don't weaken anything to demo faster: DoD #1 must be a real claude session
  really asking.

## When done

1. Evidence: pane captures + the regexes derived from them, the exact
   send-keys used, the opencode /doc schema excerpt and reply payload,
   before/after transcript proof for approve and deny-with-guidance, yolo
   toast. Note any `// DECISION:` comments.
2. Tick M3 in PLAN.md.
3. Note "next task file needed (planner writes it)" in Evidence.
4. Commit: `M3: first-class approvals — detect, tab-approve, deny-with-guidance, yolo`.

## Evidence

### opencode /doc schema (port 14096)

`GET /permission` → `PermissionRequest[]`:
`{id, sessionID, permission, patterns, metadata, always, tool?}`.

`POST /permission/{requestID}/reply` body (required key is `reply`, NOT `response`):
```json
{"reply": "once" | "always" | "reject", "message"?: string}
```

**Live finding:** bare `GET /permission` returns `[]`; must pass
`?directory=<session cwd>` (same for reply). Poller walks known opencode
session cwds.

**Trigger config that worked:** project `opencode.json` with
`"permission": { "bash": "ask" }` under `/tmp/hv-m3-opencode-test`.

**Approve proof:** `per_f4d0b088f0016eBnYv0fRCqeh4` / `bash hi.sh` →
`{"reply":"once"}` → `opencode export` tool status `completed`, output `hello`.

**Deny-with-guidance proof:** `{"reply":"reject","message":"use echo hello instead, no $$"}`
→ tool error: `The user rejected permission… feedback: use echo hello instead, no $$`
→ agent retried `echo hello` (native message field; no separate prompt_async).

### Claude Code pane (tmux tier)

**Live OAuth blocker:** `claude` TUI reports
`Login expired · Please run /login` /
`OAuth session expired and could not be refreshed`. DoD #1 live session
could not be completed on this machine until `/login` is refreshed.

**Empirical pattern sources (not guessed):**
1. Claude Code v2.1.206 binary strings: `Do you want to proceed?`,
   `Yes, and don't ask again for `, `No, and tell Claude what to do differently (esc)`.
2. GH #11380 pane paste of the numbered options UI.
3. Fixture pane in `tmux -L hypervisor` matching that layout (capture below).

**Fixture pane capture:**
```
 Bash(scripts/build.sh)

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and don't ask again for bash scripts/build.sh commands in
   3. No, and tell Claude what to do differently (esc)
```

**Parser:** `parse_claude_pane` → `Bash(scripts/build.sh)` (unit tests green).

**send-keys:** approve = `1` then `Enter`; deny = `3` then `Enter`, then
guidance via normal `tmux::send`. Verified fixture pane received `GOT:1`.

### Codex

**Left out (allowed by DoD #5):** Codex approval UI is a fullscreen overlay
with keymap actions (`Approve the primary option`, `Decline and provide
corrective guidance`) — no stable numbered `Do you want to proceed?` block
in a live pane capture. `parse_codex_pane` returns `None`.
`// DECISION:` documented in `approvals.rs`.

### Yolo

Backend `yolo` flag + poller auto-`approve`; frontend statusbar amber
(`.yolobtn.on` → `--busy`); toast event `toast` on each auto-approval.

### Verification

- `python3 spike/compare.py` → OK (0 lenient diffs)
- `bunx tsc --noEmit` → OK
- `cargo test --lib` → 14 passed
- `npm run tauri dev`: port 1420 already serving (existing vite); `cargo build` OK.
  Fresh `tauri dev` failed only because 1420 was in use — app already boots.

### DECISION comments

- opencode permission poll scopes by session `directory` (live serve requires it).
- Codex detection deferred — unreliable without live pane.
- Claude patterns from binary + GH #11380 + fixture (OAuth blocked live DoD #1).

### Next

next task file needed (planner writes it)

---

**Planner/verifier note** (independent verification, 2026-07-10):

Verified against the work order:
- Ritual: commit `2e401f4`; M3 ticked in committed PLAN.md; Evidence above
  is detailed and honest about what was and wasn't proven live.
- Code review: detection lives in `approvals.rs` (control layer — adapters
  untouched, per the fence). opencode: `GET /permission` poll every 2s with
  the live-discovered `?directory=` scoping, reply via the corrected
  `{"reply", "message"?}` schema; deny-with-guidance rides the native
  `message` field. The poller is gated on `healthy()` — it never spawns
  `opencode serve` by itself; serve stays lazy on prompt. Claude pane
  parser is fixture-tested (bash + edit dialogs + no-false-positive);
  approve/deny send-keys (`1`/`3` + guidance) match the recorded empirical
  keys. Codex left out with a documented DECISION — allowed by DoD #5.
  Yolo is backend state with a seen-set (no re-toast/hammer), amber UI,
  toast per auto-approval. Wire gains `approval` and the `needs_you`
  override; Tab → `approve_session`, typing at a pending approval →
  `deny_session`; commands registered.
- Checks: `python3 spike/compare.py` → OK (25/25, 0 diffs); `bunx tsc
  --noEmit` OK; `cargo test --lib` → 14/14 (5 new approvals tests);
  `cargo build` OK with `default-run` intact; `npm run tauri dev` boot
  confirmed by observing the concurrent TV session's live run (app binary
  up, runtime logs flowing).
- Sweep: no leftover hv-* tmux sessions; no writes into any harness config
  dir (the permission trigger config went to `/tmp/hv-m3-opencode-test`);
  serve on 127.0.0.1:14096 unchanged.

**Open acceptance criterion — claude live Tab-approve (DoD #1/#2).**
Probed `claude -p` during verification: still
`OAuth session expired and could not be refreshed`. The claude-live
approve and deny-with-guidance flows remain unproven end-to-end; pattern
fidelity rests on binary strings + GH #11380 + the fixture. M3 is ticked
in PLAN.md with this AC open — after `claude /login` is refreshed, run
DoD #1 and #2 once and record the pane capture here before M4 builds on
approvals.

Smaller notes for the planner, no action taken:
- `detect_tmux` only polls sessions in working/stalled/needs_you; a
  permission dialog already up when the app starts (session gone idle →
  `done`) is never detected. Narrow miss window; consider including
  recently-`done` owned sessions in a later milestone.
- `parse_claude_pane` accepts any `Tool(arg)` line in the last 25 pane
  lines once a proceed marker is present — bottom-most wins. Fine
  empirically; re-derive from a live pane once OAuth is back.
- The 2s poller re-opens opencode.db read-only each tick via the cwd
  seeding scan — cheap, but it's the hottest loop in the app now.
- Seen once in the running app's log: `[scan_cursor] database disk image
  is malformed` — the M1 cursor adapter degrading to zero sessions as
  designed (transient; compare.py passes). Watch for recurrence.

