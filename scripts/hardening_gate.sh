#!/usr/bin/env bash
set -euo pipefail

required_files=(
  "AGENTS.md"
  "CLAUDE.md"
  "GEMINI.md"
  ".claude/commands/README.md"
  ".claude/commands/paper-backtest.md"
  ".claude/commands/verify-schema.md"
  ".claude/commands/proof-pack.md"
  ".claude/commands/mcp-transcript.md"
  ".claude/commands/new-strategy.md"
  ".github/ISSUE_TEMPLATE/agent_task.yml"
  ".github/ISSUE_TEMPLATE/bug_report.yml"
  ".github/ISSUE_TEMPLATE/design_review.yml"
  ".github/ISSUE_TEMPLATE/docs_gap.yml"
  ".github/ISSUE_TEMPLATE/feature_request.yml"
  ".github/ISSUE_TEMPLATE/safety_review.yml"
  ".github/ISSUE_TEMPLATE/strategy_example.yml"
  ".github/ISSUE_TEMPLATE/config.yml"
  ".github/CODEOWNERS"
  ".github/labels.yml"
  "docs/review-ownership.md"
  "scripts/codeowners_check.py"
  "scripts/stale_artifact_check.sh"
  "llms.txt"
  "docs/llms.txt"
  "docs/llms-full.txt"
  "docs/proof/README.md"
  "docs/proof/demo/README.md"
  "docs/proof/demo/proof-pack.json"
  "docs/proof/demo/paper-decisions.csv"
  "docs/proof/demo/paper-proof.svg"
  "docs/threat-model.md"
  "docs/incident-runbooks.md"
  "docs/distribution.md"
  "docs/safety-model.md"
  "docs/release.md"
  "docs/releases/v0.1.1-evidence.md"
  "docs/production-readiness.md"
  "docs/public-upgrade.md"
  "docs/private-engine-capability-gap-audit.md"
  "docs/label-taxonomy.md"
  "docs/mcp.md"
  "docs/mcp/transcript.jsonl"
  "docs/memory-core.md"
  "docs/genesis.md"
  "docs/evolve.md"
  "docs/research.md"
  "docs/decision-stack.md"
  "docs/live-evidence.md"
  ".github/RELEASE_TEMPLATE.md"
)

for file in "${required_files[@]}"; do
  test -f "$file"
done

test -L CLAUDE.md
test -L GEMINI.md
test "$(readlink CLAUDE.md)" = "AGENTS.md"
test "$(readlink GEMINI.md)" = "AGENTS.md"
test -f .cursor/rules/global.mdc

contains() {
  local pattern="$1"
  local file="$2"

  if command -v rg >/dev/null 2>&1; then
    rg -q "$pattern" "$file"
    return
  fi

  grep -Eq "$pattern" "$file"
}

