# Decision Stack

ZERO's public decision stack makes the private engine's lens/layer/modifier
shape explicit without exposing private journals, credentials, wallet material,
or live exchange order data.

The stack is available in paper mode:

```bash
curl -fsS 'http://127.0.0.1:8765/decision/stack?coin=BTC'
zero-decision-stack --coin BTC --price 40500 --sample-size 4
```

Schema: `zero.decision.stack.v1`.

## Contract

The response has three decision surfaces:

- `lenses`: weighted signal views such as price action, risk capacity, memory
  context, and operator liveness.
- `layers`: ordered gates that explain which checks passed, which checks block
  entry, and which checks are non-blocking calibration warnings.
- `modifiers`: bounded adjustments that keep ZERO rejection-first and
  paper-first, including operator friction for risk-increasing live actions.

`/evaluate/{coin}` keeps the existing CLI-compatible fields and now embeds the
same stack as `decision_stack`, plus top-level `lenses` and `modifiers`.

## Boundaries

The public stack is an explanation and evaluation contract. It does not place
orders, approve live execution, bypass local preflight, or emit private runtime
state. Live execution remains local opt-in and still requires the live
readiness, immune, reconciliation, receipt, and operator-friction gates.

The public stack intentionally reports `allowed_to_execute_live: false`. That is
not a product limitation; it is the public API boundary. A local operator can
use the stack to understand why ZERO would or would not act, but live authority
stays with the self-custodial runtime.

## Agent Use

Coding agents should treat `zero.decision.stack.v1` as the first public
decision primitive:

- extend a lens only when it has deterministic fixture evidence;
- add a layer when it can block or explain a decision independently;
- add a modifier when it changes confidence or friction without pretending to be
  a gate;
- preserve paper-first defaults and never add live execution authority to MCP.

MCP exposes the same contract as `zero_get_decision_stack` and
`zero://decision/stack`.
