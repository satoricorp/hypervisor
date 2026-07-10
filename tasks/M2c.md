# M2c — opencode: adapter + the api control tier

**You are building exactly one thing this session:** opencode support — an
adapter so opencode sessions appear in the sidebar, `/new` spawn under our
tmux, and the **api tier**: prompting any opencode session over a
Hypervisor-managed `opencode serve` HTTP instance, no adoption dance needed.

Not in this session: approvals (M3 — even though the API has permission
endpoints; record them in Evidence, build nothing), worktrees (M4), sqlite
history (M5).

All storage/API facts below were verified on this machine against opencode
**1.17.8** (homebrew) on 2026-07-10. If reality disagrees with this file,
leave a `// DECISION:` and follow reality.

## Storage (adapter reads this, strictly read-only)

`~/.local/share/opencode/opencode.db` — sqlite, WAL mode (`-wal`/`-shm`
alongside), drizzle-migrated. 364 real sessions on this machine.
**Ignore `~/.local/share/opencode/storage/`** — legacy JSON layout, stale
since Feb 2026; the db is the live store.

Tables you need (`sqlite3 -readonly … .schema` for the rest):

- `session`: `id` (`ses_*` — this is the sid), `parent_id` (NULL = top-level;
  non-NULL = subagent → count children as `sidechains`), `directory` (= cwd),
  `title`, `model` (JSON text: `{"id":"…","providerID":"…","variant":"…"}`,
  may be NULL/empty on old rows), `agent`, `time_created`/`time_updated`
  (**epoch milliseconds** — divide by 1000.0; `Session.mtime` is seconds and
  the idle math depends on it), `time_archived` (non-NULL → skip the row).
- `message`: `session_id`, `time_created` (ms), `data` JSON:
  `{"role":"user"|"assistant","time":{"created":…,"completed":…},…}` —
  last message's role drives `last_role` for the state heuristic.
- `part`: `message_id`, `session_id`, `data` JSON, `type` ∈ text, tool,
  reasoning, step-start, step-finish, patch. Text:
  `{"type":"text","text":"…"}` → last_user/last_assistant. Tool:
  `{"type":"tool","tool":"todowrite","state":{"status":"completed","input":{…}}}`
  → `activity` as `tool(primary-input, truncated)`, matching the other
  adapters' `Edit(src/…)` style.

Open exactly like `adapters/cursor.rs` does:
`Connection::open_with_flags(…, SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI)`;
any failure degrades to zero sessions, never a crash. `branch` stays `""`
(no branch column; match cursor). Title: use the `title` column as-is.

Heads-up: the newest session in the db is from Jun 30 — outside the 48h
window, so the sidebar will show **nothing** until you run `opencode` fresh.
That is the adapter working, not broken.

## Backend steps

### 1. `src-tauri/src/adapters/opencode.rs`

`OpencodeAdapter` implementing `Adapter::scan` per the schema above, reusing
`ACTIVE_S`/`STALL_S` and the `RawSession` → finalize flow. Registry wiring in
`registry.rs`: `Harness::Opencode` (`as_str` → `"opencode"`), add to the
default scan vec, watch root `~/.local/share/opencode` (recursive, like the
others; the path→harness match at registry.rs:73 gets a
`/.local/share/opencode` arm). If `log/` churn makes snapshots noisy, narrow
the watch to the db files and note a DECISION.

### 2. `src-tauri/src/control/opencode.rs` — the serve child + HTTP client

Manage one `opencode serve --port 14096 --hostname 127.0.0.1` child process
(spawn lazily on first use; best-effort kill on app exit). Readiness: the
stdout line `opencode server listening on http://127.0.0.1:14096`, or GET
`/session` returning 200. It prints "OPENCODE_SERVER_PASSWORD is not set;
server is unsecured" — that is why it must **never** bind anything but
127.0.0.1. `// DECISION:` 14096 not 4096 so a user-started serve can't
collide.

Endpoints (confirmed via `/doc`, OpenAPI 3.1.0):

