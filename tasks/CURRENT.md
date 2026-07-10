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

(builder fills this in — an empty Evidence section means the milestone is not done)