contains "Private key committed or logged" docs/threat-model.md
contains "Public Packet Privacy Regression" docs/incident-runbooks.md
contains "Unexpected Live Order" docs/incident-runbooks.md
contains "Bad Release Artifact" docs/incident-runbooks.md
contains "Dependency And Supply Chain Policy" docs/dependency-policy.md
contains "Vulnerability Response" docs/dependency-policy.md
contains "Homebrew Formula Requirements" docs/distribution.md
contains "scripts/homebrew_formula.py" docs/distribution.md
contains "Trusted Publishing" docs/distribution.md
contains "cargo owner" docs/distribution.md
contains "GitHub artifact attestations" docs/release.md
contains "SBOM.spdx.json" docs/release.md
contains "PROVENANCE.json" docs/release.md
contains "just release-evidence v0.1.1" docs/release.md
contains "just registry-readiness" docs/release.md
contains "release rehearsal" docs/release.md
contains "draft release rehearsal" docs/release.md
contains "threat model" docs/production-readiness.md
contains "incident runbooks" docs/production-readiness.md
contains "Public repo readiness" docs/public-upgrade.md
contains "Full ZERO operating-system readiness" docs/public-upgrade.md
contains "Private Engine Capability Gap Audit" docs/private-engine-capability-gap-audit.md
contains "Cycle 28: Memory Core" docs/private-engine-capability-gap-audit.md
contains "Cycle 29: Genesis Proposal Core" docs/private-engine-capability-gap-audit.md
contains "Cycle 31: Research Command Chain" docs/private-engine-capability-gap-audit.md
contains "Cycle 32: Decision Stack" docs/private-engine-capability-gap-audit.md
contains "zero.genesis.proposal.v1" docs/genesis.md
contains "zero.evolve.run.v1" docs/evolve.md
contains "zero.evolve.promotion_plan.v1" docs/evolve.md
contains "zero.evolve.rollback_plan.v1" docs/evolve.md
contains "zero.evolve.promotion_verification.v1" docs/evolve.md
contains "zero.evolve.apply_receipt.v1" docs/evolve.md
contains "zero.evolve.rollback_receipt.v1" docs/evolve.md
contains "zero.research.report.v1" docs/research.md
contains "zero.decision.stack.v1" docs/decision-stack.md
contains "zero_get_genesis_proposals" docs/mcp/transcript.jsonl
contains "zero://genesis/proposals" docs/mcp/transcript.jsonl
contains "zero_get_evolve_status" docs/mcp/transcript.jsonl
contains "zero://evolve/status" docs/mcp/transcript.jsonl
contains "zero_get_research_report" docs/mcp/transcript.jsonl
contains "zero://research/report" docs/mcp/transcript.jsonl
contains "zero_get_decision_stack" docs/mcp/transcript.jsonl
contains "zero://decision/stack" docs/mcp/transcript.jsonl
contains "zero.live_evidence.v1" docs/live-evidence.md
contains "ZERO_LIVE_EVIDENCE_SIGNING_KEY" docs/live-evidence.md
contains "zero.live_canary_policy.v1" docs/live-canary-operator.md
contains "/live/canary-policy" docs/live-evidence.md
contains "scripts/live_canary_policy.py" docs/live-canary-operator.md
contains "shasum -a 256 -c SHA256SUMS" .github/RELEASE_TEMPLATE.md
contains "package registry publication remains disabled" .github/RELEASE_TEMPLATE.md
contains "gh attestation verify zero-linux" .github/RELEASE_TEMPLATE.md
contains "scripts/release_evidence.py <tag>" .github/RELEASE_TEMPLATE.md
contains "zero.release_evidence.v1" docs/releases/v0.1.1-evidence.md
contains "verification.fail=0" docs/releases/v0.1.1-evidence.md
contains "ZERO LLM Full Context" docs/llms-full.txt
contains "read-only" docs/mcp.md
contains "zero-mcp" docs/mcp.md
contains "zero_get_paper_results" docs/mcp/transcript.jsonl
contains "zero_get_memory_snapshot" docs/mcp/transcript.jsonl
contains "zero_get_runtime_status" docs/mcp/transcript.jsonl
contains "zero_get_runtime_parity" docs/mcp/transcript.jsonl
contains "zero_get_health" docs/mcp/transcript.jsonl
contains "zero_get_journal_tail" docs/mcp/transcript.jsonl
contains "zero_get_rejection_audit" docs/mcp/transcript.jsonl
contains "zero_get_memory_stats" docs/mcp/transcript.jsonl
contains "zero_get_immune_status" docs/mcp/transcript.jsonl
contains "zero_get_backtest_report" docs/mcp/transcript.jsonl
contains "zero_get_evidence_bundle" docs/mcp/transcript.jsonl
contains "zero_get_safety_catalog" docs/mcp/transcript.jsonl
contains "read_only_public" docs/mcp/transcript.jsonl
contains "zero://proof/demo" docs/mcp/transcript.jsonl
contains "zero://memory/snapshot" docs/mcp/transcript.jsonl
contains "zero://mcp/safety" docs/mcp/transcript.jsonl
contains "zero://runtime/parity" docs/mcp/transcript.jsonl
contains "zero.memory.entry.v1" docs/memory-core.md
contains "zero.runtime.production_parity.v1" docs/runtime-bus.md
contains "Machine-readable entrypoints" README.md
contains "Stewardship Pledge" GOVERNANCE.md
contains "CODEOWNERS" GOVERNANCE.md
contains "Review Ownership" docs/review-ownership.md
contains "/engine/src/zero_engine/live.py" .github/CODEOWNERS
contains "/cli/crates/zero-commands/" .github/CODEOWNERS
contains "/contracts/" .github/CODEOWNERS
contains "canonical operating guide" .github/copilot-instructions.md
contains "ZERO Agent Commands" .claude/commands/README.md
contains "zero-mcp" .claude/commands/mcp-transcript.md
contains "paper-only" .claude/commands/proof-pack.md
contains "AI Assistance" .github/PULL_REQUEST_TEMPLATE.md
contains "agent-eligible" .github/ISSUE_TEMPLATE/agent_task.yml
contains "safety-critical" .github/ISSUE_TEMPLATE/safety_review.yml
contains "good-first-strategy" .github/ISSUE_TEMPLATE/strategy_example.yml
contains "design-review" .github/ISSUE_TEMPLATE/design_review.yml
contains "docs-gap" .github/ISSUE_TEMPLATE/docs_gap.yml
contains "Open-Core Boundary" .github/ISSUE_TEMPLATE/feature_request.yml
contains "Safety Impact" .github/ISSUE_TEMPLATE/bug_report.yml
contains "Agentic contribution guide" .github/ISSUE_TEMPLATE/config.yml
contains "proof-pack" .github/labels.yml
contains "mcp" .github/labels.yml
contains "market-data" .github/labels.yml
contains "network" .github/labels.yml
contains "security" .github/labels.yml
contains "containers" .github/labels.yml
contains "packaging" .github/labels.yml
contains "Label Taxonomy" docs/label-taxonomy.md
contains "safety-critical" docs/label-taxonomy.md
contains "Agent Operating Guide" docs/llms.txt
contains "live_correlation" docs/proof/demo/proof-pack.json
contains "unavailable" docs/proof/demo/proof-pack.json
contains "does not claim live trading" docs/proof/README.md

