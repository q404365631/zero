# CLI Quickstart Capture

This capture shows the first paper-only operator flow a contributor should see
after cloning the repository. It uses the local paper API and requires no
exchange credentials.

## Terminal 1: Paper API

```bash
PYTHONPATH=engine/src python3 -m zero_engine.api
```

Expected output:

```text
zero paper API listening on http://127.0.0.1:8765
```

Leave this terminal running while using the CLI from a second terminal.

## Terminal 2: Operator CLI

Run doctor against the local paper API:

```bash
cd cli
cargo run -q -p zero -- --api http://127.0.0.1:8765 doctor
```

Abbreviated output:

```text
  [   ok] runtime                zero-doctor v0.1.1 · <os>/<arch> · debug
  [   ok] config_dir             <local config dir>
  [   ok] config_parse           <local config dir>/config.toml parses
  [   ok] engine_reachable       zero-paper-engine v0.1.1 (http://127.0.0.1:8765/)
  [   ok] engine_healthy         ok — 2 healthy / 0 stale / 0 dead
  [   ok] engine_components      all fresh
  [ warn] auth                   no token set — read-only endpoints only
  [   ok] rate_budget            58/60 tokens · refill 1.00/s
  [   ok] ws_reachable           ws://127.0.0.1:8765/ws

  overall: warn
```

The auth warning is expected. The public paper API exposes read-only inspection
without a token and marks execution responses as simulated.

Inspect the engine status:

```bash
cargo run -q -p zero -- --api http://127.0.0.1:8765 run status
```

Expected output:

```text
  engine: regime=PAPER MARKET. Local deterministic demo.  confidence=90 (paper)  equity=$10000.00  open=0  upnl=+0.00
    today: trades=0  wins=0  pnl=+0.00  streak=+0  sizing=1.00x
    market: fear_greed=50  health=100%  coins_tradeable=3
```

Inspect the risk line:

```bash
cargo run -q -p zero -- --api http://127.0.0.1:8765 run risk
```

Expected output:

```text
  risk: OK  equity=$10000.00  peak=$10000.00  dd=0.00%  daily-pnl=+0.00  daily-loss=0.00%  open=0
```

## Smoke Equivalent

The automated equivalent is:

```bash
just paper-api-smoke
```

Use the manual capture above when changing CLI rendering or paper API response
shape. Use the smoke command for routine local verification.
