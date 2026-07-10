# TV — YouTube in a satellite window that yields to your agents

A side-quest milestone (any time after M3). The premise: Hypervisor's whole
job is *waiting on agents* — so give the waiting a screen. The twist that
makes it more than a gimmick: **TV pauses itself when a session needs you.**

Mockup: `design/mockup-tv.html` (drive it: the interrupt button simulates a
session going red; Tab approves and resumes playback).

## Architecture — PiP, not a second app window

- A separate Tauri `WebviewWindow`, label `tv`, pointed at **youtube.com**
  (full site: login, subscriptions, no embed-blocking) — but styled as
  **picture-in-picture**: `decorations: false`, transparent window with CSS
  corner radius (~14px), `alwaysOnTop` default on, draggable via a drag
  region, remembers size/position, snaps to screen corners. Default ~380×214,
  min 300px. Controls (pin, close, progress, title) appear on hover with a
  dark scrim, native-PiP style — no persistent chrome.
- Launched from a **tv button in the main window's titlebar** (icon +
  `tv` label; mint when open). Also reachable via ⌘K → `tv`. Clicking again
  or the hover ✕ closes it.
- **The main window's CSP does not change.** All remote content lives in the
  satellite window. This is the whole reason it's a separate window.
- Pause/resume: the backend evals JS in the tv webview (Tauri window eval —
  our own window, not remote code injection):
  `document.querySelector('video')?.pause()` / `.play()`. Best-effort — if
  YouTube's DOM shifts, the interrupt still shows; the video just keeps
  playing (log, don't crash).

## The interrupt

On a session transitioning to `needs_you` (or a pending approval appearing),
if the tv window exists:

1. Pause the video (eval above).
2. Overlay a slim interrupt strip inside the tv window (injected element,
   same eval channel): `⏸ 5 needs you — Bash(stripe fixtures pull …)` with
   `⇥ approve · esc dismiss`. Tab routes to the same approve command the main
   window uses; either action resumes playback.
3. If the approval resolves elsewhere (desktop, phone), the strip clears and
   playback resumes on its own.

Interrupts respect a per-window cooldown (no more than one auto-pause per
30s) so a flaky session can't turn TV into a strobe.

## Settings

- `tv: pause when a session needs me` (default on)
- `tv: always on top` (default on)

## Scope fence for the eventual task file

No chromecast/airplay, no playlists, no watch-history features — it's a
webview with an interrupt strip, not a media center. No changes to the main
window CSP. If YouTube DOM-drift breaks pause, ship with the strip only.

## AC sketch

⌘K → tv opens YouTube in the satellite; while a video plays, a real session
hitting needs_you pauses it within 1s and shows the strip; Tab approves the
real session and playback resumes; main window CSP unchanged (verify
tauri.conf.json diff is empty).
