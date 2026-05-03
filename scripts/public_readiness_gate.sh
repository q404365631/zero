#!/usr/bin/env bash
set -euo pipefail

status=0

echo "== public readiness gate =="

find . \
  -path ./.git -prune -o \
  -path ./cli/target -prune -o \
  -path ./node_modules -prune -o \
  -type d \( \
    -name __pycache__ -o \
    -name .pytest_cache -o \
    -name .ruff_cache -o \
    -name .mypy_cache \
  \) -prune -exec rm -rf {} +

echo "-- community health files"
required=(
  README.md
  LICENSE
  NOTICE
  CONTRIBUTING.md
  CODE_OF_CONDUCT.md
  SECURITY.md
  SUPPORT.md
  GOVERNANCE.md
  AGENTS.md
  .github/PULL_REQUEST_TEMPLATE.md
  .github/dependabot.yml
  .github/workflows/ci.yml
  .github/workflows/codeql.yml
  .github/workflows/secret-scan.yml
  .github/workflows/scorecard.yml
)
for path in "${required[@]}"; do
  if [[ ! -f "$path" ]]; then
    echo "missing: $path" >&2
    status=1
  fi
done

echo "-- generated/cache artifacts"
if find . \
  -path ./.git -prune -o \
  -path ./cli/target -prune -o \
  -path ./node_modules -prune -o \
  -type d \( \
    -name __pycache__ -o \
    -name .pytest_cache -o \
    -name .ruff_cache -o \
    -name .mypy_cache \
  \) -print | sed -n '1,200p' | grep .; then
  status=1
else
  echo "ok"
fi

echo "-- forbidden publish artifacts"
if find . \
  -path ./.git -prune -o \
  -path ./cli/target -prune -o \
  -path ./node_modules -prune -o \
  -type f \( \
    -name '*.pyc' -o \
    -name '*.pyo' -o \
    -name '*.db' -o \
    -name '*.db-*' -o \
    -name '*.sqlite' -o \
    -name '*.sqlite3' -o \
    -name '*.wal' -o \
    -name '*.shm' -o \
    -name '.env' -o \
    -name '.env.local' -o \
    -name '*.pem' -o \
    -name '*.key' \
  \) -print | sed -n '1,200p' | grep .; then
  status=1
else
  echo "ok"
fi

echo "-- forbidden private markers"
if rg -n \
  --glob '!.git/**' \
  --glob '!cli/target/**' \
  --glob '!node_modules/**' \
  --glob '!scripts/public_readiness_gate.sh' \
  'github\.com/squaeragent/zero|github\.com/getzero/zero|sigstore://github\.com/getzero/zero|PROPRIETARY SOFTWARE|ALL RIGHTS RESERVED|VPS_IP|VPS_SSH|\.env\.secrets|204\.168\.|zero-private|/Users/forge/zero' \
  .; then
  status=1
else
  echo "ok"
fi

echo "-- public product honesty"
rg -q "Autonomous operating system for self-custodial onchain operations" README.md
rg -q "paper mode" README.md
rg -q "not yet a complete autonomous capital terminal" docs/production-readiness.md
rg -q "Do not publish this private monorepo wholesale" docs/public-upgrade.md

if [[ "$status" -eq 0 ]]; then
  echo "public readiness gate passed"
else
  echo "public readiness gate failed" >&2
fi

exit "$status"
