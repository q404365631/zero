set shell := ["bash", "-uc"]
export PYTHONDONTWRITEBYTECODE := "1"

bootstrap:
    cd engine && python3 -m pip install -e ".[dev]"

demo:
    cd engine && python3 -m zero_engine.demo

example:
    PYTHONPATH="$PWD/engine/src" python3 examples/paper-trading/run.py

strategy-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/paper-trading/strategy_demo.py

strategy-plugin-example:
    PYTHONPATH="$PWD/engine/src:$PWD/examples/strategy-plugin" python3 examples/strategy-plugin/run.py

strategy-runner-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/strategy-runner/run.py

market-data-adapter-example:
    PYTHONPATH="$PWD/engine/src:$PWD/examples/market-data-adapter" python3 examples/market-data-adapter/run.py

network-leaderboard-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/network-leaderboard/build.py

network-profile-page-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/network-profile-page/build.py

network-leaderboard-page-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/network-leaderboard-page/build.py

network-index-page-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/network-index-page/build.py

runtime-loop-example:
    PYTHONPATH="$PWD/engine/src" python3 examples/runtime-loop/run.py

memory-core-example:
    rm -rf artifacts/memory-example
    PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.memory extract --decisions examples/memory-core/decisions.jsonl --store artifacts/memory-example/memory.jsonl --knowledge artifacts/memory-example/knowledge.md --now 2026-05-01T00:00:00Z
    test -f artifacts/memory-example/memory.jsonl
    test -f artifacts/memory-example/knowledge.md
    ! rg '40500|1400|notional_usd|private_key|wallet_address' artifacts/memory-example

genesis-example:
    rm -rf artifacts/genesis-example
    PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.genesis plan --proposals examples/genesis/proposals.jsonl --journal artifacts/genesis-example/genesis.jsonl --now 2026-05-01T00:00:00Z
    PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.genesis status --journal artifacts/genesis-example/genesis.jsonl --now 2026-05-01T00:00:00Z
    test -f artifacts/genesis-example/genesis.jsonl
    rg '"decision":"accepted"' artifacts/genesis-example/genesis.jsonl
    rg '"decision":"rejected"' artifacts/genesis-example/genesis.jsonl
    rg '"decision":"escalated"' artifacts/genesis-example/genesis.jsonl
    ! rg 'private_key|wallet_address|exchange_order_id|notional_usd' artifacts/genesis-example

network-pages-smoke:
    scripts/network_pages_smoke.py

paper-api:
    cd engine && python3 -m zero_engine.api

paper-api-smoke:
    scripts/paper_api_smoke.sh

fresh-clone-rehearsal:
    scripts/fresh_clone_rehearsal.sh

demo-capture:
    scripts/demo_capture.sh

issue-template-check:
    scripts/issue_template_check.py

label-taxonomy-check:
    scripts/label_taxonomy_check.py

github-label-config-check:
    scripts/github_label_sync.py --validate-config

github-label-check repo="zero-intel/zero":
    scripts/github_label_sync.py --repo "{{repo}}" --check

github-label-sync repo="zero-intel/zero":
    scripts/github_label_sync.py --repo "{{repo}}" --apply

codeowners-check:
    scripts/codeowners_check.py

stale-artifact-check:
    scripts/stale_artifact_check.sh --check

stale-artifact-clean:
    scripts/stale_artifact_check.sh --clean

hardening-gate:
    scripts/hardening_gate.sh

public-readiness:
    scripts/public_readiness_gate.sh

package-dry-run:
    scripts/package_dry_run.sh

registry-readiness:
    scripts/registry_readiness.py

release-rehearsal:
    scripts/release_rehearsal.sh

release-verify dir:
    scripts/release_verify.py "{{dir}}"

release-provenance dir:
    scripts/release_provenance.py "{{dir}}"

release-evidence tag:
    scripts/release_evidence.py "{{tag}}"

llms-full:
    scripts/generate_llms_full.py

proof-pack:
    PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py

draft-release-rehearsal:
    scripts/draft_release_rehearsal.sh

homebrew-formula release_dir tag:
    scripts/homebrew_formula.py "{{release_dir}}" --tag "{{tag}}"

railway-smoke:
    scripts/railway_smoke.sh

deployment-evidence url:
    scripts/deployment_evidence.sh "{{url}}"

live-canary-rehearsal url:
    scripts/live_canary_rehearsal.py "{{url}}"

live-canary-verify dir:
    scripts/live_canary_verify.py "{{dir}}"

live-canary-exchange-evidence bundle source:
    scripts/live_canary_exchange_evidence.py "{{bundle}}" "{{source}}"

live-canary-operator url:
    scripts/live_canary_operator.py "{{url}}"

live-canary-operator-verify workflow:
    scripts/live_canary_operator_verify.py "{{workflow}}"

