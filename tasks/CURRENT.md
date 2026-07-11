# ARCHIVE — hide finished sessions from the board

**Prerequisite gate: H3 must be ticked in PLAN.md** (this touches the same
snapshot-assembly and UI layers H2/H3 rework — building it earlier means
merge pain). If H3 is unticked, stop and report.

**You are building exactly one thing this session:** sessions can be
archived — removed from the sidebar — and unarchived. Born from real pain:
milestone testing filled Joe's board with dead `count slowly to 15` sessions
and there was no way to clear them.

## Semantics (locked — they follow from the principles)

1. **Archive is a local tombstone, never a file operation.** Harness dirs are
   read-only (principle 1). `archived.json` in the app data dir maps
   `sid → archived_at` (same load/save pattern as owned.json).
2. **Never hide a working session.** Archiving a `working` session is
   refused with a toast (`session is working — wait for it to finish`).
   An invisible running agent is the one state this app must never create.
3. **Resurface on new activity.** If an archived session's `mtime` exceeds
   its `archived_at`, drop the tombstone and show it again. Archiving hides
   the dead past, not the living future.
4. **Archiving an owned (tmux) idle session also kills its `hv-*` tmux
   session** and removes the owned.json entry — the context lives in the
   transcript; adoption can bring it back later. The toast says so:
   `archived — tmux session closed; context stays in the transcript`.
5. **The oracle stays raw.** The filter lives in the app's snapshot path
   (events layer), NOT in `registry::scan_sessions` — `hvscan` and
   `spike/compare.py` must keep seeing every session, archived or not.

## Backend

- `archived.json` load at startup; commands:
  `archive_session(sid) -> Result<String>` (guards 2 & 4, returns toast text),
  `unarchive_session(sid)`, `list_archived() -> Vec<ArchivedWire>`
  (sid, title, harness, archived_at — title/harness from the unfiltered scan).
- Snapshot filter + resurface check run on every emit.
- Bulk: `archive_idle() -> Result<u32>` — tombstones every `done`/`stalled`
  session in one call (working and needs_you are skipped), returns the count.

## Frontend

- **⌘⌫** archives the selected session (toast with the returned text; the
  next row becomes selected so j/k flow isn't broken).
- `/archive` — selected session. `/archive idle` — the bulk cleanup (menu
  shows the count it would archive in its description if cheap to compute).
- ⌘K palette gains **archived** view: list rows (title · harness · when) with
  an `unarchive` button each; empty state "nothing archived".
- Keyboard map + PLAN.md updated (⌘⌫ row exists in the map already once this
  lands — keep them in sync).

## Definition of done

1. Archive a done test session: gone from the sidebar instantly, still gone
   after quitting and relaunching the app.
2. `/archive idle` on a board full of test sessions: every green/stalled row
   disappears in one action; working rows survive; toast reports the count.
3. Archiving a working session is refused with the explanatory toast.
4. Archive an owned idle session → `tmux -L hypervisor ls` no longer shows
   its `hv-*` session; the transcript file is untouched (`ls` the jsonl).
5. Resurface: archive a session, then `claude --resume <sid>` it in a plain
   terminal and prompt it → the row reappears on its own within ~2s.
6. Unarchive from the ⌘K archived view restores a row (observe tier if its
   tmux was killed).
7. `python3 spike/compare.py` still OK **and** `hvscan --json` still lists
   archived sessions (rule 5 proof) · `bunx tsc --noEmit` · `cargo test --lib`
   · `npm run tauri dev` boots.

## Scope fence

- No harness-file writes or deletes, ever. No M5 history/summaries (archived
  sessions get their real home when M5 lands — this is just the tombstone
  layer M5 will consume).
- Adapters and `scan_sessions` untouched.
- Don't add extra affordances (no swipe, no hover buttons) — ⌘⌫, the two
  commands, and the archived view are the whole surface.

## When done

Evidence (before/after sidebar counts, the refusal toast, resurface proof,
oracle proof), tick ARCHIVE in PLAN.md, note the next queue file, commit:
`ARCHIVE: local tombstones — hide idle sessions, never the living`.

## Evidence

(builder fills this in — an empty Evidence section means the milestone is not done)
