# Hypervisor — build plan

This document is the handoff spec. It is self-contained: an implementing agent
should be able to build the app from this file, the spike, and the mockups,
without access to the design conversation.

**One-liner:** Conductor runs your agents; Hypervisor watches all of them,
wherever they run, and lets you conduct from one bar.

## How this plan is executed

This file is the **spec**, not the assignment. Builders never work from this
file directly:

- The current assignment is always `tasks/CURRENT.md` — one milestone, with
  steps, a scope fence, verification commands, and a "When done" ritual.
- `AGENTS.md` at the repo root tells any agent to start there.
- A milestone is complete when its checkbox below is ticked and its evidence
  is recorded in the task file. One milestone per agent session.

## References (read in this order)

1. `spike/hvwatch.py` + `spike/README.md` — working adapter logic for all three
   harnesses, verified against real data. The Rust adapters port this logic.
2. Mockup, variant B (the chosen layout — sidebar + transcript pane):
   https://claude.ai/code/artifact/77f575ab-1e0a-4b34-869e-b24e5c8b8546
   Interactive; drive it with the keyboard before writing any UI code.
3. Mockup, variant A (stacked list — reference only for the card anatomy):
   https://claude.ai/code/artifact/00220b6e-3958-4eb7-b074-52812e64202b

## Principles (non-negotiable)

1. **Observer first.** Hypervisor monitors sessions regardless of where they
   started. It never requires the user to move their workflow into it.
2. **BYO tokens.** Never sell, proxy, or mark up inference. Keys stay in the
   user's env/keychain; requests go direct from each harness to its vendor.
   Hypervisor reads key *presence*, never stores key material.
3. **Local-first.** All state (history, settings, summaries) lives on disk in
   the user's Library folder. No telemetry, no cloud dependency.
4. **Not an editor.** No diff-merge UI, no code editing, no chat client. Done
   sessions link out (open PR, open in editor); they are not edited here.

## Stack (already scaffolded in this repo)

- Tauri 2 + Rust backend — adapters, tmux control, sqlite, notifications.
- React 19 + Vite + TypeScript frontend.
- No CSS framework: hand-written CSS with the token set below.
- sqlite (rusqlite) for history. Embedding/retrieval layer later (HelixDB
  embedded preferred; turbopuffer optional opt-in backend — see M5).

## Architecture

```
src-tauri/src/
  adapters/            one module per harness, all read-only
    claude_code.rs     ~/.claude/projects/**/*.jsonl
    codex.rs           ~/.codex/sessions/**/rollout-*.jsonl
    cursor.rs          Cursor globalStorage state.vscdb (composerHeaders)
    mod.rs             Adapter trait + registry
  registry.rs          merged session list, state machine, fs-watch (notify crate)
  control/
    tmux.rs            control-mode client (`tmux -C`), spawn/send-keys/pipe-pane
    adopt.rs           respawn observe-only sessions via `claude --resume` etc.
    opencode.rs        HTTP client for `opencode serve`
  history/
    store.rs           sqlite: sessions, transcripts, costs, summaries
    summarize.rs       archive-time summary generation (see M5)
  approvals.rs         pending-permission detection + approve/deny actions
  usage.rs             token/cost ledger, pricing tables, subscription detection
  events.rs            tauri event emitters (sessions:update, toast, etc.)
src/                   React: components mirror the mockup DOM 1:1
```

Data flow: adapters watch files → registry diffs state → emits `sessions:update`
(full snapshot; diffing client-side is premature) → React renders. Frontend
actions (`send_prompt`, `approve`, `spawn`, `adopt`) are tauri commands.

## Data model (from `spike/hvwatch.py --json`, extended)

```ts
type SessionState = 'working' | 'done' | 'needs_you' | 'stalled';
// dot colors: working=yellow, done=green, needs_you+stalled=red. THREE colors only.

interface Session {
  id: string; harness: 'claude code'|'codex'|'cursor'|'opencode'|'claude.ai';
  title: string;            // first real user message, truncated 64
  model: string; cwd: string; repo: string; branch: string; worktree?: string;
  lastUser: string; lastAssistant: string;
  activity?: string;        // last tool call, e.g. `Edit(src/auth/middleware.ts)`
  state: SessionState; mtimeMs: number;
  control: 'tmux'|'api'|'watch'|'observe';   // the control ladder tier
  approval?: string;        // pending permission request, e.g. `Bash(rm -rf dist)`
  subagents: Subagent[];    // sidechains (claude), isSubagent rows (cursor)
  loop?: boolean;
}
interface Subagent { target: string; model: string; state: SessionState; task: string; }
```

