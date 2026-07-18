# PUBLISH — two repos: private pro pulls (public-ready) core

**Prerequisite gate:** SEPARATE **and** MONETIZE ticked in PLAN.md. The workspace
must be `hypervisor-core` / `hypervisor-pro` / `hypervisor-app` with the license
gate live. If either milestone is missing, stop and report.

## Goal

Physically separate the crates into two git repos so proprietary code is never in
the open-source repo:
- **`hypervisor-core`** — the existing repo (`github.com/satoricorp/hypervisor`),
  destined to go public. Free app + frontend + adapters + hvscan + entitlements
  *verify*.
- **`hypervisor-pro`** — a **new private** repo. `remote/*`, `/broadcast`, future
  `/loop` + M5, the gate checks, the `hypervisor-app` shipping binary + tauri
  bundle config. Depends on `hypervisor-core`.

**Two-step visibility (deliberate).** Create `hypervisor-pro` **private now**; keep
`hypervisor-core` **private too** until the go-public moment (§Going public). This
gives the repo separation without open-sourcing before you're ready — open-sourcing
is the "second act after notarize + launch" (see the hypervisor-monetization memory
/ DEPLOY). Do not flip anything public in the normal course of this task.

## Steps

1. **New private repo** `satoricorp/hypervisor-pro` (`gh repo create … --private`).
   Move `crates/hypervisor-pro` + `crates/hypervisor-app` + `tauri.conf.json` + the
   bundle/icon assets into it. Its `Cargo.toml` depends on `hypervisor-core` via a
   **pinned git dependency** (`rev = <core sha>`) or a submodule — choose one, record
   why in a `// DECISION:`.
2. **Core repo** keeps `crates/hypervisor-core`, the frontend, `hvscan`, `spike/`,
   `site/`, the free-app assembly. It must build + run the free app on its own.
3. **CI** — core: `cargo build -p hypervisor-core` + tests + `spike/compare.py`.
   Pro: check out core at the pinned rev, build the app, run the release pipeline
   (reuse DEPLOY's `scripts/release.sh` + `.github/workflows/macos-release.yaml` —
   the notarized DMG is built from the **pro** repo now).
4. **Secrets** live in the pro repo (PostHog prod key, updater private key, license
   prod pubkey, Apple signing) — the DMG builds there.

## Going public (gated — do ONLY at launch, never automatically)

Flipping `hypervisor-core` public exposes its **entire git history**, which today
contains pro code (remote/imessage) from before the split. `git mv` does **not**
erase history. So going public requires one of:
- **(recommended)** publish core as a **fresh-history** repo (new repo, or a squashed
  root commit) that never contained pro; or
- a `git filter-repo` scrub of pro paths from core's history (fragile — verify the
  result contains no pro blobs).

**Never** `gh repo edit --visibility public` on a repo whose history still carries
pro code. This step is gated on Joe's explicit go.

## Definition of done

1. `hypervisor-pro` (private) builds the full, notarize-capable app pulling
   `hypervisor-core` at a pinned rev.
2. Clone `hypervisor-core` **alone** (no pro access) → `cargo build -p
   hypervisor-core` → the free app builds and launches.
3. `grep -rn "remote\|broadcast" <core-repo>/` finds only the `SessionSink` trait —
   no pro implementation.
4. The release pipeline produces the same DMG as before, now from the pro repo.
5. Going-public prerequisites (fresh-history plan) are documented but **not
   executed** unless Joe says so.

## Scope fence

- `hypervisor-core` is **not made public in this task** — only prepared. The
  visibility flip is a separate, explicit, Joe-gated action.
- Remote stays tailnet-only; no infra/exposure change (DEPLOY's iron rule holds).
- No secrets committed to either repo tree.

## When done

Evidence (both clone-and-build proofs, the core-repo grep, a release-from-pro run),
the pinned core rev the pro repo uses, and the dep mechanism chosen (git dep vs
submodule). Tick PUBLISH; note the go-public step remains pending Joe. Commit in
each repo.

## Evidence

(builder fills this in)
