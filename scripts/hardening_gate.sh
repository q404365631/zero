#!/usr/bin/env bash
set -euo pipefail

required_files=(
  "docs/threat-model.md"
  "docs/incident-runbooks.md"
  "docs/distribution.md"
  "docs/safety-model.md"
  "docs/release.md"
  "docs/production-readiness.md"
  ".github/RELEASE_TEMPLATE.md"
)

for file in "${required_files[@]}"; do
  test -f "$file"
done

rg -q "Private key committed or logged" docs/threat-model.md
rg -q "Public Packet Privacy Regression" docs/incident-runbooks.md
rg -q "Unexpected Live Order" docs/incident-runbooks.md
rg -q "Bad Release Artifact" docs/incident-runbooks.md
rg -q "Homebrew Formula Requirements" docs/distribution.md
rg -q "GitHub artifact attestations" docs/release.md
rg -q "threat model" docs/production-readiness.md
rg -q "incident runbooks" docs/production-readiness.md
rg -q "shasum -a 256 -c SHA256SUMS" .github/RELEASE_TEMPLATE.md
rg -q "gh attestation verify zero-linux" .github/RELEASE_TEMPLATE.md

python3 -m json.tool contracts/intelligence/snapshot.json >/dev/null
python3 -m json.tool contracts/intelligence/catalog.json >/dev/null
python3 -m json.tool contracts/intelligence/commercial.json >/dev/null
python3 -m json.tool contracts/intelligence/model_gateway.json >/dev/null
python3 -m json.tool contracts/deployment/claim.json >/dev/null
python3 -m json.tool contracts/deployment/heartbeat.json >/dev/null

bash -n scripts/assemble_release_assets.sh
bash -n scripts/install.sh
bash -n scripts/package_dry_run.sh
bash -n scripts/paper_api_smoke.sh
bash -n scripts/railway_smoke.sh
bash -n scripts/railway_start.sh
