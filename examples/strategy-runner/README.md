# Strategy Runner Example

This example shows ZERO's declarative strategy runner contract.

Run it from the repository root:

```bash
PYTHONPATH="$PWD/engine/src" python3 examples/strategy-runner/run.py
```

Or use:

```bash
just strategy-runner-example
```

The runner file proposes a paper `OrderIntent`; the paper engine still applies
risk limits, records the decision, and owns fills or rejections.

Contributor rules:

- keep declarative runners paper-only;
- use deterministic fixtures;
- do not add exchange credentials or live API calls;
- add conformance tests for new runner behavior.
