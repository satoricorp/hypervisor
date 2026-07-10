# M8a — remote: the tailnet phone page

**Prerequisite gate: M7g must be ticked in PLAN.md** (this milestone reuses
M7g's backend grammar parser, stable session ids, and approval/prompt
command layer — replanned 2026-07-10; the full M7 macOS surface now comes
later). If M7g is unticked, stop and report.

**You are building the phone triage page** per `design/remote.md` (spec,
§M8a) and `design/mockup-remote.html` (the left phone is the contract —
open it, press the APPROVE button, tap sessions, type `status`).

## Backend

1. **HTTP server bound to 127.0.0.1 only** (tiny_http or axum — pick the
   lighter one that does SSE cleanly; justify in a DECISION note). It must
   refuse to start on any other bind address by construction — no config
   option for it.
2. **Exposure is exclusively `tailscale serve`.** Settings shows the exact
   command (`tailscale serve --bg 127.0.0.1:<port>`) and whether tailscale is
   detected (`tailscale status --json`). Hypervisor never runs Funnel, never
   port-forwards.
3. **Auth on every request:** verify the `Tailscale-User-Login` header equals
   the login detected at setup. Missing/mismatched header → 403 with a plain
   explanation. This rejects direct localhost curl too — that's correct
   behavior, note it in Evidence, and gate a localhost dev bypass behind
   `HV_DEV=1` only.
4. Endpoints (JSON, all through the existing command layer — no second code
   path):
   - `GET /api/sessions` — the snapshot
   - `GET /api/events` — SSE re-broadcast of `sessions:update`
   - `POST /api/command {text}` — **the M7g grammar** (`grammar.rs`),
     verbatim: letters approve, `N: prompt`, `status`, `nudge N`
   - `POST /api/approve {letter}` / `POST /api/deny {letter, guidance}` —
     conveniences the buttons call; internally the same handlers
5. **No yolo over HTTP.** The endpoint does not exist remotely.
6. Every remote action → desktop toast + (when M5 lands) history line:
   `approved via remote · <login> · <time>`.
7. **Echo rule (design/remote.md §stable ids):** every command reply names
   the resolved target — `→ 3 · <title> — sent`. Numbers are the backend's
   stable ids from M7g, never list positions.
8. **Keep-awake:** while any owned session is `working`, hold a power
   assertion (a managed `caffeinate -dims` child is acceptable v1; release
   after 60s with none working) so approvals reach the phone with the lid
   closed. `// DECISION:` the mechanism.

## The page

One static HTML file served by the same server (embed via `include_str!`).
Port `design/mockup-remote.html`'s left phone faithfully:

- Header: wordmark + tailnet host + dots summary.
- The feed: tappable session board (tap → prefill `N: ` + focus input),
  pending approvals with red letter badges and the full command.
- **The APPROVE button** (the locked design, three Joe passes deep):
  compact ~60px, Xer0 "APPROVE" headline at 13px (ship the glyph as an
  inline SVG/woff subset — the phone won't have the font), NO letter badge
  on the button, **the full command wrapped beneath, never truncated**.
  Press: scale + green burst, **no feed echo** — the queue advancing and the
  later done-event are the confirmation.
- One input speaking the grammar; suggestions row (`status`).
- Live updates via the SSE stream; letters stay stable while visible.

## Definition of done

1. Phone on the tailnet loads the page over `tailscale serve`; a pending
   approval on the desktop appears on the phone within 2s (SSE, not refresh).
2. Tapping APPROVE on the phone unblocks the real session (transcript
   proof); the desktop shows the "via remote" toast.
3. `curl http://127.0.0.1:<port>/api/sessions` without the identity header →
   403. A device outside the tailnet cannot connect at all.
4. Typing `3: run the tests again` on the phone lands in session 3.
5. Deny path: tap the pending line → guidance flow denies with a message
   (verify in transcript).
6. Every reply on the phone echoes the resolved target (`→ 3 · <title>`);
   with the Mac's display asleep, a pending approval still reaches the
   phone and APPROVE unblocks it (power assertion proof).
7. compare.py OK · tsc · cargo test · tauri dev boots.

## Scope fence

- No iMessage (M8b — separate task file after this lands).
- No new UI framework for the page: hand-written HTML/CSS/JS like the mock.
- Bind address is not configurable. No Funnel. No yolo endpoint.
- Adapters untouched.

## When done

Evidence (curl 403 proof, phone screenshot or SSE timing note, transcript
proofs), tick M8a in PLAN.md, note "M8b task file needed (planner writes it)",
commit: `M8a: tailnet phone page — SSE triage, grammar over HTTP, APPROVE`.

## Evidence

(builder fills this in — an empty Evidence section means the milestone is not done)
