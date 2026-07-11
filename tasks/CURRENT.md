# PARITY — usable for claude code: dogfood until it feels native

**Prerequisite gate: ARCHIVE must be ticked in PLAN.md.** If unticked, stop
and report. (This task absorbs TITLES Part 2 — rename — so tasks/TITLES.md
is superseded by this file.)

**Your mission, and it is different from previous milestones:** use
Hypervisor to drive a real claude code session while you work, write down
every place it is worse than the native claude TUI, and close those gaps.
Joe's seed list: session renaming · short titles (derivation shipped —
verify it reads well in practice) · collapsible tool calls · thinking in
smaller gray text. The rest of the list is yours to discover by using it.

## The method (this is the contract)

1. **Dogfood loop.** Spawn a claude code session via Hypervisor and conduct
   a real task in it (building this very milestone works). Every time you
   reach for the terminal instead of Hypervisor, that's a finding — log it.
2. **Verify before building.** For every rendering feature, first prove the
   transcript actually carries the data: paste a real JSONL line into
   Evidence. **If the data isn't there, leave the feature out** and record
   why. No synthesized/faked content, ever.
3. **Iterate.** Fix → use again → next finding. Evidence's core artifact is
   the friction ledger: finding → data proof → fixed / left out (why).

## Known build items (verify each against real transcripts first)

### 1. A real transcript view

Today the detail pane shows only title/last-sent/last-reply/activity. Build
`get_transcript(sid, limit) -> Vec<TranscriptItem>` — a tauri command that
tail-parses the session's source file **on demand** (selected session only;
NOT in every snapshot — keep the hot loop untouched). Typed items to probe
for in claude JSONL:

- user text (role user, content text blocks — skip noise/meta as adapters do)
- assistant text
- **thinking** (assistant content blocks `type:"thinking"`, field
  `thinking`) — render smaller + `--ink-3` gray, clamp long blocks with
  expand-on-click
- **tool calls** (`type:"tool_use"`: name + input) paired with their results
  (user-role entries with `type:"tool_result"`, may be string or array,
  `is_error` flag) — render **collapsed to one line** `▸ ⚒ Bash(cargo test)`;
  click expands input + result in a scrollable monospace block. Errors get
  the red tint.

Refresh the open transcript when `sessions:update` fires for the selected
sid. Autoscroll pinned to bottom unless the user has scrolled up (the
standard chat rule — breaking it is the #1 way to make reading painful).

### 2. Rename (absorbed from TITLES Part 2)

`titles.json` override in the app data dir; `rename_session(sid, title)`
(empty or `-` clears); `/rename <title>` on the selected session with
trailing-text argument support in the `/` menu; override applied in the app
snapshot path only (hvscan/oracle stay raw); flows to sidebar, detail,
grammar echoes, phone page. Persists across restart.

### 3. Whatever the dogfooding surfaces

Candidates you'll likely hit (verify, then fix or log): tool-result noise
flooding `last_assistant`; interleaving order of thinking/tool/text;
multi-line prompts in the composer (shift+enter?); the sidebar age not
ticking without fs events; selected-row jumpiness while the fleet writes.
Budget most of your session here, not on the list above.

## Definition of done — the usability bar

1. **The exit test:** conduct one complete real task in a claude code
   session end-to-end from Hypervisor — prompt, watch thinking/tool calls
   live in the transcript view, approve at least one permission, read the
   result — without opening the terminal except to verify. Describe the task
   and the experience in Evidence.
2. Tool calls render collapsed and expand with real input/result content
   (screenshot or text proof from a real session).
3. Thinking blocks render small + gray when present; sessions without
   thinking render cleanly without placeholders.
4. `/rename` meets TITLES DoD #3/#4 (persists, flows everywhere, `-` reverts).
5. The friction ledger in Evidence has ≥ 8 entries, each resolved
   (fixed / left out with data proof).
6. `python3 spike/compare.py --limit 20` OK · `bunx tsc --noEmit` ·
   `cargo test --lib` · `npm run tauri dev` boots.

## Scope fence

- claude code is the bar. codex transcript view = best-effort if cheap;
  cursor/claude.ai unchanged. Do not degrade other harnesses' rows.
- `registry::scan_sessions` and the snapshot hot loop untouched
  (get_transcript is a separate on-demand path). Oracle stays green.
- No editor features (principle 4): read, approve, prompt — never edit code
  in Hypervisor.
- No LLM summarization (M5).

## When done

Evidence per the method; tick PARITY in PLAN.md; mark TITLES as superseded
(note in its file); name the next queue file. Commit:
`PARITY: transcript view, collapsible tools, gray thinking, rename — dogfooded`.

## Evidence

(builder fills this in — the friction ledger is the deliverable)
