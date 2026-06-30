# M1 — live session adapters (Rust port of the spike)

**You are building exactly one thing this session:** Rust adapters that read
claude code, codex, and cursor session state from disk and emit the same JSON
the Python spike emits, plus a file-watcher that logs state changes live.

No UI work. No tmux. No sqlite. Those are other milestones.

## Definition of done

All three must pass on this machine (real session data exists here):

1. `cargo run --manifest-path src-tauri/Cargo.toml --bin hvscan -- --json --max-age 48`
   prints one JSON object per line, same shape as `python3 spike/hvwatch.py --json --max-age 48`.
2. `python3 spike/compare.py --max-age 48` reports `OK` (it runs both and diffs them).
3. `cargo run ... --bin hvscan -- --watch` left running while a real claude code
   session works prints a state-transition line within 2 seconds of each change
   (e.g. `claude code <sid> done -> working`).

Then follow **When done** at the bottom.

## Read first

- `spike/hvwatch.py` — the reference implementation. Port its logic faithfully;
  it is verified against real data. `spike/README.md` explains the sources.
- `PLAN.md` §Data model — the Session shape and state heuristic.

## Steps

### 1. Dependencies (only these)

In `src-tauri/Cargo.toml`: `serde` (derive), `serde_json`, `notify` (v6),
`rusqlite` (bundled feature — needed to read Cursor's db, we are NOT writing
our own db this milestone), `glob` or `walkdir`, `chrono`.

### 2. Session type — `src-tauri/src/adapters/mod.rs`

```rust
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Session {
    pub harness: String,      // "claude code" | "codex" | "cursor"
    pub sid: String,
    pub title: String,        // first real user message
    pub model: String,
    pub cwd: String,
    pub branch: String,
    pub last_user: String,
    pub last_assistant: String,
    pub activity: String,     // last tool call, "⚒ Name(arg)" or ""
    pub mtime: f64,           // unix seconds
    pub state: String,        // "working" | "done" | "stalled"
    pub age: String,          // "3s" | "2m" | "1h" | "1d"
    pub repo: String,         // basename of cwd, "-" if none
    pub src: String,          // source file path
    pub sidechains: u32,      // subagent count
}
pub trait Adapter {
    fn scan(&self, max_age_hours: f64, limit: usize) -> Vec<Session>;
}
```

Field names/values must match the spike's `--json` output exactly — the oracle
diffs them. (The spike also emits `last_role`-derived fields; check its
`finalize()` for `state`, `age`, `repo`.)

State heuristic (constants from the spike): written <15s ago → `working`;
idle >90s with the user having spoken last → `stalled`; otherwise → `done`.

### 3. claude code adapter — `adapters/claude_code.rs`

Source: `~/.claude/projects/*/*.jsonl`, one file per session, JSONL.

Parsing cheat sheet (from the working spike — do not re-derive):
- Skip lines that aren't JSON objects.
- `isSidechain: true` entries are subagent transcripts: count ones with
  `type == "user"` and no `parentUuid` as `sidechains`, then skip.
- Every entry may carry `cwd` and `gitBranch` — keep the latest.
- `type == "user"` and not `isMeta`: content is `message.content`, either a
  string or a list of `{type:"text", text}` blocks. **Noise filter** — skip
  texts that are empty, start with `<`, `Caveat:`, or `# AGENTS.md`, or contain
  `<INSTRUCTIONS>` in the first 400 chars. First surviving text = `title`,
  latest = `last_user`.
- `type == "assistant"`: `message.model` → model. Content blocks:
  `tool_use` → activity `⚒ Name(hint)` where hint = input's `file_path` |
  `path` | `command` | `pattern` (clip 46 chars); `text` → `last_assistant`.
- Big files: read first 512KB + last 512KB, drop the partial line at the tail
  chunk's start (see `read_lines` in the spike).

### 4. codex adapter — `adapters/codex.rs`

