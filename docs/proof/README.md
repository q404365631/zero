# Proof Packs

ZERO proof packs are public-safe artifacts that let a human or agent verify what
the runtime actually did.

Run the full public proof gate:

```bash
just public-proof
```

That command verifies the demo proof pack, the ZERO Network proof chain, the
redacted live trading evidence packet, the read-only MCP server, and the
committed MCP transcript together. It is the preferred pre-PR check when
changing proof, Network, MCP, or agent-facing docs.

The current committed pack is intentionally modest:

- it is generated from the deterministic paper scenario in
  `examples/paper-trading`;
- it includes a CSV of paper decisions, a small SVG summary, and a hash-addressed
  manifest;
- it explicitly does not claim live trading, PnL, or paper-vs-live correlation.

Proof packs are proof-of-process, not custody proof or PnL proof. A public pack
can show that a deterministic runtime, profile, leaderboard, and evidence chain
were generated and verified under redaction rules. The committed live trading
packet separately proves that operator-owned live execution evidence exists and
passes public redaction checks. It does not prove exchange account ownership,
wallet control, or profitability unless the operator publishes or privately
discloses matching raw Hyperliquid exports.

Generate or verify the demo pack:

```bash
PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py
PYTHONPATH="$PWD/engine/src" scripts/proof_pack.py --check
```

## Network Proof Pack

The public repository also commits a deterministic ZERO Network proof chain in
`docs/proof/network`. It is generated from a fixed-clock paper runtime and
emits:

- `zero.network.profile.v1` in `profile.json`;
- `zero.network.leaderboard.v1` in `leaderboard.json`;
- `zero.deployment_identity_evidence.v1` in `identity/identity_bundle.json`;
- `zero.network.profile_verification.v1` in `profile-verification.json`;
- `zero.network_proof_pack.v1` in `network-proof-pack.json`.

Verify the full public-safe chain:

```bash
PYTHONPATH="$PWD/engine/src" scripts/network_proof_pack.py
PYTHONPATH="$PWD/engine/src" scripts/network_proof_pack.py --check
```

This pack proves profile, leaderboard, deployment-claim, deployment-heartbeat,
and hosted-compatible ingestion bindings. The static fixture is unsigned so it
stays reproducible; signed deployment identity is covered by the paper and
Railway smoke tests.

Agents can inspect the same Network proof chain through MCP without gaining any
live execution capability:

```bash
PYTHONPATH="$PWD/engine/src" python3 -m zero_engine.mcp --smoke
PYTHONPATH="$PWD/engine/src" scripts/mcp_transcript.py --check
```

## Live Trading Evidence

`docs/proof/live/live-trading-evidence.json` is a redacted
`zero.live_trading_evidence.v1` packet generated from private operator
Hyperliquid evidence. It summarizes live fills, trades, and live decision
records with hashed symbols, bucketed quantities, bucketed timestamps, source
hashes, and no raw venue payloads.

Verify it with:

```bash
scripts/live_trading_evidence.py verify docs/proof/live/live-trading-evidence.json
```

The verifier rejects raw wallet-like material, private keys, order IDs, client
order IDs, trace IDs, idempotency keys, and embedded source payloads.

## Privacy Regression Fixtures

The negative fixtures in `docs/proof/privacy-regression` are synthetic payloads
that intentionally include private-looking fields. They keep the verifier honest:
wallet-like material and raw exchange order IDs must be refused before anything
is published as public proof.

Run them with:

```bash
PYTHONPATH="$PWD/engine/src" scripts/proof_privacy_regression.py
```

Future correlation packs must add signed live records, exchange-side evidence,
and paper/live correlation from the same strategy window. Do not publish an
R-squared value, latency claim, win rate, or PnL result unless the exact
supporting artifacts are committed or linked from the manifest.

## Required Live Correlation Inputs

A real paper-vs-live pack must include:

- paper decisions and fills exported from the same strategy window;
- exchange-side order and fill records;
- public-safe hashes of raw venue identifiers;
- a reproducible notebook or script that computes the metric;
- a manifest hash and signature.

Until those inputs exist, `live_correlation.status` must remain `unavailable`.
