#!/usr/bin/env bash
set -euo pipefail

status=0

echo "== public readiness gate =="

repo_search() {
  local pattern="$1"

  if command -v rg >/dev/null 2>&1; then
    rg -n \
      --glob '!.git/**' \
      --glob '!cli/target/**' \
      --glob '!node_modules/**' \
      --glob '!scripts/public_readiness_gate.sh' \
      "$pattern" \
      .
    return
  fi

  find . \
    -path ./.git -prune -o \
    -path ./cli/target -prune -o \
    -path ./node_modules -prune -o \
    -type f \
    ! -path ./scripts/public_readiness_gate.sh \
    -print0 | xargs -0 grep -InE "$pattern"
}

file_contains() {
  local pattern="$1"
  local path="$2"

  if command -v rg >/dev/null 2>&1; then
    rg -q "$pattern" "$path"
    return
  fi

  grep -Eq "$pattern" "$path"
}

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
  CLAUDE.md
  GEMINI.md
  .claude/commands/README.md
  .claude/commands/paper-backtest.md
  .claude/commands/verify-schema.md
  .claude/commands/proof-pack.md
  .claude/commands/mcp-transcript.md
  .claude/commands/new-strategy.md
  llms.txt
  docs/llms.txt
  docs/llms-full.txt
  docs/mcp.md
  docs/mcp/transcript.jsonl
  docs/label-taxonomy.md
  docs/proof/README.md
  docs/proof/demo/README.md
  docs/proof/demo/proof-pack.json
  docs/proof/demo/paper-decisions.csv
  docs/proof/demo/paper-proof.svg
  .cursor/rules/global.mdc
  .github/PULL_REQUEST_TEMPLATE.md
  .github/ISSUE_TEMPLATE/agent_task.yml
  .github/ISSUE_TEMPLATE/bug_report.yml
  .github/ISSUE_TEMPLATE/design_review.yml
  .github/ISSUE_TEMPLATE/docs_gap.yml
  .github/ISSUE_TEMPLATE/feature_request.yml
  .github/ISSUE_TEMPLATE/safety_review.yml
  .github/ISSUE_TEMPLATE/strategy_example.yml
  .github/ISSUE_TEMPLATE/config.yml
  .github/labels.yml
  scripts/github_label_sync.py
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
if repo_search 'github\.com/squaeragent/zero|github\.com/getzero/zero|sigstore://github\.com/getzero/zero|PROPRIETARY SOFTWARE|ALL RIGHTS RESERVED|VPS_IP|VPS_SSH|\.env\.secrets|204\.168\.|zero-private|/Users/forge/zero'; then
  status=1
else
  echo "ok"
fi

echo "-- public product honesty"
file_contains "Autonomous operating system for self-custodial onchain operations" README.md
file_contains "paper mode" README.md
file_contains "not yet a complete autonomous capital terminal" docs/production-readiness.md
file_contains "Do not publish this private monorepo wholesale" docs/public-upgrade.md

if [[ "$status" -eq 0 ]]; then
  echo "public readiness gate passed"
else
  echo "public readiness gate failed" >&2
fi

exit "$status"