python3 -m json.tool contracts/intelligence/snapshot.json >/dev/null
python3 -m json.tool contracts/intelligence/catalog.json >/dev/null
python3 -m json.tool contracts/intelligence/commercial.json >/dev/null
python3 -m json.tool contracts/intelligence/model_gateway.json >/dev/null
python3 -m json.tool contracts/deployment/claim.json >/dev/null
python3 -m json.tool contracts/deployment/heartbeat.json >/dev/null
python3 -m json.tool contracts/live/evidence.json >/dev/null
python3 -m json.tool contracts/live/receipts.json >/dev/null

bash -n scripts/assemble_release_assets.sh
bash -n scripts/install.sh
bash -n scripts/package_dry_run.sh
bash -n scripts/release_rehearsal.sh
bash -n scripts/draft_release_rehearsal.sh
bash -n scripts/paper_api_smoke.sh
bash -n scripts/fresh_clone_rehearsal.sh
bash -n scripts/railway_smoke.sh
bash -n scripts/railway_start.sh
bash -n scripts/deployment_evidence.sh
bash -n scripts/stale_artifact_check.sh
python3 -m py_compile scripts/railway_doctor.py
python3 -m py_compile scripts/deployment_identity_evidence.py
python3 -m py_compile scripts/deployment_evidence.py
python3 -m py_compile scripts/deployment_evidence_verify.py
python3 -m py_compile scripts/deployment_rollback_rehearsal.py
python3 -m py_compile scripts/release_verify.py
python3 -m py_compile scripts/release_evidence.py
python3 -m py_compile scripts/registry_readiness.py
python3 -m py_compile scripts/release_provenance.py
python3 -m py_compile scripts/homebrew_formula.py
python3 -m py_compile scripts/generate_llms_full.py
python3 -m py_compile scripts/proof_pack.py
python3 -m py_compile scripts/mcp_transcript.py
python3 -m py_compile scripts/issue_template_check.py
python3 -m py_compile scripts/label_taxonomy_check.py
python3 -m py_compile scripts/github_label_sync.py
python3 -m py_compile scripts/codeowners_check.py
PYTHONPATH="$PWD/engine/src" python3 -m py_compile engine/src/zero_engine/mcp.py
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.mcp --smoke >/dev/null
scripts/issue_template_check.py >/dev/null
scripts/label_taxonomy_check.py >/dev/null
scripts/github_label_sync.py --validate-config >/dev/null
scripts/codeowners_check.py >/dev/null
python3 -m py_compile scripts/live_cockpit_drill.py
python3 -m py_compile scripts/live_cockpit_drill_verify.py
python3 -m py_compile scripts/live_cockpit_drill_tamper_rehearsal.py
python3 -m py_compile scripts/live_canary_policy.py
rm -rf scripts/__pycache__
scripts/registry_readiness.py >/dev/null
PYTHONPATH="$PWD/engine/src" scripts/mcp_transcript.py --check
scripts/generate_llms_full.py --check
PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py --check
scripts/draft_release_rehearsal.sh >/dev/null
rm -rf scripts/__pycache__
scripts/stale_artifact_check.sh --clean >/dev/null
scripts/public_readiness_gate.sh >/dev/null
