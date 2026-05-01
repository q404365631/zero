# API Compatibility

ZERO treats the local paper API as a public integration surface. The OpenAPI
contract in [openapi/zero-paper-api.v1.yaml](../openapi/zero-paper-api.v1.yaml)
is the machine-readable source of truth for the public runtime.

## Version Scope

The current public contract is `zero-paper-api.v1`.

It covers:

- local paper runtime health, status, positions, risk, journal, metrics, and
  audit export;
- paper execution through `POST /execute`;
- read-only market quote and Hyperliquid status surfaces;
- public-safe ZERO Network packets;
- delayed ZERO Intelligence packets and catalog;
- local live preflight and live risk-reducer controls.

It does not guarantee hosted ZERO Intelligence API behavior. Hosted commercial
surfaces will use their own versioned `/v1/intelligence/*` contract before
public launch.

## Compatibility Rules

Patch releases may:

- add optional response fields;
- add optional request fields;
- add new endpoints;
- add new enum values only when older clients can safely ignore them;
- tighten documentation and examples without changing wire behavior.

Patch releases must not:

- remove documented fields from stable responses;
- rename stable fields;
- change field types;
- make an optional request field required;
- change default paper behavior to live behavior;
- expose secrets, wallet addresses, raw trace records, idempotency keys, private
  notes, exchange order IDs, or per-trade symbols in public Network or
  Intelligence packets.

Breaking changes require a new versioned contract file and a migration note.

## Public Packet Boundary

ZERO Network and delayed ZERO Intelligence packets are public-safe aggregate
surfaces. They may include:

- aggregate decision, fill, rejection, and notional counts;
- verification status and proof hashes;
- publication and privacy metadata;
- delayed aggregate intelligence signals.

They must exclude:

- raw decisions;
- raw trace IDs;
- idempotency keys;
- wallet addresses;
- exchange order IDs;
- exchange credentials;
- private operator notes;
- strategy source labels;
- per-trade symbols.

## Live Mode Boundary

The public runtime is paper-first. Live execution is local opt-in and requires
operator-owned configuration.

Compatibility guarantees for live-facing endpoints are conservative:

- `GET /live/preflight` must stay non-secret and safe to expose in diagnostics;
- `POST /live/pause`, `/live/kill`, and `/live/flatten` must remain
  risk-reducing controls;
- when live execution is not configured, `POST /live/*` endpoints must refuse
  with an explicit reason instead of pretending to be active;
- `POST /execute` must stay paper by default unless `X-Zero-Mode: live` is
  explicitly supplied.

## Contract Gate

`scripts/openapi_contract_check.py` enforces the contract gate in local and CI
checks. It verifies:

- required public paths are present in the OpenAPI file;
- required response/request schema names are present and referenced;
- fixture JSON files are valid and retain required top-level fields;
- operation IDs remain unique.

Run it directly:

```bash
python3 scripts/openapi_contract_check.py
```

It is also included in `just docs-check` and therefore in `just ci`.
