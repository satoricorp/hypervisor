# DEPLOY — hypervisor.sh: distribution for a local-first app

**Prerequisite gate: REALIZE must be ticked in PLAN.md.** (Ship the honest
app, not the mock one.) If unticked, stop and report.

**What "deploy" means here — and what it must never mean.** Hypervisor is
local-first; nothing about the app runs on a server. Deploying =
(a) a website at hypervisor.sh, (b) signed downloadable artifacts,
(c) an update channel. **The iron rule: the app's remote server
(127.0.0.1:7428, the phone page) is tailnet-only, forever. It is never
deployed to AWS, never exposed publicly — a public approval endpoint is a
public remote-code-execution endpoint.** hypervisor.sh serves static files
only.

## Facts (verified 2026-07-10)

- AWS account `088950452464`, local CLI works (`user/joe`). Route53 hosts
  `gx.run` only — **hypervisor.sh DNS is at its registrar** (Joe action:
  either create a Route53 hosted zone + repoint NS, or add the ACM
  validation + CloudFront CNAMEs at the registrar).
- Reference pipeline: `~/git/gx/.github/workflows/macos-release.yaml` —
  build → `aws s3 cp` (latest + `releases/$SHA/` archival) → CloudFront
  domain. Copy its shape; secrets already exist in the satoricorp GitHub org.
- This repo's remote: `github.com/satoricorp/spf` (private).
- **No Apple Developer ID on this machine** (`security find-identity` → 0).
  Phase 1 ships unsigned; Phase 2 gates on Joe enrolling
  (developer.apple.com, $99/yr).

## Phase 1 — live at hypervisor.sh (no Apple account needed)

1. **Infra** (us-east-1 for ACM+CloudFront): private S3 bucket
   (`hypervisor-sh-site`) with Origin Access Control; CloudFront dist,
   default root `index.html`; ACM cert for `hypervisor.sh` +
   `www.hypervisor.sh` (DNS validation — hand Joe the exact records for the
   registrar and wait). Script it (`scripts/infra.sh`, plain aws cli,
   idempotent) rather than console clicking, so it's reviewable.
2. **The site**: `site/index.html` in-repo — single static page, our design
   tokens (dark, Berkeley Mono, Xer0 wordmark), the one-liner ("Conductor
   runs your agents; Hypervisor watches all of them…"), a download button,
   version + sha256, and the honest unsigned note: "first launch:
   right-click → Open (unsigned until notarization lands)". No analytics,
   no third-party assets (same zero-remote-content discipline as the app).
3. **Release script** `scripts/release.sh`: refuses on dirty tree; reads
   version from tauri.conf.json; exports `POSTHOG_PROJECT_KEY` +
   `POSTHOG_HOST` (from env/GitHub secrets — official analytics are baked
   into distributed builds per tasks/POSTHOG.md; the script warns loudly if
   they're unset so a keyless build isn't shipped by accident);
   `npm run tauri build`; uploads DMG + `latest.json` (tauri-updater
   manifest format, prepared for Phase 2) to `s3://…/releases/vX.Y.Z/` and
   `…/latest/`; syncs `site/`; CloudFront invalidation. Idempotent re-runs.
4. **CI**: `.github/workflows/macos-release.yaml` modeled on gx's — tag push
   `v*` → build on `macos-latest` → same uploads. Keep the local script as
   the fallback path.

## Phase 2 — real distribution quality (gate: Joe's Apple enrollment)

- Developer ID Application cert → tauri signing (`APPLE_CERTIFICATE`,
  `APPLE_ID`, `APPLE_TEAM_ID` secrets; hardened runtime) + `notarytool`
  submit/staple in CI. The unsigned note comes off the site.
- **tauri-plugin-updater**: `tauri signer generate` keypair — public key in
  tauri.conf.json, private key in GitHub secrets ONLY (never the repo, never
  the laptop keychain unmanaged); endpoint
  `https://hypervisor.sh/releases/latest.json`; app checks on launch,
  prompts to install. Version bump → tag → users update in-app.

## Definition of done (Phase 1; Phase 2 items ticked when unblocked)

1. `curl -I https://hypervisor.sh` → 200 over CloudFront/TLS.
2. Download the DMG on a clean macOS account; install; app launches
   (right-click → Open) and the live board works.
3. `releases/latest.json` validates against the tauri updater schema;
   sha256 on the site matches the artifact.
4. Re-running `scripts/release.sh` with no changes is a no-op that hurts
   nothing; with a version bump it ships and invalidates.
5. Tag-push CI produces the same artifacts as the local script.
6. Evidence records the exact DNS records handed to Joe and which path he
   chose (Route53 zone vs registrar records).
7. `python3 spike/compare.py --limit 20` OK (nothing here touches the app,
   prove it anyway).

## Scope fence

- Static site only. No server code on AWS. No telemetry or analytics.
- The remote/phone server config is untouched — do not "helpfully" add a
  public mode.
- Do not bypass Gatekeeper checks in docs (no `xattr -d com.apple.quarantine`
  instructions — right-click → Open is the honest path until notarization).
- Secrets never land in the repo; the release script reads env/aws profile.

## When done

Evidence (curl proof, clean-machine install note, DNS handoff, CI run link),
tick DEPLOY in PLAN.md, note Phase 2 blockers (Apple enrollment) — commit:
`DEPLOY: hypervisor.sh — static site, release pipeline, updater scaffolding`.

## Evidence

(builder fills this in)
