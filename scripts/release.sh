#!/usr/bin/env bash
#
# release.sh — build Hypervisor + publish the site to hypervisor.sh.
#
# Uploads the DMG + updater manifest to S3, syncs the static site, invalidates
# CloudFront. Idempotent: re-running without a version bump re-uploads the same
# bytes (harmless); a bumped version ships a new release path.
#
# Phase 1 (current): unsigned build — no Apple Developer ID on this machine, so
# there is no codesign/notarize step and latest.json carries an empty signature
# (updater is Phase-2 scaffolding, not active). The site's unsigned note stays.
#
# Required env / config:
#   BUCKET   S3 site bucket           (default: hypervisor-sh-site)
#   DIST_ID  CloudFront distribution  (from scripts/infra.sh output — required)
#   POSTHOG_PROJECT_KEY / POSTHOG_HOST  production analytics (warns if unset)
#
# Never commits secrets; reads them from env / the aws profile only.

set -euo pipefail
cd "$(dirname "$0")/.."   # repo root

BUCKET="${BUCKET:-hypervisor-sh-site}"
DIST_ID="${DIST_ID:-}"
export AWS_PAGER=""

die()  { printf '\033[1;31m✗ %s\033[0m\n' "$*" >&2; exit 1; }
say()  { printf '\n\033[1;36m▸ %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m! %s\033[0m\n' "$*"; }

# ── preflight ───────────────────────────────────────────────────────────────
[ -n "$(git status --porcelain)" ] && die "working tree is dirty — commit or stash first"
[ -n "$DIST_ID" ] || die "DIST_ID unset (run scripts/infra.sh, then export DIST_ID=<id>)"
command -v aws >/dev/null || die "aws cli not found"

if [ -z "${POSTHOG_PROJECT_KEY:-}" ]; then
  warn "POSTHOG_PROJECT_KEY unset — shipping a KEYLESS build (analytics inert)."
  warn "Set the production key before a real release, or this ships without analytics."
fi
export POSTHOG_PROJECT_KEY="${POSTHOG_PROJECT_KEY:-}"
export POSTHOG_HOST="${POSTHOG_HOST:-https://us.i.posthog.com}"

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null \
  || grep -m1 '"version"' src-tauri/tauri.conf.json | sed -E 's/.*"version": *"([^"]+)".*/\1/')"
[ -n "$VERSION" ] || die "could not read version from tauri.conf.json"
say "releasing Hypervisor v${VERSION} → s3://${BUCKET} (dist ${DIST_ID})"

# ── build the app ───────────────────────────────────────────────────────────
say "building app (npm run tauri build)"
npm run tauri build

DMG="$(ls -t src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1)"
[ -n "$DMG" ] && [ -f "$DMG" ] || die "no DMG produced under target/release/bundle/dmg/"
SHA256="$(shasum -a 256 "$DMG" | awk '{print $1}')"
say "built $(basename "$DMG")  sha256 ${SHA256}"

# ── build the site (inject version + sha256 + prod analytics key) ───────────
say "building site"
( cd site && npm install --silent && RELEASE_VERSION="$VERSION" RELEASE_SHA256="$SHA256" npm run build )

# ── updater manifest (Phase-2 scaffold; empty signature until signing lands) ─
PUB_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
cat > site/dist/latest.json <<JSON
{
  "version": "${VERSION}",
  "notes": "Hypervisor ${VERSION}",
  "pub_date": "${PUB_DATE}",
  "platforms": {
    "darwin-aarch64": {
      "signature": "",
      "url": "https://hypervisor.sh/releases/v${VERSION}/$(basename "$DMG")"
    }
  }
}
JSON

# ── upload: DMG (versioned archival + stable latest) ────────────────────────
say "uploading artifacts"
aws s3 cp "$DMG" "s3://${BUCKET}/releases/v${VERSION}/$(basename "$DMG")" \
  --content-type application/x-apple-diskimage --cache-control "public,max-age=31536000,immutable"
aws s3 cp "$DMG" "s3://${BUCKET}/releases/latest/Hypervisor.dmg" \
  --content-type application/x-apple-diskimage --cache-control "public,max-age=60"
aws s3 cp site/dist/latest.json "s3://${BUCKET}/releases/latest.json" \
  --content-type application/json --cache-control "public,max-age=60"

# ── sync the static site (everything except the releases/ tree) ─────────────
aws s3 sync site/dist/ "s3://${BUCKET}/" \
  --exclude "releases/*" --exclude "latest.json" \
  --cache-control "public,max-age=300" --delete

# ── invalidate CloudFront ───────────────────────────────────────────────────
say "invalidating CloudFront ${DIST_ID}"
aws cloudfront create-invalidation --distribution-id "$DIST_ID" --paths "/*" \
  --query 'Invalidation.{id:Id,status:Status}' --output table

say "done → https://hypervisor.sh  (v${VERSION})"
