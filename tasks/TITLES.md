# TITLES — real titles, not whole prompts, and renaming

**Prerequisite gate: ARCHIVE must be ticked in PLAN.md** (serialized so the
two small UX tasks don't collide in the same layers). If unticked, stop and
report.

**You are building exactly one thing this session:** sessions get short,
readable titles instead of the entire first prompt, and the user can rename
any session.

## Part 1 — derive better titles (adapters + spike in lockstep)

This is the one sanctioned exception to "adapters untouched": title
derivation changes in the Rust adapters **and `spike/hvwatch.py`
identically, in the same commit**, so `python3 spike/compare.py` stays OK.
The oracle diffs titles strictly — it is your tripwire that both sides moved
together.

Per harness:

- **claude code**: transcripts contain `{"type":"summary","summary":"…"}`
  entries — Claude Code's own generated short title for the conversation.
  Prefer the **latest** summary entry as the title. Fallback when absent:
  the **first line** of the first real user text, clipped to 48 chars at a
  word boundary (`…`).
- **codex**: check whether recent rollouts carry a session title in
  `session_meta` (inspect a real file; record the finding). If not: first
  line of first real user text, 48-char word-boundary clip.
- **opencode** (db `title`) and **cursor** (composer `name`): already short
  and harness-generated — unchanged.

## Part 2 — rename (local override)

- `titles.json` in the app data dir: `sid → custom_title` (same load/save
  pattern as owned.json/archived.json). **Never a harness-file write.**
- Override is applied in the app's snapshot path, **not** in
  `registry::scan_sessions` — `hvscan` and the oracle keep seeing derived
  titles (same rule as ARCHIVE's filter).
- `#[tauri::command] rename_session(sid, title) -> Result<String, String>`:
  empty title clears the override; returns the toast text.
- `/rename <new title>` in the `/` menu, acting on the selected session.
  The current menu is a picker — extend the root chooser so a command can
  carry trailing text as its argument (`/rename tighten auth work`).
  `/rename` with no argument → toast `usage: /rename <title> · "/rename -"
  reverts to the derived title`; `/rename -` clears.
- Renames flow everywhere titles appear: sidebar, detail header, grammar
  echoes (`→ 3 · <renamed> — sent`), the phone page. No extra plumbing
  should be needed if the override is applied at snapshot assembly — verify.

## Definition of done

1. A real claude code session that has a summary entry shows that short
   title in the sidebar (find one in ~/.claude/projects and name it in
   Evidence).
2. A session whose transcript lacks a summary shows the clipped first line
   (≤ 48 chars, ellipsis, no mid-word cut).
3. `/rename payments spike` on the selected session: title changes in the
   sidebar + detail immediately, appears in a grammar echo, survives app
   restart, and shows on the phone page.
4. `/rename -` reverts to the derived title.
5. `python3 spike/compare.py` → OK (proving spike + adapters moved in
   lockstep) · `bunx tsc --noEmit` · `cargo test --lib` (add a clip-function
   test: word boundary, unicode safety) · `npm run tauri dev` boots.

## Scope fence

- No LLM summarization (that's M5's summarize.rs — this is derivation +
  override only).
- No harness-file writes. Override in app snapshot path only.
- Don't touch archive/approval logic beyond reading beside them.

## When done

Evidence (the real summary-title example, before/after sidebar shots or
text, the codex session_meta finding, restart-persistence proof), tick
TITLES in PLAN.md, note the next queue file, commit:
`TITLES: harness-derived short titles + local rename overrides`.

## Evidence

(builder fills this in — an empty Evidence section means the milestone is not done)

---

**Planner note (2026-07-10):** Part 1 (derivation) shipped ahead of the gate
by the planner — Joe hit raw-prompt titles twice while actively testing.
Findings: NO `type:"summary"` entries exist in any transcript on this machine
(compaction-only), so the summary preference is future-proofing and the
first-line 48-char word-boundary clip carries the feature; codex session_meta
has no title field (checked live). Adapters + spike moved in lockstep;
compare.py OK at 41/41 (an initial 2v2 mismatch was per-harness limit churn
on a busy machine — two live agents writing during the run — not divergence).
**This task is now Part 2 only: the rename override + /rename.** Same gate.
