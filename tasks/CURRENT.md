# REALIZE — nothing mock stays in the app

**Prerequisite gate: PARITY must be ticked in PLAN.md.** If unticked, stop
and report.

**You are enforcing one rule everywhere:** every visible control either does
something real or it disappears. The rule for each mock: **verify a real
backing exists → build it; otherwise remove the fake and note it.** Removed
things return with their milestone (M5 history, M6 usage, M7 notifications).

Already real (don't redo): tv in the ⌘K palette (planner, 2026-07-10);
short titles; approvals; adoption; archive; rename (PARITY).

## The mock inventory (audit first — this list may have drifted)

Open every view and menu; list what's fake in Evidence before changing
anything. Known as of writing:

1. **Settings view** — all toggles are decorative.
   - `launch at login` → make real: tauri-plugin-autostart.
   - `tv: pause when a session needs me` → make real: gate the tvOnRed
     interrupt path on a persisted setting (settings.json in app data dir —
     create the tiny settings store; other rows reuse it).
   - `sources` toggles (claude/codex/cursor/opencode) → make real: a
     disabled source is skipped by the scan/watch loop (session rows vanish;
     document interaction with owned sessions).
   - Notification rows (`notify when done`, `sound`, dock badge) → **remove**
     (no notification system until M7; the rows return with it).
2. **Usage view** — mock tiles/bars. This is M6's ledger work. Either execute
   M6 here in full (per its PLAN bullet: tokens/cost from transcripts —
   claude + codex record usage fields; pricing table; subscription split) or,
   if it doesn't fit the session, **replace the pane with real-but-minimal**:
   live session counts by harness + "cost ledger lands with M6" — and say so.
   No fake dollar numbers may survive either way. The `$4.51 · 2.41 MTOK`
   ticker follows the same rule.
3. **Access view** — mock rows. Make real: detect key *presence* (env vars
   from a login-shell probe, `security find-generic-password` exit codes
   only — never read/store key material, principle 2), subscriptions
   best-effort (claude/codex config files indicate plan; verify what exists
   on disk, paste proof). Unverifiable rows are dropped, not invented.
4. **History view** — mock rows. Interim-real until M5: sessions from the
   unfiltered scan older than the sidebar window + archived tombstones,
   searchable. Clicking opens the PARITY transcript view read-only. Note
   "M5 replaces this with sqlite + summaries."
5. **Stub `/` commands** — audit which still toast "lands in M3/M4":
   - `/broadcast` → real: send the prompt to every controllable session
     (tmux + api tiers), echo per-target results.
   - `/review` → real: spawn a reviewer session (existing spawn path) in the
     parent's cwd with a canned review prompt.
   - `/plan` → real: prefix the prompt with a plan-first instruction; the
     approval flow (M3) already parks execution — verify the interaction.
   - `/kill` → real: tmux kill-session + owned.json cleanup, confirm toast.
   - `/compact` → real for claude tmux sessions: send-keys the literal
     `/compact` command; verify in the pane. Others: remove from menu.
   - `/loop`, `/worktree`, `/handoff` → **remove from the menu** (return
     with their milestones); killing a fake beats shipping a timer that can
     runaway-prompt an agent.

## Definition of done

1. A screen-by-screen audit in Evidence: every control listed as
   real-verified / made-real / removed. Zero fake data renders anywhere
   (grep the frontend for the mock constants and delete them).
2. Each "made real" item has a live proof (toggle autostart → check
   `osascript`/login items; disable codex source → its rows vanish;
   `/kill` → tmux session gone; `/compact` → pane shows compaction).
3. Settings persist across restart (settings.json).
4. `python3 spike/compare.py --limit 20` OK · `bunx tsc --noEmit` ·
   `cargo test --lib` · `npm run tauri dev` boots.

## Scope fence

- Principle 2 is absolute in Access: presence only, never values.
- No notification code (M7), no sqlite (M5), no LLM calls.
- Removing is success, not failure — record what was removed and which
  milestone restores it.

## When done

Evidence per above; tick REALIZE in PLAN.md; name the next queue file.
Commit: `REALIZE: every control real or removed — settings, access, history, commands`.

## Evidence

(builder fills this in)
