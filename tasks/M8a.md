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

### Decisions
- **HTTP:** `tiny_http` over axum — no tokio/tower; SSE is a keep-alive writer
  per connection. Bind hardcoded `127.0.0.1:7428` (SocketAddrV4 only).
- **Keep-awake:** managed `caffeinate -dims` child; hold while any *owned*
  session is `working`; release after 60s idle.

### Auth / 403
```
$ curl -sS -w 'HTTP %{http_code}\n' http://127.0.0.1:7428/api/sessions
tailscale not detected — remote auth has no login to match. …
HTTP 403
```
Direct localhost without `Tailscale-User-Login` is rejected (correct).
`HV_DEV=1` gates a localhost-only bypass for proofs (Tailscale.app not
installed on this machine — `/usr/local/bin/tailscale` wrapper points at a
missing binary). Settings shows `tailscale serve --bg 127.0.0.1:7428` and
"off (tailscale not detected)".

### Endpoints (HV_DEV=1)
- `GET /` → phone page (APPROVE + EventSource), Xer0 font at `/Xer0-Regular.otf`
- `GET /api/sessions` → snapshot with stable `n` / `letter`
- `GET /api/events` → SSE first event **15–19ms** (well under 2s)
- `POST /api/command {"text":"status"}` →
  `● 0 working · ● 7 done · ● 1 needs you\nA · 4 · … — wants: Bash(…)`
- `POST /api/approve {"letter":"A"}` →
  `→ A · 4 · m7g closed-window detection bash script — approved`
- `POST /api/deny {"letter":"A","guidance":"M8A deny — do not run that"}` →
  `→ 1 · m7g closed-window detection bash script — denied`
- `POST /api/command {"text":"1: reply with exactly M8A_PONG…"}` →
  `→ 1 · m7g closed-window detection bash script — sent`
- `POST /api/yolo` → **404** (no remote yolo)

### Echo rule
Every reply names the resolved target (`→ N · <title> — …`).

### Keep-awake
`remote::keepawake::tests::hold_spawns_caffeinate_release_kills` — hold
spawns `caffeinate`, release clears the child. Live owned+working cycle not
re-run this session (owned.json empty; api-tier working does not arm
keep-awake by design).

### Checks
- `python3 spike/compare.py` → OK (29 sessions, 0 diffs)
- `bunx tsc --noEmit` → OK
- `cargo test --lib` → **27 passed**, 3 ignored
- `./target/debug/hypervisor` boots; `[remote] listening on 127.0.0.1:7428`

### Blockers / follow-ups
- Tailscale.app not installed → cannot prove phone-over-`tailscale serve` or
  lid-closed path on a real MagicDNS host this session; auth + SSE + grammar
  proven on loopback with `HV_DEV=1`.
- **M8b task file needed (planner writes it).**

