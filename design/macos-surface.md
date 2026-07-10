# The macOS surface (M7) — the app when its window is closed

For a glance-and-dispatch tool, this arguably *is* the product: most of the
day Hypervisor should cost zero screen space and still deliver the green-dot
promise. Three surfaces, one shared grammar.

Mockup: `design/mockup-menubar.html` — all three surfaces share live state;
approve in one, watch the others update.

## The grammar is universal

`a`/`b`… approves that pending letter · `N: <prompt>` steers session N ·
`status` prints the board. Identical on: the phone page (M8a), iMessage
(M8b), the menu bar dropdown input, and the ⌥Space bar. Letters are
short-lived queue ids for pending approvals; they are stable while visible
and never collide with session numbers. Learn it once, use it from anywhere.

## 1 · Menu bar item

- One dot + optional count: **red** (+n) = things need you, **yellow** =
  fleet working, **green** = all clear. The whole app status in 10 pixels.
- Click → dropdown (the phone triage page docked in the corner):
  - header: dots summary + yolo state
  - pending approvals: letter badge · command · Approve button
  - compact session rows (dot · n · title · age); click opens the main window
    focused on that session
  - footer: one grammar input + "⌥space anywhere" hint
- Implementation: tauri tray API (`TrayIconBuilder`) with a menu-bar-only
  mode (`ActivationPolicy::Accessory` when the main window is closed, so no
  dock icon lingers). The dropdown is a small chromeless window anchored to
  the tray icon, not a native NSMenu — we need real inputs and buttons.

## 2 · Notifications

- A permission request fires an actionable notification:
  `Hypervisor — A · 5 wants: Bash(stripe fixtures pull …)`
  - **Approve** button: resolves without opening anything.
  - **Reply…**: inline text field = deny-with-guidance (the reply routes as
    `N: <text>` and rejects the request).
- Done/stalled notifications are opt-in per event type (Settings), batched
  (≤1 per 30s per session) — same rules as the iMessage bridge.
- Implementation note: buttons + inline reply need
  `UNUserNotificationCenter` category actions (`UNTextInputNotificationAction`).
  The basic tauri notification plugin can't do this — plan for a small
  native layer (objc2) or the notification fork that exposes actions. If
  actions prove unreachable in v1, the notification opens the ⌥Space bar
  pre-filled instead — degrade, don't drop.

## 3 · ⌥Space command bar

- Global shortcut (tauri global-shortcut plugin) summons a centered
  chromeless window: board strip (dots + next pending letter), one input,
  transient output line. Esc dismisses. It is intentionally the same text
  interface as the phone.
- The bar is also the notification fallback target and the "quick prompt
  without switching windows" path — the desktop's like-button equivalent is
  typing a single letter here.

## Sequencing

M7 after M6 per PLAN.md. The grammar parser should be built once (backend,
shared by dropdown/HUD; M8a/M8b reuse it over HTTP/iMessage) — do not
implement it per-surface.
