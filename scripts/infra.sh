#!/usr/bin/env bash
#
# infra.sh — stand up the hypervisor.sh static-site infrastructure in AWS.
#
# Idempotent: safe to re-run. Creates a PRIVATE S3 bucket (served only through
# CloudFront via Origin Access Control), an ACM cert (us-east-1, DNS-validated),
# and a CloudFront distribution. It NEVER touches the app's remote server — this
# is static hosting only.
#
# Two passes, because ACM DNS validation needs records added at your registrar:
#   Pass 1  → creates the bucket + OAC, requests the cert, PRINTS the DNS
#             validation records, then stops (cert still PENDING_VALIDATION).
#   (you)   → add those records at the hypervisor.sh registrar (or Route53),
#             wait for ACM to flip the cert to ISSUED (minutes, up to ~30).
#   Pass 2  → creates/updates the CloudFront distribution + bucket policy and
#             PRINTS the final record to point hypervisor.sh at CloudFront.
#
# Requires: awscli v2, a profile with rights to S3/ACM/CloudFront. Account
# 088950452464 (user/joe) per tasks/DEPLOY.md.
#
# Config via env: DOMAIN, BUCKET, AWS_PROFILE.

set -euo pipefail

DOMAIN="${DOMAIN:-hypervisor.sh}"
ALT="www.${DOMAIN}"
BUCKET="${BUCKET:-hypervisor-sh-site}"
REGION="us-east-1"            # ACM certs for CloudFront MUST live in us-east-1.
OAC_NAME="hypervisor-sh-oac"
export AWS_PAGER=""           # never drop into a pager mid-script

say()  { printf '\n\033[1;36m▸ %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m! %s\033[0m\n' "$*"; }

ACCOUNT_ID="$(aws sts get-caller-identity --query Account --output text)"
say "AWS account ${ACCOUNT_ID} · domain ${DOMAIN} · bucket ${BUCKET}"
[ "$ACCOUNT_ID" = "088950452464" ] || warn "account is not the expected 088950452464 — continuing anyway"

# ── 1. private S3 bucket ────────────────────────────────────────────────────
if aws s3api head-bucket --bucket "$BUCKET" 2>/dev/null; then
  say "bucket ${BUCKET} already exists"
else
  say "creating private bucket ${BUCKET}"
  # us-east-1 is special: no LocationConstraint allowed.
  aws s3api create-bucket --bucket "$BUCKET" --region "$REGION" >/dev/null
fi
aws s3api put-public-access-block --bucket "$BUCKET" \
  --public-access-block-configuration \
  BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true

# ── 2. CloudFront Origin Access Control ─────────────────────────────────────
OAC_ID="$(aws cloudfront list-origin-access-controls \
  --query "OriginAccessControlList.Items[?Name=='${OAC_NAME}'].Id | [0]" --output text)"
if [ "$OAC_ID" = "None" ] || [ -z "$OAC_ID" ]; then
  say "creating Origin Access Control ${OAC_NAME}"
  OAC_ID="$(aws cloudfront create-origin-access-control \
    --origin-access-control-config \
    "Name=${OAC_NAME},Description=hypervisor.sh site,SigningProtocol=sigv4,SigningBehavior=always,OriginAccessControlOriginType=s3" \
    --query 'OriginAccessControl.Id' --output text)"
fi
say "OAC ${OAC_ID}"

# ── 3. ACM certificate (DNS-validated) ──────────────────────────────────────
CERT_ARN="$(aws acm list-certificates --region "$REGION" \
  --query "CertificateSummaryList[?DomainName=='${DOMAIN}'].CertificateArn | [0]" --output text)"
if [ "$CERT_ARN" = "None" ] || [ -z "$CERT_ARN" ]; then
  say "requesting ACM cert for ${DOMAIN} + ${ALT}"
  CERT_ARN="$(aws acm request-certificate --region "$REGION" \
    --domain-name "$DOMAIN" --subject-alternative-names "$ALT" \
    --validation-method DNS \
    --query CertificateArn --output text)"
  sleep 5   # give ACM a moment to populate the validation records
fi
say "cert ${CERT_ARN}"

CERT_STATUS="$(aws acm describe-certificate --region "$REGION" --certificate-arn "$CERT_ARN" \
  --query 'Certificate.Status' --output text)"

say "DNS VALIDATION RECORDS — add these at the ${DOMAIN} registrar (or Route53):"
aws acm describe-certificate --region "$REGION" --certificate-arn "$CERT_ARN" \
  --query 'Certificate.DomainValidationOptions[].{host:ResourceRecord.Name,type:ResourceRecord.Type,points_to:ResourceRecord.Value}' \
  --output table

