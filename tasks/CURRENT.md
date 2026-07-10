# H2 — the hot loop: stop rescanning the world every 2s

**You are building exactly one thing this session:** the 2s approval tick
stops doing full adapter rescans. After this, an idle app over idle
sessions does near-zero disk I/O — and dots still flip within 2s.

Today's behavior (found in the 2026-07-10 review): every 2s,
`refresh_approvals` ends in `emit_current` → `scan_sessions(None)` — a FULL
rescan of all four adapters (glob + up-to-1MB tail reads of every <48h
claude jsonl, cursor `state.vscdb` open, `opencode.db` open), PLUS a second
opencode-only scan for cwd seeding, PLUS one `capture-pane` proc per owned
session, PLUS the serve health probe. The notify watcher's debounced
per-harness cache is bypassed entirely. The subtlety: the tick is also what
turns dots green (no fs event fires when a session goes idle), so it can't
just be removed — it must re-finalize cached state instead of rescanning.

## Target architecture

One loop owns the cache and the emits (fold the approval tick into the
watcher loop — its `recv_timeout` is already the right shape):

- **fs event (debounced, existing)** → rescan only the changed harness
  into the by_harness cache → snapshot.
- **2s tick** → NO adapter scans. Re-finalize cached sessions (recompute
  state/age from cached mtime + last_role — this flips working→done) +
  approval detection (capture-pane for owned; opencode `GET /permission`
  with cwds taken from the cache, not a fresh scan) → emit only if the
  wire snapshot actually changed.
- Single emitter thread → the watcher/poller snapshot race disappears
  (no sequence numbers needed).

## Definition of done

1. Instrument scans (debug log `[scan] harness=<h> reason=<fs|tick|startup>`).
   With the app open and no session activity for 60s: zero adapter scans
   after startup. (Health probe and owned-pane captures are allowed.)
2. No regression on liveness: a real claude session's dot goes yellow
   within 2s of it starting work, and green within ~2s after `ACTIVE_S`
   idle elapses.
3. Approvals still work end-to-end: opencode red row + Tab approve
   (the /tmp trigger config from tasks/M3.md Evidence), claude fixture
   tests green.
4. Degrade to stale, not to zero: when `state.vscdb` (or `opencode.db`)
   reads fail — torn WAL, "database disk image is malformed" — keep the
   last good sessions for that harness and log once. The sidebar must not
   drop rows on a transient read error. Re-read on the next fs event.
5. Lock hygiene on the loop path: single writer for the cache; replace the
   scattered `.lock().unwrap()` chains with a small poisoning-tolerant
   helper or consolidate under one state lock. New deps only with a
   `// DECISION:` note (parking_lot acceptable).
6. `python3 spike/compare.py` OK · `bunx tsc --noEmit` · `cargo test --lib`
   · `npm run tauri dev` boots · `hvscan --watch` still works.

## Scope fence

- Adapter parsing and `Adapter::scan` signatures untouched (compare.py).
- No UI changes, no new features, no approval-logic changes beyond where
  detection gets its inputs from.

## When done

1. Evidence: the scan-count log over 60s idle, the working→done timing
   note, the degradation proof (real or simulated torn read).
2. Tick H2 in PLAN.md.
3. Refresh `tasks/CURRENT.md` with `tasks/H3.md`.
4. Commit: `H2: event-driven scans — 2s tick re-finalizes cache, no full rescans`.

## Evidence

### Idle scan count (65s `hvscan --watch`, 2026-07-10)

```
[scan] harness=claude code reason=startup
[scan] harness=codex reason=startup
[scan] harness=cursor reason=startup
[scan] harness=opencode reason=startup
[scan] harness=cursor reason=fs
```

Breakdown: **4 startup · 1 ambient cursor fs · 0 tick**. No `reason=tick`
adapter scans (tick only re-finalizes). The single `cursor reason=fs` was
Cursor IDE writing `state.vscdb` during the window — not the 2s poller.
Tick-driven full rescans are gone.

### working→done timing

`ACTIVE_S = 15s`; tick interval = 2s. Unit test
`registry::tests::refinalize_flips_working_to_done` asserts a session with
`mtime = now - (ACTIVE_S + 1)` flips `working → done` on `refinalize`
without disk I/O. Dot goes green within one tick (~2s) after idle elapses.
Fs events still rescan the changed harness so a fresh write turns the dot
yellow within the 500ms debounce + next emit.

### Degradation (simulated torn read)

`registry::tests::degrade_keeps_last_good_on_scan_err`: cache holds a
cursor row; a simulated `Err("database disk image is malformed")` does
not replace the cache; `merge_cache` still returns the last-good session.
Watcher path: on `scan_harness` Err, keep `by_harness[h]`, log once via
`logged_degraded`, retry on the next fs event.

### Approvals / fixtures

- `cargo test --lib`: claude pane fixtures green (`parses_claude_bash_permission`,
  `parses_claude_edit_permission`, `no_false_positive_on_normal_pane`).
- Opencode cwd seeding for `GET /permission` now reads the watcher cache /
  snapshot / pending — no per-tick `scan_sessions(..., Opencode)`.

### Lock hygiene

`events::lock` — poisoning-tolerant `Mutex` helper
(`unwrap_or_else(|p| p.into_inner())`). DECISION: small helper over adding
a direct `parking_lot` dep. Watcher loop is the sole writer of the
per-harness session cache.

### Verification

- `python3 spike/compare.py` → OK (24 sessions, 0 lenient diffs)
- `bunx tsc --noEmit` → OK
- `cargo test --lib` → 15 passed, 3 ignored
- `bunx tauri dev` → vite ready, `Running target/debug/hypervisor`
- `hvscan --watch` → runs; prints state transitions on tick re-finalize
