# CLI Doctor Troubleshooting

`zero doctor` is the first diagnostic to run when the operator terminal cannot
inspect the engine. It checks local config, engine reachability, token presence,
rate-budget state, live preflight, and WebSocket reachability when configured.

The examples below are safe for public paper mode. They do not require exchange
credentials and do not imply live trading is ready.

## Missing API Token

Run doctor against the local paper API without a token:

```bash
unset ZERO_API_TOKEN
cd cli
cargo run -q -p zero -- --api http://127.0.0.1:8765 doctor
```

Expected safe output snippet:

```text
[ warn] auth                   no token set - read-only endpoints only
overall: warn
```

This is expected for local paper inspection. Read-only status, risk, proof, and
public contract endpoints can still be inspected. Do not paste a live token into
docs, shell history, issue comments, screenshots, or public proof packs.

If you intended to use an authenticated engine, pass a token explicitly or set
the environment variable for the current shell:

```bash
ZERO_API_TOKEN="<operator-token>" \
  cargo run -q -p zero -- --api http://127.0.0.1:8765 doctor
```

Expected output when a token is present and accepted:

```text
[   ok] auth                   token present
[   ok] auth_verified          token accepted
```

If `auth_verified` fails, rotate or rewrite local config with:

```bash
cargo run -q -p zero -- init --force
```

## Paper API Not Running

If terminal 1 is not running the paper API, doctor cannot reach the engine:

```bash
cd cli
cargo run -q -p zero -- --api http://127.0.0.1:8765 doctor
```

Expected safe output snippet:

```text
[ fail] engine_reachable       unreachable: ...
overall: fail
```

Start the paper API in another terminal:

```bash
cd ..
just paper-api
```

Expected API output:

```text
zero paper API listening on http://127.0.0.1:8765
```

Then rerun doctor:

```bash
cd cli
cargo run -q -p zero -- --api http://127.0.0.1:8765 doctor
```

Expected reachable-engine snippet:

```text
[   ok] engine_reachable       zero-paper-engine v0.1.1 (http://127.0.0.1:8765/)
[   ok] engine_healthy         ok
```

## Live Preflight Refusing Closed

The public paper runtime exposes live-readiness contracts, but live risk should
refuse closed until local custody, journal, reconciliation, emergency controls,
and executor setup are coherent.

Run doctor:

```bash
cd cli
cargo run -q -p zero -- --api http://127.0.0.1:8765 doctor
```

Expected safe output snippet:

```text
[ warn] live_preflight         not ready: live_executor, wallet, key, journal
overall: warn
```

Inspect the detailed preflight packet:

```bash
curl -fsS http://127.0.0.1:8765/live/preflight | python3 -m json.tool
```

Expected boundary:

```json
{
  "schema_version": "zero.live_preflight.v1",
  "live_mode": "refused",
  "ready": false
}
```

This is correct for public paper mode. It proves live-capable surfaces fail
closed; it does not prove accepted live execution. Do not treat this warning as
a bug unless a locally owned live canary was explicitly configured and expected
to pass.

For the cockpit-level view:

```bash
cd cli
cargo run -q -p zero -- --api http://127.0.0.1:8765 run live-cockpit
```

Expected safe output snippet:

```text
live-cockpit: live_mode=refused  ready=false  risk_allowed=false
```

## Useful Follow-Up Commands

```bash
cargo run -q -p zero -- --api http://127.0.0.1:8765 run status
cargo run -q -p zero -- --api http://127.0.0.1:8765 run risk
cargo run -q -p zero -- --api http://127.0.0.1:8765 run live-cockpit
just paper-api-smoke
```

Use `just paper-api-smoke` when changing CLI rendering, API response shape, or
doctor examples. It starts the local paper API, exercises the CLI path, and
asserts that live mode still refuses by default.
