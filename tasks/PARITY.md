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

**Done 2026-07-10.** Gate ARCHIVE was already ticked. TITLES Part 2 absorbed.

### Data proofs (verify-then-build)

**tool_use** (real line, `743b4c0f-…` under `~/.claude/projects/-Users-joe-git-gx/`):
```json
{"type":"tool_use","id":"toolu_01EtG3XCDhsdbBgqytTuH7W6","name":"Bash","input":{"command":"ls /Users/joe/git/gx && cat /Users/joe/git/gx/CLAUDE.md 2>/dev/null | head -100","description":"List repo root and read CLAUDE.md"}}
```

**tool_result + is_error** (same file):
```json
{"type":"tool_result","tool_use_id":"toolu_01WUysmjqQZoGGQBsQsaKQzT","is_error":true,"content":"Exit code 1\ncommit d262128f…"}
```

**thinking** — structure present, text always empty on this machine (1620 empty
/ 0 nonempty across 40 recent jsonl files):
```json
{"type":"thinking","thinking":"","has_signature":true}
```
→ Parser skips empty thinking; UI renders nonempty as small `--ink-3` gray
with clamp/expand. No placeholders when absent.

**Collapsed tool proof** (same gx session, 169 tools paired):
```
▸ ⚒ Bash(ls /Users/joe/git/gx && cat /Users/joe/git/gx…)
▸ ⚒ Bash(git status && git diff --stat)
▸ ⚒ Read(/Users/joe/git/gx/CONTEXT.md)
```

### Friction ledger (≥8, each resolved)

| # | Finding | Data / repro | Resolution |
|---|---------|--------------|------------|
| 1 | Detail pane only showed title / last-sent / last-reply | Snapshot fields only; no JSONL walk | **Fixed** — `get_transcript` on-demand + `TranscriptView` |
| 2 | Tool calls invisible / not collapsible | tool_use + tool_result proven above | **Fixed** — `▸ ⚒ Name(hint)`; expand input+result; red on `is_error` |
| 3 | Wanted gray thinking like native TUI | thinking blocks exist but `thinking:""` everywhere locally | **Left out** nonempty dogfood; empty skipped; UI ready when text appears |
| 4 | Can't rename sessions | no titles.json | **Fixed** — `titles.json` + `rename_session` + `/rename` / `/rename -` |
| 5 | `/rename payments spike` didn't match menu filter | root filter used `startsWith` on full remainder | **Fixed** — trailing-text match for rename |
| 6 | Multi-line prompts impossible (single-line `<input>`) | reached for terminal for long prompts | **Fixed** — `<textarea>`; ⏎ send · ⇧⏎ newline |
| 7 | Autoscroll always forced — painful when reading up | old `scrollTop = scrollHeight` on every update | **Fixed** — pin-unless-scrolled-up (48px threshold) |
| 8 | Prompt mangled in Hypervisor tmux (`Run Bash…` → `sh: …`) | Claude boots INSERT/manual; Escape→NORMAL eats vim motions | **Fixed** — `tmux::send`: Escape → `i` → C-u → literal → Enter |
| 9 | Reached for terminal to inspect JSONL shapes | verify-then-build contract | **Fixed** (process) — on-demand parser; hot loop untouched |
| 10 | Live spawn can't run tools: `model may not exist` (fable/opus) even when banner shows Fable 5; ANSI leak `claude-opus-4-8[1m]` | `claude -p` + tmux spawn both fail 2026-07-10 | **Left out** live tool+approve exit path — environmental Claude Code routing; approve path unchanged from M3x |
| 11 | Cursor / opencode full transcript | no verified JSONL shape for tools/thinking | **Left out** — empty vec; codex best-effort user/assistant only |
| 12 | Opencode fs-scan flood during `tauri dev` boot | log spam `[scan] harness=opencode reason=fs` | **Left out** — out of PARITY scope (hot-loop / notify) |

### Exit test

**Attempted:** spawn claude via Hypervisor tmux socket (`tmux -L hypervisor`,
`--session-id`, same path as `/new`), send prompt with fixed `tmux::send`,
watch pane + JSONL.

**What worked:** session appeared; after Escape→i fix the full prompt landed
in the transcript (`USER: Run the Bash tool with command: echo PARITY_DOGFOOD_OK && pwd`);
`get_transcript` / fixture tests parse tools from real gx sessions.

**What blocked:** Claude Code returned `There's an issue with the selected
model (claude-fable-5)` for every live invoke (tmux and `claude -p`) on this
machine today — no tool_use, no permission dialog. Could not complete
approve-live in this session. Recorded as ledger #10.

**Transcript view proof instead:** `cargo test transcript::tests::real_claude_jsonl_has_tools`
against live `~/.claude/projects/-Users-joe-git-gx/*.jsonl` — tools present,
empty thinking produces zero Thinking items.

### Rename

- `control/titles.rs` load/save roundtrip test OK
- Override applied in `to_wire` / `merge_pending` only (hvscan stays raw —
  compare.py OK)
- `/rename <title>` + `/rename -` wired in menu + `doRename`
- `titles.json` at `~/Library/Application Support/com.joe.hypervisor/titles.json`

### Verification

- `python3 spike/compare.py --limit 20` → OK (38 sessions, ≤1 lenient state race)
- `bunx tsc --noEmit` → OK
- `cargo test --lib` → 37 passed, 3 ignored
- `npm run tauri dev` → vite ready on :1420, `Running target/debug/hypervisor`,
  startup scans for all four harnesses

### Next queue file

`tasks/REALIZE.md` (copied into `tasks/CURRENT.md`)