Source: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`.

- `type == "session_meta"`: payload has `cwd`, `git.branch`, session id.
- `type == "turn_context"`: payload has `model`, `cwd`.
- `type == "response_item"`, payload.type:
  - `message`: content blocks with `input_text`/`output_text`; same noise
    filter as above; role user → title/last_user, else last_assistant.
  - `function_call` | `local_shell_call` | `custom_tool_call`: activity from
    `name` + `arguments` (clip 46).
  - `reasoning`: `summary[].text` → last_assistant.
- `type == "event_msg"` with payload.type `agent_message`: `message` → last_assistant.

### 5. cursor adapter — `adapters/cursor.rs`

Source: `~/Library/Application Support/Cursor/User/globalStorage/state.vscdb`,
opened read-only immutable:
`file:...state.vscdb?mode=ro&immutable=1` (rusqlite: `OpenFlags::SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI`).

```sql
SELECT composerId, workspaceId, lastUpdatedAt, isSubagent, value
FROM composerHeaders WHERE isArchived=0
ORDER BY lastUpdatedAt DESC LIMIT ?;
```

- `value` is JSON: `name` → title, `modelName`/`unifiedMode` → model,
  `hasUnreadMessages` → treat as done with `last_assistant = "unread response"`.
- `lastUpdatedAt` is **milliseconds**.
- Rows with `isSubagent=1`: don't list; count per `workspaceId` and attach to
  that workspace's newest listed session as `sidechains`.
- Workspace folder: map `workspaceId` → `workspaceStorage/<id>/workspace.json`
  `.folder` (strip `file://`).
- The whole adapter is best-effort: any sqlite/JSON error returns an empty vec
  (log to stderr), never panics. Cursor's schema is undocumented and shifts.

### 6. CLI — `src-tauri/src/bin/hvscan.rs`

Flags: `--json` (JSONL to stdout, sorted by mtime desc), `--max-age <hours>`
(default 48), `--limit <n>` per harness (default 8), `--watch`.

`--watch`: use `notify` to watch the three source roots (recursive), debounce
500ms, rescan only the harness whose files changed, and print
`<harness> <sid> <old_state> -> <new_state>` on every transition. This becomes
the `sessions:update` tauri event in a later milestone — keep the rescan
function callable from lib code, not buried in the binary.

### 7. Verify

Run the three commands under **Definition of done**. For #3, start a real
claude code session (`claude -p 'count to 20 slowly'` works) and watch for the
transition lines.

## Scope fence

- Do not touch `src/` (React), `vite.config.ts`, or `tauri.conf.json`.
- Do not add tauri commands/events yet beyond keeping the scan function
  library-accessible.
- Do not write to any harness directory. Read-only, always.
- Do not optimize (no incremental offsets, no caching beyond mtime skip) —
  correctness against the oracle is the whole game this session.

## When done

1. Paste the output of `spike/compare.py` and a few `--watch` transition lines
   under **Evidence** below.
2. In `PLAN.md`, change `**M1 — the live board (read-only).**` list item to
   checked (`[x]`) including its AC line.
3. Copy `tasks/M2.md` to `tasks/CURRENT.md` if it exists; otherwise note
   "M2 task file needed" in Evidence.
4. Commit: `M1: rust session adapters for claude code, codex, cursor`.

## Evidence

```
$ python3 spike/compare.py --max-age 48
compared 22 sessions (22 python / 22 rust) · 0 lenient diffs
OK
```

`hvscan --watch` while `claude -p 'count to 20 slowly'` ran:

```
watching… (ctrl-c to quit)
claude code cf0a7d56-800d-4726-b9e8-e0e6304aa3eb done -> working
claude code d7dacdda-1da9-45ef-b4af-eed6caf7a08b working -> stalled
claude code af554637-5044-4ec9-80a1-f8c07ff74b46 done -> working
claude code c7bfdd18-8aae-49ae-9c7d-831999b1b669 done -> working
```

M2 task file needed (tasks/M2.md does not exist).
