# M7g — the grammar core: one language, stable ids, survives window close

**Prerequisite gate: H1 must be ticked in PLAN.md** (owned.json v2 carries
harness; tests are safe). M3x should be closed too — the grammar routes
into approvals and must stand on live-proven ground; if M3x is still open,
stop and report.

**You are building exactly one thing this session:** the backend command
grammar that every future surface shares (menu bar dropdown, ⌥Space, the
tailnet phone page, iMessage), plus the two properties it needs to be safe:
**stable session ids** and **a backend that outlives the window**. No tray,
no notifications, no HTTP server — those are M7/M8a.

This is the grammar work extracted from M7 (see design/macos-surface.md
"The grammar is universal") so remote doesn't wait on tray/notification
plumbing. PLAN.md's rule stands: the parser is built once, in the backend,
never per-surface.

## The grammar (from design/macos-surface.md, unchanged)

`status` → the board · `<letter>` → approve that pending request ·
`N: <text>` → prompt session N · `nudge N` → send "continue" to session N ·
anything else → one-line help. Case-insensitive, forgiving whitespace.
Deny stays what it already is: `N: <guidance>` at a session with a pending
approval denies with that guidance (same rule as the desktop prompt bar).

## Stable ids (the design change — spec in design/remote.md §stable ids)

Today session numbers are sidebar positions, resorted by mtime on every
update. Over an async channel, `3: yes go ahead` can hit the wrong agent
because the board moved after you read it. Change:

- The registry assigns each session a **stable number on first sight**
  (monotonic, never reused for the process lifetime). Wire gains `n`.
- The sidebar still sorts by mtime, but each row's keycap shows its stable
  `n`, and digit keys select by stable number — not by position. With >9
  sessions, numbers still display; digits cover 1–9 (j/k and ⌘K reach the
  rest).
- Approval **letters** (A, B, …) are assigned on detection, stable while
  pending, never reused in-process, and can never collide with numbers
  (letters vs digits — enforce by construction, add a test).
- DECISION latitude: if stable-number keycaps read badly in the sidebar,
  propose an alternative presentation in Evidence — but the wire/grammar
  semantics (stable `n`, stable letters) are non-negotiable.

## Steps

1. `src-tauri/src/grammar.rs`: a pure parser (`&str → Command` enum) and an
   executor that routes through the EXISTING handlers — `approve_session`,
   `deny_session`, `send_prompt` — no second code path. `status` formatter:
   `● 2 working · ● 1 done · ● 1 needs you` + one line per red
   (`A · 3 · <title> — wants: <command>`). Unit tests: every grammar arm,
   unknown input → help text, the letter/number non-collision property,
   formatter snapshot.
2. Stable ids in the registry/state layer: numbers keyed by sid, letters
   keyed by approval identity (opencode request id / tmux fingerprint).
   Survive snapshot churn; process-lifetime only (no persistence needed).
3. Window close ≠ quit: intercept `CloseRequested` → hide the window; the
   backend (watcher, tick, yolo) keeps running. ⌘Q / dock quit remain real
   exits. On real exit, owned tmux sessions deliberately survive — log a
   line naming any still `working`.
4. A proof harness so the grammar is exercisable before any transport
   exists: `hvscan cmd "<text>"` subcommand (preferred — headless,
   scriptable) or a temporary tauri command. `// DECISION:` the choice.

## Definition of done

1. Grammar unit tests green (arms, collision property, formatter).
2. `hvscan cmd "status"` prints the live board. `hvscan cmd "3: say hi"`
   lands in a real owned session (transcript proof). `hvscan cmd "a"`
   approves a real pending opencode permission (use the /tmp trigger
   config from tasks/M3.md Evidence).
3. Close the main window: the app keeps running; a permission request
   raised while the window is closed is still detected (log proof);
   reopening from the dock shows the window with live state.
4. Sidebar shows stable numbers; when a session finishes and the list
   re-sorts, no other session is renumbered; digit keys follow the stable
   numbers.
5. `python3 spike/compare.py` OK · `bunx tsc --noEmit` · `cargo test --lib`
   · `npm run tauri dev` boots.

## Scope fence

- No HTTP server, no tailscale, no chat.db, no tray icon, no global
  shortcut, no notifications, no power assertion (M8a takes keep-awake).
- Adapters untouched.

## When done

1. Evidence: test output, `hvscan cmd` transcripts, the window-closed
   detection proof, a before/after of the stable-number sidebar.
2. Tick M7g in PLAN.md.
3. Refresh `tasks/CURRENT.md` with `tasks/M8a.md` (its gate now points at
   M7g).
4. Commit: `M7g: shared command grammar, stable session ids, backend survives window close`.

## Evidence

(builder fills this in — an empty Evidence section means the milestone is not done)
