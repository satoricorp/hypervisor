# H3 — surface failures + webview safety

**You are building exactly one thing this session:** the app stops hiding
its failures (spawn deaths, adapter degradation, the 8-session cap) and the
webview stops trusting raw HTML.

## Steps

1. **CSP.** `tauri.conf.json` `security.csp` is currently `null`. Set a
   real policy (`default-src 'self'` baseline; `style-src` inline only if
   the CSS genuinely needs it; the ipc/asset origins Tauri requires). The
   tv satellite window is remote-content by design and must still open —
   verify, don't touch its code.
2. **Structured toasts.** `ToastEvent` becomes
   `{ label: String, detail: Option<String> }` — no HTML across the wire,
   ever. Frontend `Toast` renders text nodes (bold label styled in the
   component, not via markup in the payload). Kill
   `dangerouslySetInnerHTML` in Toast.tsx and every `TOAST` dispatch that
   builds HTML strings (store.tsx). The `iconOf` innerHTML call sites
   (static local SVG constants) may stay. Rationale: approval text comes
   from pane captures — agent-influenced content — and today one forgotten
   `esc()` is an XSS with `invoke("approve_session")` reach.
3. **Spawn health.** ~2s after `/new` (and adopt), check the pane is alive
   (`tmux -L hypervisor has-session` / pane_dead). Dead → toast the last
   pane lines (e.g. `claude: command not found`) and remove the pending
   placeholder + owned entry. Today a failed spawn leaves a ghost
   "new session — xxxxxxxx" row forever and logs only to a terminal
   nobody is watching.
4. **Health line.** Statusbar gains a quiet segment fed by a `health`
   event: watcher alive · per-adapter last-scan ok/degraded · serve
   up/down. One glance replaces reading eprintln output.
5. **Sidebar overflow.** Wire gains `total`; when total > shown, the
   sidebar footer shows `+N more · not monitored` — honest about the
   `LIMIT = 8` cap.
6. **Subagents.** `wireToSession` always emits `subs: []`, so the h/l
   keyboard affordance is dead on live data. Populate claude code `subs`
   from sidechain rows (target/task/state — `spike/hvwatch.py` is the
   oracle). If the transcript data can't support it cleanly, write a
   `// DECISION:` comment explaining why, leave subs empty, and hide the
   h/l hint when a session has none. Either way, stop advertising a dead
   feature.

## Definition of done

1. CSP proof: a `<script src="https://example.com/x.js">` injected via
   devtools is blocked (console error captured in Evidence); the app is
   fully functional afterwards; the tv window still opens.
2. `grep -rn dangerouslySetInnerHTML src/` shows only `iconOf` call sites.
3. Spawning with the harness CLI unavailable (e.g. a PATH-stripped wrapper
   script) produces a visible toast naming the failure; no ghost
   placeholder or owned entry remains.
4. Kill `opencode serve` mid-run → the health segment flips within one
   tick; the rest of the sidebar keeps working.
5. With >8 recent sessions on disk, the footer shows the overflow count.
6. `python3 spike/compare.py` OK · `bunx tsc --noEmit` · `cargo test --lib`
   · `npm run tauri dev` boots.

## Scope fence

- No loop restructuring beyond emitting health (H2 owns the loop).
- No remote/grammar work. The mockup DOM stays the contract for everything
  not listed here.
- Adapters: only the sidechain-population change in step 6; compare.py
  must stay OK (it compares the fields it compares — extending subs data
  must not alter existing fields).

## When done

1. Evidence: CSP console error, the failed-spawn toast, health-line
   screenshot or DOM text, overflow footer proof.
2. Tick H3 in PLAN.md.
3. Refresh `tasks/CURRENT.md` with `tasks/M7g.md`.
4. Commit: `H3: CSP + structured toasts, spawn health, adapter health line, overflow honesty`.

## Evidence

(builder fills this in — an empty Evidence section means the milestone is not done)
