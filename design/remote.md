# Remote — work with sessions from your phone

Two channels, shipped as two milestones. Both exist so Joe can triage agents
away from the desk: see what needs him, approve/deny, and send prompts.

**Threat model first:** whoever controls a remote channel controls approvals —
i.e. arbitrary command execution on the Mac. Every design choice below follows
from that.

Mockup: `design/mockup-remote.html` (left phone = M8a web slice, right phone =
M8b iMessage).

## Stable ids & the echo rule (applies to both channels — added 2026-07-10)

Session numbers must be **stable ids assigned by the backend on first
sight** (built in M7g), not sidebar positions. Positional numbers reshuffle
every time any session writes a file; over an async channel the board you
read may be stale by the time you type `3: yes go ahead` — positional ids
would make that a roulette against arbitrary command execution.

- A number means the same session for the life of the app process. Sidebar
  order may change; numbers don't.
- Approval letters are assigned on detection, stable while pending, never
  reused in-process, and never collide with numbers.
- **Echo rule:** every remote reply names the resolved target
  (`→ 3 · fix flaky test — sent`) so a misroute is visible immediately.
  Both the phone page and the iMessage bridge follow it.

## M8a — Tailscale mobile slice (the backbone)

The trusted channel. Full remote control, including approvals.

### Architecture

- Tiny HTTP server inside the Tauri backend (axum or tiny_http), bound to
  **127.0.0.1** only. Exposure happens exclusively via `tailscale serve`
  (user runs it once; Settings shows the command + status). No listening on
  0.0.0.0, no port forwarding, no Funnel.
- Auth: `tailscale serve` injects `Tailscale-User-Login` on tailnet-internal
  requests. The server verifies it equals the configured login (detected via
  `tailscale status --json` at setup). Requests without the header (i.e. not
  proxied by tailscale) are rejected — belt for the suspenders.
- Endpoints (all JSON, mirroring existing tauri commands):
  - `GET  /api/sessions` — the same snapshot the desktop UI gets
  - `GET  /api/events` — SSE re-broadcast of `sessions:update`
  - `POST /api/approve {sid}` / `POST /api/deny {sid, guidance}`
  - `POST /api/prompt {sid, text}` — routes through the same send path
    (tmux/api tiers only; observe rows are read-only remotely — adoption
    stays a desktop decision)
- The mobile page: one static HTML file served from the same server. Same
  design tokens as the app, but **a text interface, not a shrunken
  dashboard** — phone UX is reading a feed and typing one line:
  1. **Tap a session → type → send** is the majority flow: the feed opens
     with a tappable board (one line per session); tapping prefills `N: ` and
     focuses the input.
  2. **Pending approvals get LETTERS** (A, B, …) — short-lived queue ids that
     never collide with session numbers. Reply a bare letter to approve.
     Deny = `N: <guidance>` to the session (clears its pending letter).
  3. **THE button**: a single large approve button pinned above the input,
     always showing the next pending letter + its exact command. Pressing it
     must feel like a like button — press-scale, green burst, advances to the
     next letter, collapses to a quiet "nothing needs you" state when the
     queue is empty. This is the one deliberate un-square element on mobile.
  4. One input speaking **the same grammar as the iMessage bridge**:
     `status` · `N: <prompt>` · bare letter approves · `nudge N`. One
     language, two transports.

### Rules

- **No remote yolo.** The toggle simply isn't rendered remotely.
- Every remote action produces a desktop toast + a line in the (M5) history
  log: `approved via remote · joe@… · 14:02`.
- If tailscale isn't running, Settings shows "remote: off (tailscale not
  detected)" — the server never falls back to a broader bind.

### AC sketch

Phone on tailnet loads the page; a pending approval on the desktop appears on
the phone within 2s; tapping Approve unblocks the real session; a prompt sent
from the phone lands in a tmux-owned session; a device outside the tailnet
gets connection refused; a curl without the identity header gets 403.

## M8b — iMessage bridge (the native trick)

Texting your Mac. Delightful, but Apple-fragile and identity-soft — so it is
**read-mostly by default**.

### Architecture

- **Inbound:** poll `~/Library/Messages/chat.db` (sqlite, read-only
  `immutable=1` — same technique as the Cursor adapter) every ~2s for new rows
  in `message` joined to `handle`/`chat`. Only messages from the **self-chat**
  (Messages "chat with yourself") are considered commands; sender allowlist =
  own handles only. Requires Full Disk Access: onboarding checks and degrades
  gracefully ("imessage bridge: needs Full Disk Access — off").
- **Outbound:** AppleScript via `osascript`: send to the self-chat. Plain
  text only, compact formatting (the dot board is unicode dots + counts).
- **Command grammar** (forgiving, case-insensitive — the M7g parser):
  - `status` → `● 2 working · ● 3 done · ● 1 needs you` + one line per red
  - `3: <text>` → prompt to session 3 (numbers are the stable backend ids —
    identical on the desktop sidebar and the phone page; see §stable ids)
  - `approve 5` / `deny 5 <guidance>` → **only if** approvals-over-imessage is
    explicitly enabled in Settings (default OFF; the reply otherwise says
    "approvals are disabled over imessage — use the tailnet page")
  - `help` → the grammar
- Unsolicited pushes (opt-in per event type): session done, session needs you,
  session stalled. Batched — never more than one text per 30s.

### Fragility ledger (accepted)

chat.db schema is undocumented and shifts with macOS releases; AppleScript
automation permission prompts on first send; FDA required. All wrapped
best-effort like the Cursor adapter: failure = feature off, never a crash.

### AC sketch

Text `status` from the phone → reply within 5s matching the desktop board.
Text `3: tighten the summary` → session 3 goes working, last-sent matches.
`approve 5` with the toggle off → refusal text; with it on → approval lands.
A text from a non-self handle is ignored (verified in Evidence).

## Sequencing

After M7 (menu bar + notifications) — M7 builds the "command from outside the
window" plumbing both channels reuse. Order: M8a, then M8b.
