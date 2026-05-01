# zero — honesty discipline

The three layers that make the CLI refuse to lie.

`zero` renders live trading state. The operator must be able
to trust every character on the screen: if a number is there,
it came from somewhere; if a value is stale, the interface
says so; if a command would increase risk while the operator
is tilted, the interface makes them slow down. That trust is
not a design aesthetic — it is enforced by three layers of
mechanism that each refuse to compile or refuse to pass tests
when violated.

This doc is the map. For each layer: what it is, where it
lives, what it forbids, and how the invariant is enforced.

- [Layer 1: `Stat<T>` — every number carries its provenance](#layer-1-statt--every-number-carries-its-provenance)
- [Layer 2: Widgets — staleness is render-native](#layer-2-widgets--staleness-is-render-native)
- [Layer 3: Commands — risk direction is a type](#layer-3-commands--risk-direction-is-a-type)
- [How the three layers interlock](#how-the-three-layers-interlock)

---

## Layer 1: `Stat<T>` — every number carries its provenance

**Source:** `crates/zero-engine-client/src/stat.rs`
· **Rationale:** spec §3.1 · ADR-003
· **What it forbids:** bare numeric values in the type graph.

Every number the TUI renders passes through this type:

```rust
pub struct Stat<T> {
    pub value: T,
    pub as_of: DateTime<Utc>,
    pub n: Option<u64>,
    pub source: Source,
}

pub enum Source { Http, Ws, Mcp, Derived, Mock }
```

The four fields encode the four questions an operator asks
about any live number:

| Field | The question it answers |
|---|---|
| `value` | what |
| `as_of` | when was this produced |
| `n` | how many samples is it based on (or `None` for live readings) |
| `source` | where did it come from |

Because `Stat` is the only shape that crosses the
engine-client → TUI boundary, a widget cannot render a number
without also being handed the metadata required to render it
*honestly*. There is no fallback path; `value: f64` does not
flow through the types. A developer who wants to display a
number has to either:

1. Construct a `Stat<T>` — which forces them to think about
   `as_of` and `source` — or
2. Discover their value cannot flow to the render layer at
   all, because the function they need takes `&Stat<T>`.

### What `Stat` buys beyond documentation

- **`stat.age(now)`** and **`stat.is_stale(now, threshold)`**
  ship as methods on `Stat` itself. Widgets never do their
  own "is this old?" arithmetic against bare `DateTime`s; the
  comparison goes through the type. See Layer 2 for the
  widget-level rules that use them.
- **`source`** survives into the status bar's engine segment
  (via the `EngineState` poller's source tagging) and lets
  the operator tell "haven't heard from the WS in 20 s" from
  "haven't done a REST poll in 30 s." Those are different
  problems with different fixes; the type makes them
  distinguishable.
- **`n`** is the difference between "your win rate is 80 %"
  (over three trades) and "your win rate is 80 %" (over four
  hundred). The verdict widget renders `n=3` in muted text
  next to the summary so the operator sees the confidence
  interval before they see the headline.

### Escape hatches and where they end

`Source::Mock` exists for fixtures in tests and local
development; a `Stat<T>` constructed with `Mock` is still a
`Stat`, so the render path cannot distinguish it from a real
reading. The lint layer (see Layer 3) forbids `Mock` from
appearing in non-test code paths via a grep gate in
`.lints/check-anti-patterns.sh`. Breaking the rule requires
both a source edit and a lint-rule edit — two places, on
purpose.

---

## Layer 2: Widgets — staleness is render-native

**Source:** `crates/zero-tui/src/widgets/statusbar.rs`
· **Rationale:** spec §3.1 (honesty is render-native)
· **What it forbids:** stale values that render like fresh ones.

`Stat<T>` carries the metadata; widgets are responsible for
turning that metadata into ink on screen. The status bar is
the load-bearing surface — it's always visible, so if it's
honest, the rest of the interface has a fighting chance of
being honest too. The three rules it enforces:

### 2.1 Operator-state is always visible

The spec mandates it (§3.1), and the widget code enforces it:
the `ops:` segment cannot be hidden by tier fallback. The
`Full`, `Compact`, and `Minimal` tiers all render it.
Verified by the snapshot matrix in
`crates/zero-tui/tests/statusbar_fault_matrix.rs`, which
takes a 4-widths × 4-states grid and asserts the `ops`
segment appears in every cell. A regression that drops the
segment in any cell fails CI.

### 2.2 Stale operator-state gets a muted asterisk

```rust
const OPERATOR_STATE_STALE_AFTER: Duration = Duration::seconds(30);
// …
let stale = stat.is_stale(self.now, OPERATOR_STATE_STALE_AFTER);
let label_color = if stale { metadata } else { color };
let marker_span = if stale { Span::styled("*", …) } else { … };
```

If the classifier hasn't reported in 30 s, the label renders
in muted grey with a trailing `*`. This is the literal
mechanism by which "this number is old" becomes visible
without the operator having to think about it. The 30 s
threshold is chosen so that a healthy engine (classifier
polls every 1–5 s) never trips it, but a WS drop or poller
stall does.

### 2.3 Feed age renders as seconds with colour bands

- `caution` (amber) when age > 3 s
- `alert` (red) when age > 10 s

Same pattern: the widget compares `stat.as_of` to `now`
through `Stat::age`, and colour is a pure function of the
age bucket. There is no path where a 30-second-old feed
renders as `feed:--` or `feed:OK`; the age surface is
mandatory.

### 2.4 What regression guards this layer

The `statusbar_fault_matrix` suite (16 snapshots across
40/80/120/200 cols × healthy/reconnecting/down/stale) is
the render-level enforcement. The `reconnecting` cell is a
literal reproduction of the 2026-04-21 incident where
tracing output from the WS poller bled into the status bar
during a 403 auth-reject loop; that cell carries negative
assertions that forbid the strings `HTTP error` and `403`
from ever appearing in the rendered bar. A regression
that writes diagnostics to the TUI's terminal fails here.

---

## Layer 3: Commands — risk direction is a type

**Source:** `crates/zero-commands/src/risk.rs`,
`crates/zero-commands/src/friction.rs`
· **Rationale:** spec §6 (friction ladder)
· **What it forbids:** gating a risk-reducing command.

The friction ladder slows the operator down when they're
about to increase risk in a bad state. A tilted operator who
types `/execute` at 2 AM should see a 10-second typed-confirm
overlay; a tilted operator who types `/kill` should see
nothing but immediate execution. Gating the second one would
*kill the operator's ability to protect themselves* — which
is the single worst thing the CLI could do.

The spec states this as a principle. The code states it as a
type:

```rust
pub enum RiskDirection { Increases, Reduces, Neutral }

pub trait Gateable: sealed::Sealed + Copy + 'static {
    const DIRECTION: RiskDirection;
}

pub struct Increases;
impl sealed::Sealed for Increases {}
impl Gateable for Increases { … }

// Note: no `impl Sealed for Reduces` anywhere.

pub struct FrictionGate<D: Gateable> { … }
```

`Gateable` is a sealed trait — only types in this module can
implement it. The module implements it for `Increases` and
nothing else. Therefore `FrictionGate<Reduces>` is
uninhabited; you cannot construct it.

This is enforced by a `compile_fail` doctest on the public
type:

```rust
/// ```compile_fail
/// use zero_commands::risk::FrictionGate;
/// // Simulate a consumer trying to build a gate for a
/// // risk-reducing command.
/// struct Reduces;
/// let _: FrictionGate<Reduces> = FrictionGate::new();
/// ```
pub struct FrictionGate<D: Gateable> { … }
```

Plus a positive doctest that proves the `Increases` path
still compiles. Both run under `cargo test --doc`, so the
invariant is enforced by the compiler *and* asserted by the
test harness — the only two places an invariant can survive.

### The runtime complement

At the decision layer, `friction::decide(direction, label)`
has this as its first arm:

```rust
RiskDirection::Reduces | RiskDirection::Neutral => FrictionDecision::Proceed,
```

Every state label (`Fresh`, `Steady`, `Elevated`, `Tilt`,
`Fatigued`, `Recovery`) routes through the same arm for
`Reduces`. No path produces a `Pause` or `TypedConfirm` for
a risk-reducing command.

This is pinned by the "2 AM test suite" in
`crates/zero-commands/tests/two_am_scenarios.rs`: six
`kill_at_<label>_proceeds` tests assert
`decide(Reduces, each_label) == Proceed`, against an
exhaustive `match` on `Label` that forces the suite to grow
by one test per label variant added. Adding a new label
without adding its "reduces-never-gates" test fails to
compile; adding it and getting the decision wrong fails the
test.

### The grep gate

The last line of defence is `.lints/check-anti-patterns.sh`,
a grep script that blocks string-level regressions the type
system can't catch: celebration copy in risk-reducing
command output, marketing copy in chrome, forbidden model
strings, and so on. It runs on CI for every PR touching
`cli/**` and is invoked manually via
`./.lints/check-anti-patterns.sh`.

---

## How the three layers interlock

The layers do not duplicate each other; each catches a
different failure mode.

| Failure mode | Caught by |
|---|---|
| Widget receives a number without metadata | Layer 1 (`Stat<T>` — the function won't type-check) |
| Widget has metadata but renders it as if fresh | Layer 2 (snapshot tests fail) |
| Command tree treats a `Reduces` command as gateable | Layer 3a (doctest refuses to compile) |
| Decision function routes `Reduces` to `Pause` | Layer 3b (2 AM suite fails) |
| Operator-visible copy celebrates a kill | Layer 3c (grep gate fails) |

A new engineer making any one of these changes hits a red
test or a compile error on their first `cargo test`. That is
the whole design: the honesty discipline is load-bearing
because it is compile-time or CI-visible on every change.

### What explicitly isn't enforced by these layers

- **Engine correctness.** `Stat` carries source and age,
  but it cannot detect that the engine itself is reporting
  a wrong number. The CLI trusts the engine; the engine is
  the source of truth.
- **Operator intent.** The friction ladder slows someone
  who is tilted; it does not judge whether the trade they
  want to make is smart. That's the verdict widget's job,
  and it is explicitly a *suggestion* surface, not a block.
- **Panic hooks and unwinding.** A caught panic becomes
  `ExitKind::Internal` (code 4) — the CLI never claims "all
  is well" while internally broken.

For everything else: if you find a path that lets an
untrustworthy number reach the screen, file it as a bug
against this doc and one of the three layers.
