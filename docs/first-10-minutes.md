# First 10 Minutes

This is the fastest path from a clean checkout to an inspected ZERO paper
runtime. It requires no exchange credentials, no cloud account, and no private
context.

## 0. Choose Install Path

Use the latest release binary if you want to inspect the operator terminal
without compiling Rust:

```bash
curl -fsSL https://raw.githubusercontent.com/zero-intel/zero/main/scripts/install.sh | bash
zero --version
```

Use source if you want to contribute:

```bash
git clone https://github.com/zero-intel/zero.git
cd zero
python3 -m venv .venv
source .venv/bin/activate
just bootstrap
```

## 1. Run The Paper Engine

```bash
just demo
```

You should see deterministic JSON with accepted and rejected paper decisions.
The important signal is not that a trade filled. The important signal is that
ZERO records both approvals and refusals through the same safety path.

## 2. Start The Local Paper API

In terminal 1:

```bash
just paper-api
```

Expected output:

```text
zero paper API listening on http://127.0.0.1:8765
```

Leave it running.

## 3. Inspect From The Operator Terminal

In terminal 2:

```bash
cd cli
cargo run -q -p zero -- --api http://127.0.0.1:8765 doctor
cargo run -q -p zero -- --api http://127.0.0.1:8765 run status
cargo run -q -p zero -- --api http://127.0.0.1:8765 run risk
```

Expected shape:

```text
[ ok ] engine_reachable  zero-paper-engine v0.1.1
[warn] auth              no token set - read-only endpoints only
engine: regime=PAPER MARKET. Local deterministic demo.
risk: OK
```

The auth warning is expected. The public paper API exposes read-only inspection
without a token and marks execution responses as simulated.

## 4. See The Public Product Boundary

With the paper API still running:

```bash
curl -fsS http://127.0.0.1:8765/network/profile | python3 -m json.tool
curl -fsS http://127.0.0.1:8765/intelligence/snapshot | python3 -m json.tool
curl -fsS http://127.0.0.1:8765/live/preflight | python3 -m json.tool
```

What this proves:

- ZERO Network packets are public-safe and redacted.
- ZERO Intelligence public snapshots are delayed and aggregate.
- Live mode refuses by default until local self-custodial preflight passes.

## 5. Run The Reproducible Demo Capture

```bash
scripts/demo_capture.sh
```

The capture starts the local paper API, probes it through the CLI and HTTP
contracts, executes a simulated paper order, and prints public-safe Network and
Intelligence payloads.

Use an installed release binary instead of compiling the CLI:

```bash
ZERO_BIN="$(command -v zero)" scripts/demo_capture.sh
```

Maintainers can rehearse the same public source boundary from a temporary clean
tree:

```bash
just fresh-clone-rehearsal
```

That command copies only publishable source files, verifies the public and
hardening gates, runs the paper example, and smokes the paper API through the
CLI outside the maintainer checkout.

## 6. Verify Public Proof And Agent Surfaces

```bash
just public-proof
```

This checks the deterministic demo proof pack, the ZERO Network proof chain,
the read-only MCP server, and the committed MCP transcript. It is the shortest
command that proves the public proof packets and agent-readable surface are in
sync without enabling live execution.

## 7. Run The Local Gate

Before opening a pull request:

```bash
just ci
```

`just ci` is the repo's local confidence gate: engine lint/tests, CLI
format/clippy/tests, docs gate, hardening gate, paper API smoke, examples, and
package dry run.

## What You Have Proven

After ten minutes, you have verified:

- The engine runs locally in paper mode.
- The CLI can inspect operator state.
- Safety gates accept and reject actions.
- Public proof and intelligence contracts do not leak raw traces.
- The demo proof pack, Network proof chain, and MCP transcript are current.
- Live execution is not enabled by accident.
- The local repo can pass the same core gate maintainers use before release.
