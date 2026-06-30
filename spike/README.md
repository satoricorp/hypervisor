# hvwatch — the adapter spike

Proves Hypervisor's core bet: **sessions can be observed regardless of where they
started**, by reading each harness's own on-disk state. No hooks installed, no
processes owned, read-only, zero dependencies.

```bash
python3 spike/hvwatch.py               # live board, 2s refresh
python3 spike/hvwatch.py --once        # snapshot
python3 spike/hvwatch.py --json        # JSON lines — the future history.db feed
python3 spike/hvwatch.py --max-age 168 # look back 7 days
```

## What each adapter reads

| harness | source | what we get |
|---|---|---|
| claude code | `~/.claude/projects/<proj>/<session>.jsonl` | title, model, cwd, git branch, last user msg, last assistant text, **live tool calls** (`⚒ Bash(...)`), subagent sidechain count |
| codex | `~/.codex/sessions/Y/M/D/rollout-*.jsonl` | title, model, cwd, branch, last messages, function calls, reasoning summaries |
| cursor | `globalStorage/state.vscdb` → `composerHeaders` (sqlite, `immutable=1`) | composer title, mode, workspace folder, `hasUnreadMessages`, native `isSubagent` flag |

## Status heuristic (v0)

- **working** — file written within the last 15s
- **done** — idle and the agent spoke last (waiting on you)
- **stalled** — idle >90s and *you* spoke last (agent went quiet)

Cursor bonus: `hasUnreadMessages` is literally the green-dot signal, provided by
Cursor itself.

## Known limits / next steps

- Cursor schema is undocumented and version-dependent (current parse: 2026 builds
  with `composerHeaders`). Wrapped so failures degrade to "no cursor sessions."
- Big transcripts are head+tail parsed (512KB each side); fine for status, not for
  full history ingestion — the real ingester should tail incrementally (fsevents).
- No control yet. Next tier per the plan: spawn-in-tmux (`send-keys`/`pipe-pane`),
  then adoption via `claude --resume <sid>` / `codex resume` for sessions found here.
- `--json` output is the shape history.db rows should take (sqlite, one row per
  session + transcript table).