State heuristic (validated in spike): working = source written <15s ago;
done = idle, agent spoke last; stalled = idle >90s and user spoke last.
Cursor: `hasUnreadMessages` from composerHeaders maps to done.

## The control ladder

| tier | who | how | chip copy |
|---|---|---|---|
| api | opencode | `opencode serve` HTTP | `api · background` |
| tmux | anything Hypervisor spawns | own tmux server, `send-keys`/`pipe-pane` | `⏻ runs in background` |
| adopt | user's bare-terminal sessions | one keystroke: respawn under our tmux via `claude --resume <id>` / codex resume; context preserved | (becomes tmux) |
| watch | Cursor IDE | state.vscdb read-only | `watch` |
| observe | claude.ai | none in v1 (extension later) | `observe-only` |

Selecting an observe-only session changes the prompt placeholder to
"observe-only — ⏎ adopts into hypervisor tmux (claude --resume <sid>)"; return
performs the adoption. The lid-closed promise requires a power assertion
(IOKit `IOPMAssertionCreateWithName` / `caffeinate`-equivalent) held while any
tmux session is working — without it, soften the chip copy.

## Keyboard map (global, when prompt not focused; letters focus the prompt)

| key | action |
|---|---|
| `1–9` | select session n |
| `j/k` `↑/↓` | prev/next session |
| `h/l` `←/→` | out of / into the selected session's subagents |
| `⇥ Tab` | approve the selected session's pending permission |
| `⏎` | focus prompt (or adopt, if selection is observe-only) |
| `/` | command menu in prompt (filter as you type) |
| `⌘K` | palette: session/history/usage/access/settings + commands |
| `⌘N` | New Agent (harness → model picker) |
| `esc` | back out: menu step → menu → blur → session view |

Prompt focused: `⏎` sends to the target chip (`● 3` / `● 1·2`). Typing at a
pending approval = deny with guidance (message goes to session, request cleared).

## Commands (`/` menu)

Wired in v1: `/new` (harness→model, auto-worktree if repo busy, history context
attach), `/subagents` (target→model, spawns handoff), `/plan` (run, then park in
needs_you with "approve to execute?"), `/review` (spawn reviewer subagent on the
diff), `/loop` (re-run interval, `↻` chip), `/worktree`, `/broadcast`.
Stubs OK in v1: `/handoff`, `/compact`, `/kill` (kill = tmux kill-session where owned).

Yolo mode: statusbar toggle, auto-approves every permission request, amber when on.

## Design system (extract from mockup CSS `:root`)

- bg `#0a0c0e` · surface `#10141a` · surface-2 `#151a21` · inset `#0c0f13`
- border `#1f242b` / `#2e3540` · ink `#dee3e8` / `#8a93a0` / `#545e6b`
- selection white `#e9edf2` (rings + the solid left bar, never a color fill)
- status: ok `#46d68c` · busy `#e2a33e` · err `#e5544b` · mint `#57e0c9` (money only)
- radii: window 10 (macOS), cards 3, chips 2 — lean square
- type: Berkeley Mono everywhere (local font, fallback SF Mono); "Xer0" for the
  HYPERVISOR wordmark only; 12px UI, 9.5px caps labels, tabular-nums for digits
- copy: lowercase except the `+ New Agent` button; controls say what they do
- dark only in v1

## Milestones — each independently shippable, verify before moving on

Build order: M1 → M0 → M2 → … (adapters first — they're the moat and the
spike is their test oracle; UI second, binding them together in M2).

- [x] **M1 — live session adapters.** Port `spike/hvwatch.py` to Rust
  (`adapters/` + `hvscan` CLI + `notify` fs-watch). Task file: `tasks/M1.md`.
  AC: `python3 spike/compare.py` prints OK against real data on this machine;
  `hvscan --watch` logs a state transition within 2s of a real claude code
  session starting/finishing work.

- [x] **M0 — UI skeleton.** Tauri window renders variant B layout with the
  mockup's mock-session array. `npm run tauri dev` works.
  AC: the keyboard map above fully drives the mock UI.

- [x] **M2 — live binding + owned-tmux control.** Registry emits
  `sessions:update` tauri events from the M1 watcher; sidebar goes live.
  Dedicated tmux socket (`tmux -L hypervisor`); `/new` spawns claude/codex
  sessions; prompt bar sends via send-keys; spawned-session ↔ transcript
  correlation persisted in owned.json. Task file: `tasks/M2.md`.
  AC: real sessions render with live dots (yellow while a real claude session
  works, green within 2s of done); + New Agent creates an `hv-*` tmux session
  that appears in the sidebar; a prompt sent from the bar lands in it.