live-cockpit-drill url="http://127.0.0.1:8765":
    scripts/live_cockpit_drill.py "{{url}}"

live-cockpit-drill-verify dir:
    scripts/live_cockpit_drill_verify.py "{{dir}}"

live-cockpit-drill-tamper-rehearsal dir:
    scripts/live_cockpit_drill_tamper_rehearsal.py "{{dir}}"

checksum output *artifacts:
    python3 scripts/write_sha256s.py "{{output}}" {{artifacts}}

engine-lint:
    cd engine && ruff check --no-cache .

engine-format:
    cd engine && ruff format .

engine-test:
    cd engine && PYTHONPATH="$PWD/src" pytest -p no:cacheprovider

cli-lint:
    cd cli && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings

cli-test:
    cd cli && cargo test --workspace

docs-check:
    test -f AGENTS.md
    test -L CLAUDE.md
    test -L GEMINI.md
    test "$(readlink CLAUDE.md)" = "AGENTS.md"
    test "$(readlink GEMINI.md)" = "AGENTS.md"
    test -f .cursor/rules/global.mdc
    test -f .claude/commands/README.md
    test -f .claude/commands/paper-backtest.md
    test -f .claude/commands/verify-schema.md
    test -f .claude/commands/proof-pack.md
    test -f .claude/commands/mcp-transcript.md
    test -f .claude/commands/new-strategy.md
    test -f .github/copilot-instructions.md
    test -f .github/ISSUE_TEMPLATE/agent_task.yml
    test -f .github/ISSUE_TEMPLATE/bug_report.yml
    test -f .github/ISSUE_TEMPLATE/design_review.yml
    test -f .github/ISSUE_TEMPLATE/docs_gap.yml
    test -f .github/ISSUE_TEMPLATE/feature_request.yml
    test -f .github/ISSUE_TEMPLATE/safety_review.yml
    test -f .github/ISSUE_TEMPLATE/strategy_example.yml
    test -f .github/ISSUE_TEMPLATE/config.yml
    test -f .github/CODEOWNERS
    test -f .github/labels.yml
    test -f llms.txt
    test -f docs/llms.txt
    test -f docs/llms-full.txt
    test -f docs/proof/README.md
    test -f docs/proof/demo/README.md
    test -f docs/proof/demo/proof-pack.json
    test -f docs/proof/demo/paper-decisions.csv
    test -f docs/proof/demo/paper-proof.svg
    test -f docs/local-development.md
    test -f docs/first-10-minutes.md
    test -f docs/demo-terminal.md
    test -f docs/cli-quickstart.md
    test -f docs/api.md
    test -f docs/mcp.md
    test -f docs/mcp/transcript.jsonl
    test -f docs/api-compatibility.md
    test -f docs/runtime-bus.md
    test -f docs/memory-core.md
    test -f docs/genesis.md
    test -f docs/strategy-plugins.md
    test -f docs/market-data-adapters.md
    test -f docs/positioning.md
    test -f docs/open-core-boundary.md
    test -f docs/zero-network.md
    test -f docs/zero-intelligence.md
    test -f docs/threat-model.md
    test -f docs/incident-runbooks.md
    test -f docs/dependency-policy.md
    test -f docs/distribution.md
    test -f docs/hyperliquid-readonly.md
    test -f docs/live-certification.md
    test -f docs/live-cockpit.md
    test -f docs/live-evidence.md
    test -f docs/live-canary-operator.md
    test -f docs/operator-context.md
    test -f docs/deployment-identity.md
    test -f docs/model-gateway.md
    test -f docs/operator-isolation.md
    test -f docs/immune-system.md
    test -f docs/production-readiness.md
    test -f docs/public-upgrade.md
    test -f docs/autonomous-os-plan.md
    test -f docs/agentic-contribution.md
    test -f docs/label-taxonomy.md
    test -f docs/release.md
    test -f docs/releases/v0.1.1.md
    test -f docs/releases/v0.1.1-evidence.md
    test -f docs/launch-scorecard.md
    test -f docs/backlog.md
    test -f docs/launch-issues.md
    test -f .github/RELEASE_TEMPLATE.md
    test -f examples/paper-trading/run.py
    test -f examples/paper-trading/strategy_demo.py
    test -f examples/paper-trading/scenario.json
    test -f examples/paper-trading/candles.jsonl
    test -f examples/strategy-plugin/README.md
    test -f examples/strategy-plugin/plugin.py
    test -f examples/strategy-plugin/run.py
    test -f examples/strategy-runner/README.md
    test -f examples/strategy-runner/close-strength.yaml
    test -x examples/strategy-runner/run.py
    test -f examples/market-data-adapter/README.md
    test -f examples/market-data-adapter/adapter.py
    test -f examples/market-data-adapter/run.py
    test -f examples/runtime-loop/README.md
    test -x examples/runtime-loop/run.py
    test -f examples/memory-core/README.md
    test -f examples/memory-core/decisions.jsonl
    test -f examples/genesis/README.md
    test -f examples/genesis/proposals.jsonl
    test -f examples/network-leaderboard/README.md
    test -f examples/network-leaderboard/build.py
    test -f examples/network-leaderboard/profiles.jsonl
    test -f examples/network-profile-page/README.md
    test -f examples/network-profile-page/build.py
    test -f examples/network-leaderboard-page/README.md
    test -f examples/network-leaderboard-page/build.py
    test -f examples/network-index-page/README.md
    test -f examples/network-index-page/build.py
    test -f contracts/paper-api/v2_status.json
    test -f contracts/paper-api/execute_accepted.json
    test -f contracts/paper-api/execute_rejected.json
    test -f contracts/paper-api/memory.json
    test -f contracts/paper-api/genesis.json
    test -f contracts/deployment/claim.json
    test -f contracts/deployment/heartbeat.json
    test -f contracts/live/evidence.json
    test -f contracts/live/receipts.json
    test -f contracts/network/profile.json
    test -f contracts/network/leaderboard.json
    test -f contracts/network/ingestion.json
    test -f contracts/network/profile.html
    test -f contracts/network/leaderboard.html
    test -f contracts/network/index.html
    test -f contracts/intelligence/snapshot.json
    test -f contracts/intelligence/catalog.json
    test -f contracts/intelligence/commercial.json
    test -f contracts/intelligence/model_gateway.json
    test -f contracts/intelligence/model_gateway_health.json
    test -f contracts/intelligence/model_gateway_audit.json
    test -f openapi/zero-paper-api.v1.yaml
    test -x scripts/assemble_release_assets.sh
    test -x scripts/install.sh
    test -x scripts/demo_capture.sh
    test -x scripts/issue_template_check.py
    test -x scripts/label_taxonomy_check.py
    test -x scripts/github_label_sync.py
    test -x scripts/codeowners_check.py
    test -x scripts/stale_artifact_check.sh
    test -x scripts/openapi_contract_check.py
    test -x scripts/network_pages_smoke.py
    test -x scripts/package_dry_run.sh
    test -x scripts/registry_readiness.py
    test -x scripts/homebrew_formula.py
    test -x scripts/release_provenance.py
    test -x scripts/release_verify.py
    test -x scripts/release_evidence.py
    test -x scripts/generate_llms_full.py
    test -x scripts/proof_pack.py
    test -x scripts/release_rehearsal.sh
    test -x scripts/draft_release_rehearsal.sh
    test -x scripts/hardening_gate.sh
    test -x scripts/public_readiness_gate.sh
    test -x scripts/railway_start.sh
    test -x scripts/railway_doctor.py
    test -x scripts/deployment_evidence.py
    test -x scripts/deployment_evidence.sh
    test -x scripts/live_canary_rehearsal.py
    test -x scripts/live_canary_verify.py
    test -x scripts/live_canary_exchange_evidence.py
    test -x scripts/live_canary_operator.py
    test -x scripts/live_canary_operator_verify.py
    test -x scripts/live_cockpit_drill.py
    test -x scripts/live_cockpit_drill_verify.py
    test -x scripts/live_cockpit_drill_tamper_rehearsal.py
    test -x scripts/fresh_clone_rehearsal.sh
    test -x scripts/railway_smoke.sh
    test -f Dockerfile
    test -f compose.yaml
    test -f railway.toml
    test -f docs/railway-deploy.md
    python3 scripts/openapi_contract_check.py
    scripts/issue_template_check.py
    scripts/label_taxonomy_check.py
    scripts/github_label_sync.py --validate-config
    scripts/codeowners_check.py
    PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.mcp --smoke
    PYTHONPATH="$PWD/engine/src" scripts/mcp_transcript.py --check
    scripts/generate_llms_full.py --check
    PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py --check

container-build:
    docker build -t zero-public:local .

container-demo: container-build
    docker run --rm zero-public:local

container-example: container-build
    docker run --rm zero-public:local python /app/examples/paper-trading/run.py

lint: stale-artifact-clean engine-lint cli-lint docs-check issue-template-check label-taxonomy-check github-label-config-check codeowners-check hardening-gate public-readiness

test: engine-test cli-test

container-smoke:
    docker build -t zero-public:local .
    docker run --rm zero-public:local
    docker run --rm zero-public:local python /app/examples/paper-trading/run.py

ci: lint test paper-api-smoke fresh-clone-rehearsal example strategy-example strategy-plugin-example strategy-runner-example market-data-adapter-example runtime-loop-example memory-core-example genesis-example network-leaderboard-example network-profile-page-example network-leaderboard-page-example network-index-page-example network-pages-smoke registry-readiness package-dry-run release-rehearsal draft-release-rehearsal public-readiness
