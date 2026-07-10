# TV — productionize the PiP player (side-quest milestone)

**Prerequisite gate: M3 (approvals) must be ticked in PLAN.md.** The interrupt
lifecycle below depends on approval state existing. If M3 is unticked, stop
and report instead of building.

**You are building exactly one thing this session:** turning the TV prototype
(`src-tauri/src/tv.rs`, commits 797456d…215affe) into the real feature per
`design/tv.md`. The prototype already has: toggle via titlebar button + ⌘T,
hide-pauses/show-resumes (never destroy), drag-anywhere
(movableByWindowBackground), fill-the-player init script, interrupt strip via
eval, red-transition auto-pause from the store.

## What's left (this is the milestone)

1. **Interrupt lifecycle tied to real approvals.** When a session gains an
   `approval`, the strip shows it (already wired via tvOnRed — verify the
   detail text is the approval, not the stalled copy). When the approval
   resolves *anywhere* (desktop Tab, yolo, menu bar later), the backend evals
   strip-removal + `video.play()` into the tv window. Watch the snapshot for
   approval-cleared transitions in the same place tvOnRed lives — or better,
   move the transition watch into the Rust watcher so it works with the main
   window closed.
2. **Honesty constraint — the strip cannot approve.** The tv window hosts
   youtube.com (external URL) which has **no IPC access**, so buttons inside
   the strip can only do page-local things (resume). Do not attempt to wire
   an Approve button inside the strip; instead the strip copy points at the
   approval surfaces: `approve from hypervisor (⌘⇥) — resume ignores`.
   Record this constraint in Evidence; it's why M7's ⌥Space exists.
3. **Interrupt cooldown**: at most one auto-pause per 30s (design/tv.md) —
   a flaky session must not strobe the player. Keep state in the backend.
4. **Rounded corners, attempt with fallback.** Chromeless NSWindow corners
   are square. Attempt via objc on the ns_window (the drag flag in tv.rs
   shows the pattern): transparent window background + `contentView.wantsLayer
   = YES` + `layer.cornerRadius = 14` + `masksToBounds = YES`. If this fights
   WKWebView rendering, ship square and note it — square is acceptable,
   broken compositing is not.
5. **Position/size persistence**: save the tv window's frame on move/resize
   (window event listeners) into the app data dir (settings.json or its own
   file); restore on create.
6. **⌘K → `tv`** palette entry in the main window (same action as ⌘T).
7. **Settings rows** (Settings view already exists): `tv: pause when a
   session needs me` (default on — gates the auto-pause) and `tv: always on
   top` (default on).

## Definition of done

1. ⌘T / titlebar / ⌘K all toggle the same surviving window; a playing video
   resumes at its timestamp with the window at its remembered position after
   quit-and-relaunch of the app (position persists; the video/timestamp only
   needs to survive hide/show, not relaunch).
2. A real owned session hitting a pending approval while the tv plays: video
   pauses within 1s, strip shows the exact command. Approving from the main
   window (Tab) clears the strip and resumes playback without touching the
   tv window.
3. Two approval events 5s apart: the second one updates the strip text but
   does not re-fire the pause animation storm (cooldown holds).
4. Settings toggles verifiably change behavior (turn auto-pause off → no
   pause on the next approval).
5. `python3 spike/compare.py` OK · `bunx tsc --noEmit` · `cargo test --lib` ·
   `npm run tauri dev` boots.

## Scope fence

- No embed-URL wrapper page, no playlists, no media keys, no AirPlay.
- Main window CSP untouched (verify tauri.conf.json diff is empty).
- tmux/adapters untouched.

## When done

Evidence (cooldown demonstration, persistence proof, corner-radius outcome,
the IPC constraint note), tick the TV checkbox in PLAN.md, commit:
`TV: production PiP — approval-tied interrupts, cooldown, persistence`.

## Evidence

(builder fills this in — an empty Evidence section means the milestone is not done)
