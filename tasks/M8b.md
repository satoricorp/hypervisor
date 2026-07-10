# M8b — remote: the iMessage bridge

**Prerequisite gate: M8a must be ticked in PLAN.md** (this reuses M8a's
grammar executor, stable ids, and command layer — one language, two
transports). If M8a is unticked, stop and report.

**You are building exactly one thing this session:** texting your Mac.
Poll the Messages self-chat for commands in the M7g grammar, act through the
**same** executor the tailnet page uses, and reply over iMessage. It is
**read-mostly and approvals-OFF by default** — the identity is soft, so the
one thing that runs code (approve) is gated behind an explicit Settings
toggle.

Spec: `design/remote.md` §M8b. Mockup: the **right** phone in
`design/mockup-remote.html`.

Not in this session: no new remote transport beyond iMessage, no yolo
anywhere, no M5 history writes (leave a TODO where the history line would
go), no changes to the M8a HTTP server or page.

## Honesty up front — two hard gates on this machine (2026-07-10)

1. **Full Disk Access.** `~/Library/Messages/chat.db` exists (227 MB) but
   reading it returned `authorization denied` from a plain shell — the
   process needs FDA. The DoD's live steps cannot pass until FDA is granted
   to the binary running the bridge (or to the terminal, for `hvscan`
   proofs). This is M8b's equivalent of M8a's missing Tailscale.app: build
   the code, gate cleanly, and record exactly what could and couldn't be
   proven live.
2. **Schema not yet explored.** Because of #1 I could not read the live
   schema. The landmarks below are the well-known chat.db shape, but you
   **must verify them against the live db before trusting them** and paste
   the real queries into Evidence. Do not ship a fabricated schema.

Both failures must degrade to "imessage bridge: off — needs Full Disk
Access", never a crash (wrap like the Cursor adapter).

## Inbound — poll the self-chat

`src-tauri/src/remote/imessage.rs`.

### Opening chat.db (WATCH THE WAL TRAP)

chat.db is **WAL and live-written by Messages**. The Cursor adapter opens
its db `mode=ro&immutable=1` — but M2c (opencode.db, also WAL) proved
`immutable=1` freezes a stale snapshot and **drops the newest rows**, which
here means your just-sent command is invisible. Open **`mode=ro` only** (no
`immutable=1`), `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI`, and leave a
`// DECISION:` citing the opencode lesson. Failure → bridge off.

### Schema landmarks (VERIFY LIVE, then paste real queries into Evidence)