if [ "$CERT_STATUS" != "ISSUED" ]; then
  warn "cert status is ${CERT_STATUS}. Add the CNAME record(s) above, then re-run this script."
  warn "watch: aws acm describe-certificate --region ${REGION} --certificate-arn ${CERT_ARN} --query 'Certificate.Status'"
  exit 0
fi
say "cert is ISSUED — continuing to CloudFront"

# ── 4. CloudFront distribution (S3 + OAC origin) ────────────────────────────
S3_DOMAIN="${BUCKET}.s3.${REGION}.amazonaws.com"
DIST_ID="$(aws cloudfront list-distributions \
  --query "DistributionList.Items[?Aliases.Items && contains(Aliases.Items, '${DOMAIN}')].Id | [0]" \
  --output text)"

if [ "$DIST_ID" = "None" ] || [ -z "$DIST_ID" ]; then
  say "creating CloudFront distribution"
  CONFIG="$(mktemp)"
  # CachingOptimized is the AWS-managed cache policy (stable well-known id).
  cat > "$CONFIG" <<JSON
{
  "CallerReference": "hypervisor-sh-$(date +%s)",
  "Aliases": { "Quantity": 2, "Items": ["${DOMAIN}", "${ALT}"] },
  "DefaultRootObject": "index.html",
  "Origins": { "Quantity": 1, "Items": [ {
      "Id": "s3-${BUCKET}",
      "DomainName": "${S3_DOMAIN}",
      "OriginAccessControlId": "${OAC_ID}",
      "S3OriginConfig": { "OriginAccessIdentity": "" }
  } ] },
  "DefaultCacheBehavior": {
    "TargetOriginId": "s3-${BUCKET}",
    "ViewerProtocolPolicy": "redirect-to-https",
    "CachePolicyId": "658327ea-f89d-4fab-a63d-7e88639e58f6",
    "Compress": true,
    "AllowedMethods": { "Quantity": 2, "Items": ["GET", "HEAD"],
      "CachedMethods": { "Quantity": 2, "Items": ["GET", "HEAD"] } }
  },
  "CustomErrorResponses": { "Quantity": 1, "Items": [ {
      "ErrorCode": 403, "ResponsePagePath": "/index.html",
      "ResponseCode": "200", "ErrorCachingMinTTL": 10
  } ] },
  "Comment": "hypervisor.sh static site",
  "Enabled": true,
  "ViewerCertificate": {
    "ACMCertificateArn": "${CERT_ARN}",
    "SSLSupportMethod": "sni-only",
    "MinimumProtocolVersion": "TLSv1.2_2021"
  },
  "HttpVersion": "http2and3"
}
JSON
  DIST_ID="$(aws cloudfront create-distribution --distribution-config "file://${CONFIG}" \
    --query 'Distribution.Id' --output text)"
  rm -f "$CONFIG"
else
  say "CloudFront distribution ${DIST_ID} already exists (leaving config as-is)"
fi

DIST_DOMAIN="$(aws cloudfront get-distribution --id "$DIST_ID" --query 'Distribution.DomainName' --output text)"
say "distribution ${DIST_ID} → ${DIST_DOMAIN}"

# ── 5. bucket policy: allow ONLY this distribution to read ───────────────────
say "attaching bucket policy (CloudFront service principal, scoped to this dist)"
aws s3api put-bucket-policy --bucket "$BUCKET" --policy "$(cat <<JSON
{
  "Version": "2012-10-17",
  "Statement": [ {
    "Sid": "AllowCloudFrontServicePrincipalReadOnly",
    "Effect": "Allow",
    "Principal": { "Service": "cloudfront.amazonaws.com" },
    "Action": "s3:GetObject",
    "Resource": "arn:aws:s3:::${BUCKET}/*",
    "Condition": { "StringEquals": {
      "AWS:SourceArn": "arn:aws:cloudfront::${ACCOUNT_ID}:distribution/${DIST_ID}"
    } }
  } ]
}
JSON
)"

# ── 6. final DNS + summary ──────────────────────────────────────────────────
say "DONE. Point the domain at CloudFront:"
cat <<TXT

  If DNS stays at the registrar (needs apex ALIAS/ANAME support):
    ${DOMAIN}       ALIAS/ANAME  ${DIST_DOMAIN}
    ${ALT}          CNAME        ${DIST_DOMAIN}

  If you move DNS to Route53 (cleaner apex — create a hosted zone, repoint the
  registrar NS to its 4 nameservers, then):
    ${DOMAIN}       A     (alias) → ${DIST_DOMAIN}
    ${ALT}          A     (alias) → ${DIST_DOMAIN}

  Bucket:        s3://${BUCKET}   (private; CloudFront-only)
  Distribution:  ${DIST_ID}  (${DIST_DOMAIN})
  Cert:          ${CERT_ARN}

  Next: publish the site with  scripts/release.sh  (uploads site/ + the DMG,
  then invalidates this distribution).
TXT
