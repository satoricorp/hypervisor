# Hypervisor

Local macOS control surface for Claude Code, Codex, OpenCode, and Cursor
sessions. Read-only adapters watch harness transcripts; owned sessions run in
an isolated `tmux -L hypervisor` socket. Hypervisor is not an editor.

## Prerequisites

- macOS
- [tmux](https://github.com/tmux/tmux)
- Rust toolchain (`rustup`)
- Node.js + npm (scripts use npm; do not rely on bun lockfiles)
- Harness CLIs resolvable from a **zsh login shell**: `claude`, `codex`,
  `opencode` (as needed)
- `claude /login` refreshed if you need live Claude Code control

## Run

```bash
npm install
npm run tauri dev
```

The Vite/Tauri webview expects port **1420** (strict — do not remount elsewhere).

## Headless checks

```bash
cargo run --bin hvscan          # one-shot session scan
cargo run --bin hvscan -- --watch
```

## Tests

Safe by default (no adopt / live prompt / harness spawn):

```bash
cargo test --lib
```

Live-fire tests (adopt real sessions, prompt opencode, spawn harness CLIs):

```bash
HV_LIVE=1 cargo test --lib -- --ignored
```

Adapter parity tripwire:

```bash
python3 spike/compare.py
```

Typecheck the frontend:

```bash
bunx tsc --noEmit
# or: npx tsc --noEmit
```
