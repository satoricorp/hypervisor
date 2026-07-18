# SEPARATE — split paid features into their own crate (pre-repo)

**Prerequisite gate:** none hard. Independent structural refactor on the current
layout (M8a/M8b remote, M4 worktrees, REALIZE command set). Does **not** depend on
Apple enrollment. **Read this whole file before touching code — this is a
dependency-inversion refactor, not a feature. Behaviour must not change.**

## What this milestone is — and what it is NOT

Monetization has three independent axes. This task is **only the first**:

1. **Separation (this task).** Paid features become a separate Cargo crate
   `hypervisor-pro` that depends on `hypervisor-core`; core never imports pro
   (cargo-enforced). This is the unit later moved to a private repo.
2. **Gating (later — `tasks/MONETIZE.md`).** The runtime license check
   (`entitlements.rs`, offline Ed25519 → tier Free/Pro/Remote). **Do not build
   licensing here.** In this task pro features are selected purely by the `pro`
   Cargo feature (compile-time); the dev/ship build has them on.
3. **Publishing (later — `tasks/PUBLISH.md`).** Creating the public repo and having
   the private pro repo pull core as a dependency. **Do not create a second repo or
   push anything here.**

End state of THIS task: one git repo, a Cargo **workspace** of three crates
(`hypervisor-core` lib, `hypervisor-pro` lib, `hypervisor-app` bin), one tauri
binary, everything still working. A `--no-default-features` build produces a
complete free app.

## The free / paid line (decided 2026-07-13 — do not re-adjudicate)

**Free → `hypervisor-core`:** cross-harness session aggregation + everything to
interact with a session (watch, prompt, approve/deny, spawn, adopt); **worktrees**
(stay free — entangled with spawn, part of parallel interaction); the **TV**; the
whole React frontend; adapters; `hvscan`; usage ledger; access; telemetry; grammar;
the control ladder; local-state stores.

**Paid → `hypervisor-pro` (behind `feature = "pro"`):**
- `remote/*` — the whole tailnet + iMessage surface (`mod.rs`, `imessage.rs`,
  `tailscale.rs`, `keepawake.rs`) — the future **Remote ($40)** tier.
- `/broadcast` — `broadcast_prompt` (events.rs:1032) + its registration.
- Not yet built; land in pro when they are: `/loop`, history-memory (M5).

The **frontend is not split** — it stays in core, open. Pro menu items being inert
in a pro-less build is expected; hiding/gating them by license is MONETIZE's job,
not this task's.

## The one seam to invert (core → pro); the rest is file moves

Remote is already a *consumer* of core — it calls `approve_sid`/`deny_sid`/
`prompt_sid` and reads `SessionsUpdate`. There is exactly **one backwards edge**:
core pushes session snapshots into remote's `SseBus`.

Verified coupling points:
- `AppState.remote_bus: Arc<SseBus>` — field at **events.rs:132**, constructed at
  **lib.rs:95**, test double at events.rs:1683.
- push site — **events.rs:431** `crate::remote::broadcast_sessions(&state.remote_bus, &update)`.
- core `use crate::remote::…` — **events.rs:11**; **lib.rs:30-31, 99, 147, 165-166**.

**Invert it.** In core:

```rust
// A sink core pushes snapshots to without knowing who consumes them.
pub trait SessionSink: Send + Sync {
    fn broadcast(&self, update: &SessionsUpdate);
}
```

- `AppState` holds `sinks: Vec<Arc<dyn SessionSink>>` in place of `remote_bus`.
  The emit path (events.rs:431) becomes `for s in &state.sinks { s.broadcast(&update); }`.
- Pro's `SseBus` gets `impl SessionSink`. Pro startup (`remote::start`) builds the
  bus, registers it into `state.sinks`, and spawns the server — called from the app
  binary's setup **only under `feature = "pro"`**.
- Seam test: `grep -rn "remote" src-tauri` in the core path returns nothing outside
  `#[cfg(feature = "pro")]`.