- `message`: `ROWID` (monotonic — your poll watermark), `text` (**often
  NULL on modern macOS** — the body lives in `attributedBody`, a serialized
  NSAttributedString blob; you may need to decode it or fall back), `date`
  (**Mac absolute time: nanoseconds since 2001-01-01 UTC** on current macOS,
  seconds on ancient ones — convert, don't assume), `is_from_me`,
  `handle_id`.
- `handle`: `ROWID`, `id` (the address — phone/email/Apple ID).
- `chat`, `chat_message_join`, `chat_handle_join`: identify the **self-chat**
  (the "chat with yourself" / Note-to-Self thread whose only participant is
  your own handle). Record the exact predicate you used to isolate it.

### Command selection (self-chat + own handle only)

Only messages in the **self-chat** authored by you count as commands.
Practically: `is_from_me = 1`, in the self-chat, `ROWID > watermark`. Get the
own-handle allowlist from the account (verify live — Messages/`handle`);
anything outside it is ignored. **DoD #4 requires proving a non-self message
is ignored** — record how you simulated/verified it.

### The self-reply loop guard (do not skip)

Your outbound replies land in the same self-chat as `is_from_me = 1` and
will be read on the next poll — parsing to `Help` and replying to your own
reply, forever. Prevent it: advance the watermark **past your own sends**
(re-read max ROWID after each reply, or record sent ROWIDs and skip them).
State the mechanism in a `// DECISION:`. Add a test that a reply text fed
back in does not produce another outbound.

### Poll loop

Every ~2s (reuse the approval-poller cadence; a dedicated thread is fine).
Read messages `ROWID > watermark`, oldest-first, advance watermark, and for
each: `grammar::parse` → gate → execute → reply.

## Grammar + the approvals gate (the security boundary)

**Reuse the M7g grammar verbatim** — `grammar::parse` / `grammar::plan` — and
the M8a executor. The path already exists in `remote/mod.rs`:
`run_command` → `parse` → `plan` → `execute_action` calling
`events::{approve_sid, deny_sid, prompt_sid, current_sessions, ids_snapshot}`.
`execute_action`/`run_command` are private; make them `pub(crate)` (or lift
to a shared `remote::exec` module) and call them from the bridge. **No
second command code path** — that is the whole point of M7g.

`// DECISION:` (flag for Joe): the shared grammar approves by **bare
letter**, so M8b speaks that, **not** the literal `approve 5` / `deny 5` in
design/remote.md §M8b — that text predates the M7g extraction. One language,
two transports wins; note the divergence in Evidence.

**The gate.** Add a Settings flag `imessage_approvals` (default **false**).
Wrap the executor: when the flag is off and the planned `Action` is
`Approve` **or** `Deny`, do not execute — reply verbatim
`"approvals are disabled over imessage — use the tailnet page"`. `Status`,
`Prompt`, `Nudge`, `Help` always run. Note precisely: the only code-execution
path is `Action::Approve` (bare letter → resume the held command); `Prompt`
and `Nudge` inject agent text but never release a held permission, and
`N: <text>` to a session **with** a pending approval resolves to `Deny`
(refuses the tool) — so gating `Approve` is what actually blocks remote
command execution. Gate `Deny` too, per the design's stated intent.

Every executed remote action already emits a desktop toast via
`toast_remote` — pass an iMessage identity string as the `login` arg so the
toast reads `… via remote · imessage`. Leave a `// TODO(M5):` where the
history line will go.

## Outbound — osascript to the self-chat

`osascript` sending to Messages. Plain text only; the dot board is unicode
dots + counts (already produced by `grammar::format_status`). First send
triggers a macOS **Automation** TCC prompt (I saw it block a System-Events
call during planning) — onboarding must surface that; a blocked send →
bridge degrades, never crashes. Record the exact AppleScript you shipped.

## Unsolicited pushes (opt-in, batched)

Per-event-type opt-in (session done / needs_you / stalled), default off.
**Never more than one text per 30s** — batch. Drive off the same
`sessions:update` the desktop already emits; do not add a second watcher.
Keep this minimal — the request/reply path is the milestone; pushes are the
garnish.

## Settings (there is no settings store yet)

The app persists only `owned.json` in `app_data_dir` — no settings file
exists. Add a minimal one: `app_data_dir/settings.json`, a small serde
struct with `imessage_bridge_enabled: bool` (default false),
`imessage_approvals: bool` (default false), and the push opt-ins. Tauri
commands to read/set it, and **minimal** UI (a couple of toggles in the
existing statusbar/settings surface) — not a settings framework. The bridge
only polls while `imessage_bridge_enabled` is on.

## Definition of done

1. With the bridge enabled and FDA granted: text `status` from the phone →
   reply within 5s matching the desktop board (dot counts + one line per
   red).
2. Text `3: <prompt>` → session 3 (stable backend id, same number the
   desktop sidebar shows) goes working; its last-sent matches; reply echoes
   `→ 3 · <title> — sent`.
3. `imessage_approvals` OFF: a bare-letter approve → refusal text, the held
   command does **not** run (verify the session stays blocked). Toggle ON:
   the same letter approves and the real command proceeds (transcript
   proof).
4. A message from a non-self handle is ignored (record how verified). The
   self-reply loop does not fire (bridge does not answer its own replies).
5. `python3 spike/compare.py` OK · `bunx tsc --noEmit` · `cargo test --lib`
   (grammar/gate/loop-guard/date-conversion unit tests, no live DB needed)
   · `npm run tauri dev` boots.

If FDA can't be granted this session, #1–#4 stay unproven: implement fully,
unit-test everything that doesn't need the live DB, and record the precise
blocker + the manual steps to finish — exactly as M8a did for Tailscale.

## Scope fence

- **Adapters untouched** (compare.py is the tripwire). chat.db is read
  **read-only, `mode=ro`, never `immutable=1`**; the only writes are
  `settings.json` in app_data_dir and outbound iMessages via osascript.
- No second grammar or command path — reuse `grammar` + the M8a executor.
- No remote yolo, ever. Approvals gated OFF by default.
- No M5 history writes (TODO marker only). No changes to the M8a HTTP
  server, its page, or the desktop keyboard.

## When done

1. Evidence: the **live-verified** chat.db queries (self-chat predicate,
   new-message poll, date conversion, attributedBody handling), the exact
   AppleScript, the gate refusal text, approve/deny transcript proof (or the
   FDA blocker + manual steps if unproven), the non-self-ignored and
   loop-guard proofs, and all `// DECISION:` notes. State plainly what was
   proven live vs. code-only.
2. Tick M8b in PLAN.md.
3. Note "M8b is the last remote milestone; next build target is M4
   (worktrees) — planner writes the task file."
4. Commit: `M8b: iMessage bridge — self-chat grammar, approvals gated off by default`.

## Evidence

```
(builder fills this in — empty Evidence means the milestone is not done)
```