- [x] **M2b — adoption.** Adopt observe-only claude/codex sessions: respawn
  under our tmux via `claude --resume <sid>` / `codex resume <uuid>`, with the
  live-writer fork guard (refuse if idle <60s). Task file: `tasks/M2b.md`.
  AC: adopt a bare terminal session and then successfully prompt it; adopting
  an active session is refused with an explanatory toast.

- [x] **M2c — opencode tier.** opencode adapter (storage at
  `~/.local/share/opencode/` — sqlite `opencode.db` + session/message dirs;
  schema needs exploration before the task file is written), `/new` spawn via
  tmux, and the `api` control tier over `opencode serve` HTTP (confirm
  endpoints via its `/doc` OpenAPI). AC: opencode sessions appear in the
  sidebar and accept prompts.

- [ ] **M3 — approvals.** Detect pending permission requests (claude code: hook
`PreToolUse`/permission events or transcript markers; codex: approval prompts in
rollout). Tab approves, typing denies-with-guidance, yolo toggle.
AC: a real `claude` session asking to run a command is approved from Hypervisor
with Tab and proceeds.

- [ ] **M4 — worktrees.** `/worktree`, auto-worktree default when `/new` targets a
repo with an active session, repo·branch·worktree line in the header, settings
toggle. AC: two sessions in one repo never share a working tree unless the user
opts out.

- [ ] **M5 — history + memory.** sqlite store; sessions archive on end (or manual);
archive-time **summary with simple semantic meaning** — one sentence of outcome
+ key decisions + files touched, stored per session (generate with a cheap
model via the user's own key, or extractively if none). History view =
search over summaries. `/new` attaches the top related summaries (same repo
first, then embedding similarity — HelixDB embedded; turbopuffer as opt-in
remote backend, off by default per principle 3).
AC: create a new agent in a repo with prior sessions; its first context
message contains those summaries, and the history view finds them by keyword.

- [ ] **M6 — usage + access.** Token/cost ledger from transcripts (claude code and
codex record usage; cursor best-effort), pricing table shipped + updatable,
subscription-vs-API split, Access view (env/keychain detection, read-only).
AC: ticker `$x.xx · x.xM TOK` bottom-left matches a hand-check of one day.

- [ ] **TV (side-quest, any time after M3).** YouTube in a separate
  `WebviewWindow` satellite that auto-pauses when a session needs you.
  Spec: `design/tv.md`. Main-window CSP must not change.

- [ ] **M7 — the macOS surface.** Menu bar item with aggregate dot, notification on
done/needs_you with inline reply, dock badge = red count, global hotkey,
launch-at-login, power assertion while sessions work.
AC: with the window closed, a finishing session notifies; replying from the
notification reaches the session.

- [ ] **M8a — remote: tailnet mobile slice.** Triage page (needs-you stack +
  approve/deny + prompt) served from the backend on 127.0.0.1, exposed only
  via `tailscale serve`, authenticated by `Tailscale-User-Login`. Full spec:
  `design/remote.md`. No remote yolo, ever.

- [ ] **M8b — remote: iMessage bridge.** Text your Mac: `status`,
  `3: <prompt>`; approvals over iMessage OFF by default. chat.db read-only
  polling + AppleScript replies, self-chat only. Spec: `design/remote.md`.

## Risks

- **Cursor schema drift** — `composerHeaders` is undocumented; wrap the adapter
  so failure degrades to "no cursor sessions," never a crash. Pin known-good
  schema versions and log mismatches.
- **Resume-fork caveat** — `claude --resume` on a session that's still open
  elsewhere forks it. Adoption must detect a live writer (recent mtime + pty
  check) and warn instead of silently forking.
- **Transcript secrets** — history.db may contain keys/tokens echoed in tool
  output. Exclude from Time Machine by default; redact obvious patterns at
  ingest; consider SQLCipher later.
- **Big transcripts** — tail incrementally (store per-file offsets); never
  re-read whole files on each tick (the spike's head+tail trick is v0 only).

## Instructions for the implementing agent

- Work milestone by milestone; do not start M(n+1) until M(n)'s AC passes
  against real harness data on this machine.
- The mockup is the UI contract — match its DOM structure, tokens, and keyboard
  behavior before improvising. When the mockup and this doc conflict, this doc wins.
- Don't add dependencies without need; no UI kit, no state library until pain.
- Never write to any harness's files or directories. Adapters are read-only.
- Anything ambiguous: leave a `// DECISION:` comment and pick the option that
  preserves the four principles.
