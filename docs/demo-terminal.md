# Demo Terminal

This page is the maintained demo path for screenshots, asciinema captures, and
release smoke checks. It is intentionally local-first and paper-only.

## Scripted Capture

Run from the repository root:

```bash
scripts/demo_capture.sh
```

The script:

- starts the local paper API on `127.0.0.1:8765`;
- probes the runtime version;
- runs `zero doctor`, `zero run status`, `zero run risk`, and
  `zero run positions`;
- submits a simulated paper order;
- prints a redacted ZERO Network profile packet;
- prints a delayed ZERO Intelligence snapshot;
- prints live preflight refusal state;
- prints live cockpit, receipt, canary-policy, and runtime-parity readouts.

It uses `cargo run` by default so contributors can run it from source. To use a
release binary:

```bash
ZERO_BIN="$(command -v zero)" scripts/demo_capture.sh
```

Use another port when `8765` is busy:

```bash
ZERO_DEMO_PORT=8876 scripts/demo_capture.sh
```

## Expected Story

The demo should show five facts in this order:

1. ZERO is running a local paper engine.
2. The operator terminal can inspect runtime health.
3. Status and risk are explicit.
4. Execution is simulated unless self-custodial live mode is deliberately
   configured.
5. The live cockpit refuses risk by default and names the next operator action.
6. Public proof and intelligence packets are aggregate and redacted.

## Release Demo Checklist

Before using the demo in a public release or launch post:

- [ ] `scripts/demo_capture.sh` passes from a clean checkout.
- [ ] `ZERO_BIN="$(command -v zero)" scripts/demo_capture.sh` passes after
  installing the latest release.
- [ ] No local paths, secrets, raw trace IDs, or idempotency keys appear in the
  public proof or intelligence payloads.
- [ ] The transcript still says paper mode is the default.
- [ ] The live preflight path still refuses by default.
- [ ] The live cockpit path still reports `live_mode=refused` by default.

## Copy For Maintainers

Short caption:

```text
ZERO running locally in paper mode: inspectable runtime state, explicit risk,
simulated execution, a live cockpit that refuses risk by default, and
public-safe proof packets.
```

Long caption:

```text
This is ZERO's first-run path. It starts a local paper runtime, inspects it from
the Rust operator terminal, submits a simulated order through the safety path,
prints the live cockpit/readiness boundary, and emits redacted Network and
Intelligence payloads. No exchange credentials or hosted control plane are
required.
```
