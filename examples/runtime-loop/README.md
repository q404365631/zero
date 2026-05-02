# Runtime Loop Example

This example runs one bounded paper OODA cycle from the public scenario fixture.

```bash
PYTHONPATH="$PWD/engine/src" python3 examples/runtime-loop/run.py
```

It writes temporary decision and cycle journals, then prints the latest
`zero.runtime.cycle.v1` record. The example is deterministic and never places
live orders.