Pro→core imports are already the right direction; make these **`pub`** in core so
they stay legal from the pro crate: `AppState`, `SessionsUpdate`, `ToastEvent`,
`grammar::{Action, BoardRow}`, the telemetry event enum + `ApprovalVia`/`PromptVia`,
and `approve_sid`/`deny_sid`/`prompt_sid`. (`board_from_wire`/`execute_action` are
remote-internal — they move to pro.) Leave a `// DECISION:` note wherever the public
boundary is non-obvious.

## Steps

### Phase 0 — invert + feature-gate, in place (no new crates yet)
1. Add `SessionSink` to core; replace `AppState.remote_bus` with `sinks`; rewrite
   the events.rs:431 push. Behaviour identical.
2. Put every pro module + its wiring behind `#[cfg(feature = "pro")]`: `mod remote;`,
   the `remote::start` call (lib.rs:99), the `remote_status`/`imessage_status`/
   `broadcast_prompt` entries in `generate_handler!` and their `use` lines
   (lib.rs:30-31, 147, 165-166), and `broadcast_prompt` in events.rs. Declare feature
   `pro` in Cargo.toml, **default-on**.
3. Make the consumed core types `pub` (list above).

**Phase 0 acceptance:**
- `cargo build` (default) — app behaves identically to today; `cargo test --lib`
  still ~59 passed.
- `cargo build --no-default-features` — **compiles and launches a complete free
  app** (aggregation, prompt/approve/spawn/adopt, worktrees, TV). The core-path
  `remote` grep above is empty.

### Phase 1 — Cargo workspace (still one repo)
4. Convert to a workspace under `crates/`:
   - **`hypervisor-core`** (lib) — all free modules + `bin/hvscan.rs` + `SessionSink`
     + the public API. **Zero dependency on pro.**
   - **`hypervisor-pro`** (lib) — `remote/*`, `broadcast`, the `SseBus`/`SessionSink`
     impl + `start`. Depends on `hypervisor-core`.
   - **`hypervisor-app`** (bin) — the tauri binary: `Builder` + `generate_handler!` +
     `generate_context!` + setup. Depends on `hypervisor-core` always, `hypervisor-pro`
     under `feature = "pro"` (default-on). One tauri binary; free build is
     `--no-default-features`. Point its `tauri.conf.json` at the existing `dist/`.
5. Move files; fix paths (`crate::` → `hypervisor_core::` inside pro).

**Phase 1 acceptance:**
- `cargo build -p hypervisor-core` — core lib compiles alone. This is the
  structural proof that core carries no pro code (cargo won't allow the edge).
- `cargo build -p hypervisor-app` (default) — full app; `--no-default-features` —
  free app. Both launch.
- `cargo test --workspace` green; `python3 spike/compare.py --limit 20` OK
  (adapters unchanged); `tsc --noEmit` clean.

## Scope fence

- **No licensing.** No `entitlements.rs`, no Ed25519, no tiers — that's MONETIZE.
  Pro is selected by the Cargo feature only.
- **No second repo / no publishing / no git-remote work** — that's PUBLISH.
- **No behaviour change.** Move + invert only. Do not "improve" remote/broadcast
  while relocating them.
- Adapters stay read-only; no writes to harness dirs; app state stays local
  (AGENTS.md hard rules unchanged).
- Worktrees stay in core/free. Frontend stays in core, unsplit.
- No new dependencies beyond what the workspace split itself needs.

## When done

Record evidence below: the `cargo build -p hypervisor-core` and
`--no-default-features` output proving core builds free, the empty core-path
`remote` grep, and test counts. Add a ticked **SEPARATE** line to PLAN.md's
milestone list and note the two follow-ons it unblocks (MONETIZE = license gate,
PUBLISH = repo split / go public). Commit:
`SEPARATE: hypervisor-core / hypervisor-pro workspace — pro behind a feature, core builds free`.

## Evidence

(builder fills this in)
