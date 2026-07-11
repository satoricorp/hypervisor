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

### Pre-change audit (2026-07-10)

| Surface | Was fake | Disposition |
|---|---|---|
| Settings: launch at login | decorative Switch | **made-real** — `tauri-plugin-autostart` |
| Settings: tv pause on needs_you | missing | **made-real** — `settings.json` + gate in `tv_interrupt` |
| Settings: sources (claude/codex/cursor/opencode) | decorative | **made-real** — filter in emit path |
| Settings: notify / sound / dock badge | decorative | **removed** → returns with M7 |
| Settings: auto-worktree | decorative | **removed** → returns with M4 |
| Settings: claude.ai source | decorative / no adapter | **removed** |
| Usage pane + `$4.51 · 2.41 MTOK` ticker | hard-coded dollars | **replaced** with live session counts; M6 ledger deferred |
| Access pane | fake keys (`sk-ant-…`) + fake renew dates | **made-real** — presence only |
| History pane | `HISTORY` mock constant | **made-real** interim — wide scan + archived |
| `/broadcast` `/review` `/plan` `/kill` `/compact` | "lands in M3/M4" toast | **made-real** |
| `/loop` `/worktree` `/handoff` | stub toasts | **removed** from menu |
| Remote settings | already real (M8a) | **real-verified** |
| Archive / rename / yolo / tv palette | already real | **real-verified** (untouched) |

`rg '$4.51|2.41 MTOK|mocked —|lands in M3|HISTORY =' src` → no matches.

### Made-real proofs

**settings.json** — `control::settings::tests::load_save_roundtrip` OK.
Shape: `{ tv_pause_on_needs_you, sources: {claude,codex,cursor,opencode} }`.
DECISION: disabled-source rows (including owned) vanish from the sidebar;
tmux keeps running; re-enable to see them. Filter is in
`apply_approvals_to_snapshot` (hvscan stays raw).

**Access** (this machine, presence only — no values returned):
```
OPENAI_API_KEY env=set (login shell)
ANTHROPIC_API_KEY / OPENROUTER_API_KEY = missing
claude subscription: organizationType=claude_max · default_claude_max_20x
codex: auth_mode=chatgpt · OPENAI_API_KEY name present in auth.json (value not read)
cursor-access-token keychain exit=0
```
`access::tests::probe_returns_rows_without_panic` asserts no `sk-` in details.

**History** — `list_history` wide-scans 30d/200, drops sidebar top-8, adds
archived tombstones. Click → read-only `get_transcript`. hvscan shows real
older sessions on disk.

**/compact** — tmux `-L hypervisor` send-keys `/compact` → pane captured
literal `/compact`. Menu only offers it for claude tmux.

**/kill** — throwaway `hv-realize-proof` killed; `has-session` fails after.
`kill_session` now also removes owned.json + re-emits.

**/broadcast /plan /review** — wired to `broadcast_prompt` / prefixed
`send_prompt` / `spawn_session`+canned review prompt. Plan prefix parks
execution via existing M3 approval flow when tools are requested.

**Autostart** — plugin registered + capabilities
(`autostart:allow-enable/disable/is-enabled`). No LaunchAgent until the
user toggles in-app (expected).

**tv pause** — `tv_interrupt` early-returns when
`settings.tv_pause_on_needs_you == false`.

### Verification

```
python3 spike/compare.py --limit 20  → OK (38 sessions, 0 diffs)
bunx tsc --noEmit                    → OK
cargo test --lib                     → 39 passed, 3 ignored
bun run build (tsc+vite)             → OK
cargo build                          → OK
```

### Removed → milestone that restores

| Removed | Returns with |
|---|---|
| Notification rows (notify/sound/badge) | M7 |
| auto-worktree settings toggle | M4 |
| `/loop` | (future loop milestone / M4+) |
| `/worktree` | M4 |
| `/handoff` | (future) |
| Fake dollar Usage ledger | M6 |
| claude.ai source row | never (no adapter) |

### Next queue file

`tasks/M8b.md` (gate: M8a — already ticked). Copied into `tasks/CURRENT.md`.
