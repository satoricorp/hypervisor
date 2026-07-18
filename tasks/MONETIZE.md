# MONETIZE — the license gate (entitlements)

**Prerequisite gate:** SEPARATE ticked in PLAN.md — `hypervisor-pro` is its own
crate behind `feature = "pro"`. If that crate doesn't exist, stop and report.

## What a "license gate" is

The runtime check that decides whether *this* user may use a paid feature. Same
binary for everyone; the gate separates free from paying:

1. On purchase the merchant-of-record (Lemon Squeezy / Paddle / Polar) issues a
   **license key** — a signed token encoding tier + expiry.
2. The app **verifies it offline** with an Ed25519 **public** key baked in at build
   (no server call — local-first) and caches the plan.
3. Before any paid feature runs, code asks `entitlements::plan()`; below the
   feature's tier → refuse with a structured `needs_upgrade` the UI turns into an
   upgrade prompt.

The private signing key lives only at the issuer — never in either repo, never on
the build machine.

## Goal

Turn the compile-time `pro` feature into a **runtime, per-tier** entitlement so one
shipped binary serves Free / Pro ($15) / Remote ($40), gating by the held license.

## Tiers

`Free < Pro < Remote` (Remote implies Pro). Feature → minimum tier:
- **Pro:** `/broadcast`, `/loop`, history-memory (M5).
- **Remote:** the `remote/*` server (tailnet page + iMessage).
- **Free:** everything else (aggregation, session interaction, worktrees, TV).

## Steps

1. **`hypervisor-core::entitlements`** — verify lives in core (a public-key check is
   harmless to open-source and lets the free app read license state):
   - `enum Plan { Free, Pro, Remote }`, totally ordered.
   - `verify(token, pubkey) -> Option<License { plan, expires }>` — Ed25519 over a
     compact signed payload. Public key via `option_env!("HV_LICENSE_PUBKEY")` at
     build (same pattern as the PostHog keys); absent → Free only.
   - Source: `license.key` in the app data dir, pasted via Settings; verified on
     load + change; cached in `AppState`. `plan()` accessor on `AppState`.
2. **Gate the pro features** — checks in the pro crate at each entry point, one
   helper `require(plan, Tier) -> Result<(), Upgrade>`:
   - `remote::start` — no bind unless `plan() >= Remote`.
   - `broadcast_prompt` (+ future `/loop`) — `Err(needs_upgrade("Pro"))` unless
     `plan() >= Pro`.
3. **Frontend bridge** — `get_entitlements` command → `{ plan, features:[..] }`. The
   UI (a) hides/badges pro menu items when plan is too low or the feature isn't
   compiled in (empty features list in a `--no-default-features` build — this is how
   pro items stay hidden in the free app), (b) renders the upgrade prompt on a
   `needs_upgrade` error.
4. **Settings** — a license row: paste key, show current plan + expiry, clear.
   Content-free telemetry `license_tier` (names/counts only, existing gate).
5. **Dev keypair now, real MoR later** — generate a **test** Ed25519 keypair; ship
   the public key via env for local builds (never the private key). Wiring the real
   MoR product + production public key is a **launch step**, documented here, not
   blocking this milestone.

## Definition of done

1. No license: `remote::start` doesn't bind; `/broadcast` returns `needs_upgrade`;
   UI shows the upgrade prompt; all free features work.
2. Test **Pro** license: `/broadcast` works; remote still gated.
3. Test **Remote** license: remote server binds; phone page loads.
4. Tampered / expired token → treated as Free; no panic.
5. `cargo test --workspace` green (verify accept/reject/expiry, tier ordering, gate
   refusals); `tsc --noEmit` clean.
6. `--no-default-features` core build is still a working free app (plan degrades to
   Free; pro features simply absent).

## Scope fence

- Offline verification only — **no license server, no phone-home** (local-first).
- Private signing key never enters a repo or the build machine.
- No real payment integration here (test keypair only); MoR wiring is a documented
  launch follow-up.
- Don't move the free/paid line — SEPARATE fixed it (worktrees + TV free).

## When done

Evidence (the three license states exercised + tamper test + counts). Tick
MONETIZE in PLAN.md; note the launch follow-up (MoR account + prod pubkey) and that
PUBLISH is unblocked. Commit:
`MONETIZE: offline license gate — Free/Pro/Remote tiers, per-feature entitlement`.

## Evidence

(builder fills this in)