- `GET /session?directory=<cwd>` — list
- `POST /session/{sid}/prompt_async?directory=<cwd>`, body
  `{"parts":[{"type":"text","text":"…"}]}` (`parts` is the only required
  field) — fire-and-forget; the db watcher picks up the reply.
- `POST /session/{sid}/abort` — note for `/kill` later; not required now.
- `GET /permission`, `POST /permission/{requestID}/reply`, `GET /event`
  (SSE) — **M3 material, record in Evidence, do not build.**

HTTP client: add `ureq = "2"` (blocking, small — calls happen in tauri
command handlers). No other new dependencies.

### 3. Routing + tiers

- `control_for` (events.rs): owned → `tmux` (unchanged); cursor → `watch`;
  **opencode non-owned → `api`** (chip copy `api · background`, already in
  the mockup ladder); else `observe`.
- `send_prompt`: owned → send-keys (unchanged). Non-owned opencode → HTTP
  `prompt_async`, guarded: if `now - mtime < 60s`, refuse with
  `"active {idle}s ago — it may still be open in another terminal. close it
  there, or let it go idle, then prompt."` (same shape as the M2b fork
  guard; the user's TUI may hold the session). Owned sessions skip the guard.

### 4. `/new` spawn via tmux

- `tmux::spawn`: give opencode its own arm — TUI via
  `/bin/zsh -lic 'opencode --model <provider/model>'` (confirm the exact
  model flag with `opencode --help`; DECISION if it differs). While there,
  the `"cursor" | "opencode" => "not wired until M2b"` arm is stale — split
  it: cursor says `"cursor is watch-only"`, and update the test assertion in
  tmux.rs that greps for "M2b".
- Correlation: extend `owned::find_new_sid` with an opencode arm — poll
  `scan_sessions(…, Some(Harness::Opencode))` for the newest session with
  `directory == cwd` and `time_created ≥ spawn_time`; sid is the `ses_*` id.
- `src/menuData.ts`: replace `MODELS.opencode` with real `provider/model`
  ids taken from `opencode models` on this machine.

## Definition of done

1. Run `opencode` in a terminal (any repo), exchange one message, quit. The
   row appears in the sidebar: harness `opencode`, title/model from the db,
   chip `api · background`.
2. After >60s idle, prompt it from the bar → lands via HTTP (verify with
   `opencode export <sid>` or by reopening the TUI); dot goes yellow within
   2s of the db changing.
3. Prompt while <60s idle → refused with the idle-time toast; nothing sent.
4. `/new` → opencode → model: an `hv-*` tmux session running the opencode
   TUI appears; control flips to `⏻ runs in background` within 15s of the
   first prompt; a prompt from the bar lands via send-keys.
5. `python3 spike/compare.py` still prints OK; `cargo test --lib` passes
   including new opencode unit tests (at minimum: ms→s + archived/parent
   filtering against a fixture db built in the test, model-JSON → display
   string, tool-part → activity string); `npm run tauri dev` still boots
   (`default-run = "hypervisor"` stays).

## Scope fence

- `adapters/claude_code.rs`, `codex.rs`, `cursor.rs` untouched — compare.py
  is the tripwire.
- Never write under `~/.local/share/opencode/` — the adapter opens the db
  READ_ONLY; only the serve child (opencode itself) writes there.
- serve binds 127.0.0.1 only; the child is killed on app exit; no password
  handling, no remote exposure.
- tmux only via `control::tmux` (`-L hypervisor`). Only file write:
  owned.json.
- No approvals, no SSE consumer, no sqlite history. One new crate: `ureq`.

## When done

1. Under **Evidence**: paste the sidebar row for a real opencode session,
   the HTTP prompt confirmation, the idle-guard refusal toast, the `/new`
   spawn + correlation result, compare.py and test output. Note
   `// DECISION:` comments and record the M3-relevant endpoints
   (`/permission`, `/event`).
2. Tick M2c in `PLAN.md`.
3. Note "M3 task file needed" in Evidence.
4. Commit: `M2c: opencode adapter + api control tier`.

## Evidence

```
(builder fills this in)
```
