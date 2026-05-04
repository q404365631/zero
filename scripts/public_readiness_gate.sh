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
  docs/memory-core.md
  docs/genesis.md
  docs/evolve.md
  docs/research.md
  docs/decision-stack.md
  docs/private-engine-capability-gap-audit.md
  docs/label-taxonomy.md
  docs/proof/README.md
  docs/proof/demo/README.md
  docs/proof/demo/proof-pack.json
  docs/proof/demo/paper-decisions.csv
  docs/proof/demo/paper-proof.svg
  docs/proof/network/README.md
  docs/proof/network/network-proof-pack.json
  docs/proof/network/profile.json
  docs/proof/network/profile-verification.json
  docs/proof/network/leaderboard.json
  docs/proof/network/identity/identity_bundle.json
  docs/proof/network/identity/SHA256SUMS
  docs/proof/live/README.md
  docs/proof/live/live-trading-evidence.json
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
  .github/CODEOWNERS
  .github/labels.yml
  Formula/zero.rb
  docs/review-ownership.md
  scripts/github_label_sync.py
  scripts/codeowners_check.py
  scripts/homebrew_formula_check.py
  scripts/stale_artifact_check.sh
  scripts/live_trading_evidence.py
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

echo "-- stale local artifacts"
if scripts/stale_artifact_check.sh --clean; then
  echo "ok"
else
  status=1
fi

echo "-- review ownership"
if scripts/codeowners_check.py; then
  echo "ok"
else
  status=1
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
file_contains "Self-evolution" docs/private-engine-capability-gap-audit.md
file_contains "Full ZERO operating-system readiness: 100/100" docs/production-readiness.md
file_contains "zero.runtime.production_parity.v1" docs/production-readiness.md
file_contains "zero.live_canary_policy.v1" docs/live-canary-operator.md
file_contains "/live/canary-policy" docs/live-evidence.md
file_contains "zero.memory.entry.v1" docs/memory-core.md
file_contains "zero.genesis.proposal.v1" docs/genesis.md
file_contains "zero.evolve.run.v1" docs/evolve.md
file_contains "zero.evolve.promotion_plan.v1" docs/evolve.md
file_contains "zero.evolve.rollback_plan.v1" docs/evolve.md
file_contains "zero.evolve.promotion_verification.v1" docs/evolve.md
file_contains "zero.evolve.apply_receipt.v1" docs/evolve.md
file_contains "zero.evolve.rollback_receipt.v1" docs/evolve.md
file_contains "zero.runtime.production_parity.v1" docs/runtime-bus.md
file_contains "zero.research.report.v1" docs/research.md
file_contains "zero.decision.stack.v1" docs/decision-stack.md
file_contains "zero.deployment_identity_evidence.v1" docs/deployment-identity.md
file_contains "zero.network.profile_verification.v1" docs/zero-network.md
file_contains "zero.network_proof_pack.v1" docs/proof/README.md
file_contains "zero.network_proof_pack.v1" docs/proof/network/network-proof-pack.json
file_contains "zero.network.profile_verification.v1" docs/proof/network/profile-verification.json
file_contains "zero.live_trading_evidence.v1" docs/proof/live/live-trading-evidence.json
file_contains "redacted private live evidence" docs/production-readiness.md
file_contains "Do not publish this private monorepo wholesale" docs/public-upgrade.md
file_contains "brew tap zero-intel/zero" README.md
file_contains "brew tap zero-intel/zero" docs/release.md
file_contains "The public runtime defaults to paper mode" Formula/zero.rb

echo "-- homebrew formula"
if scripts/homebrew_formula_check.py; then
  echo "ok"
else
  status=1
fi

if [[ "$status" -eq 0 ]]; then
  echo "public readiness gate passed"
else
  echo "public readiness gate failed" >&2
fi

exit "$status"
