# zero — operator terminal

A CLI for operating ZERO self-custodial onchain operations. The engine is the
source of truth; this binary is a renderer and a dispatcher,
with a friction ladder that refuses to let a tilted operator
rush a risk-increasing command.

Intelligence, not automation.

---

## For operators

### Install

**Latest release binary:**

```bash
curl -fsSL https://raw.githubusercontent.com/zero-intel/zero/main/scripts/install.sh | bash
```

The installer downloads the latest GitHub Release asset for your OS, verifies
`SHA256SUMS`, verifies the GitHub artifact attestation with `gh`, and installs
`zero` to `~/.local/bin` by default.

**From source:**

```bash
git clone https://github.com/zero-intel/zero.git
cd zero/cli
cargo install --path crates/zero --profile release-small
```

This builds the size-optimised binary (~4.2 MB on
darwin-arm64) and puts it on your `PATH` at
`~/.cargo/bin/zero`. Homebrew and package-registry installs
are not published yet.

Rust toolchain: `rustc` 1.88+ (pinned in `rust-toolchain.toml`).

### First run

```bash
zero init         # one-time setup wizard — picks your handle, engine URL, defaults
zero doctor       # smoke-test: config, engine reachability, session DB
zero              # launch the TUI
```

`zero init` is interactive by default. Supply `--yes` with
`--non-interactive` for CI pipelines that want to write a
config from flags; `--dry-run` rehearses the plan without
writing. Full flag list: [`docs/commands.md`](docs/commands.md#zero-init).

`zero doctor` runs before the first launch so that a bad
config fails fast on a clear error instead of silently on a
confusing one. It exits non-zero on any failed check and
emits JSON with `--format json` for script consumption.

On the first real launch, the TUI shows a one-time welcome
block:

```
── welcome ──────────────────────────────────────
zero is your operator terminal.
intelligence, not automation.

the engine is the source of truth; this CLI is a renderer + dispatcher.
the operator-state segment on the status bar is always visible — never hidden.
risk-reducing commands (/kill, /flatten-all, /close, /pause-entries, /break) are friction-exempt. always.

two commands to get started:
  /help  — the full surface, grouped by risk direction.
  /status — what the engine sees right now.

this welcome shows once. re-read it any time with /help.
─────────────────────────────────────────────────
```

This is exactly what the operator sees — not marketing copy,
the literal lines. The canonical source is `WELCOME_LINES`
in `crates/zero/src/main.rs`; any edit there is reflected
here on the next doc pass.

### The three surfaces you'll use every day

- **`zero`** — the bare invocation launches the TUI. This is
  where real operator work happens: the conversation pane,
  the modes (conversation / positions / heat), the status bar
  with always-visible operator-state.
- **`zero doctor`** — your sanity check. Run it when
  something feels off: a slash-command fails, the status bar
  says `ops:?`, the engine looks unreachable. Output is
  terse and actionable; pair with `--format json | jq` for
  scripting.
- **`zero run <slash-cmd>`** — runs a single slash-command
  non-interactively and exits. `zero run status`,
  `zero run risk`, `zero run pulse 20`. Risk-increasing
  commands (`/execute` and friends) are refused here because
  they require a typed-confirm overlay that doesn't exist
  without a TTY — running them through a pipe would bypass
  the friction ladder on purpose, which the CLI will not do.
- **Interactive `/execute <coin> <buy|sell> <size>`** — posts
  the exact order request to the engine after operator-state
  friction clears. The engine decides paper versus live from
  its launch mode or `X-Zero-Mode`; the CLI renders `(paper)`
  or `(live)` from the engine response, not from local guessing.
  When the live executor returns a canary receipt hash, the CLI
  also renders `receipt=sha256:...` for audit follow-up.

Full reference: [`docs/commands.md`](docs/commands.md)
(auto-generated from `--help` — stale docs fail CI).

### Exit codes for scripts

`0` success · `1` usage error · `2` engine unreachable ·
`3` auth invalid (reserved) · `4` internal error.
Full table: [`docs/commands.md`](docs/commands.md#exit-codes).

### Where the CLI stores things

| Path | What |
|---|---|
| `<zero_dir>/config.toml` | active operator config (handle, defaults, guardrails, custody metadata) |
| `<zero_dir>/operators/<operator-slug>/state/state.db` | session log — conversations, slash-command history, journey milestones |
| `<zero_dir>/operators/<operator-slug>/zero.log` | TUI tracing output (WS/poller WARN records live here, not on the status bar) |
| `<zero_dir>/operators/<operator-slug>/sock` | local headless supervisor socket |
| OS keychain account `operator:<operator-slug>` | engine bearer token (`dev.getzero.zero`) and Hyperliquid API key (`dev.getzero.hyperliquid`) |

The `--no-persist` global flag disables session persistence
for a single invocation — nothing goes to `state.db`, useful
for one-off scripted runs. `--api` and `--token` override the
config per invocation; set the corresponding env vars
(`ZERO_API_URL`, `ZERO_API_TOKEN`) for shell use.

See [Operator Isolation](../docs/operator-isolation.md) for the local
filesystem and keychain partition model.

Live Hyperliquid custody metadata can live in config, but the private key must
stay in the OS keychain or local process environment. `zero doctor` reads the
engine `/live/preflight` gate and warns until the runtime proves custody,
journal, account reconciliation, risk, and emergency controls are ready.
Operators can inspect the same read-only account truth with `/hl-account`, the
live risk gate with `/hl-reconcile`, and the current breaker layer with
`/immune`. `/live-cockpit` combines preflight, reconciliation, immune breakers,
certification, heartbeat, recent live records, and the next required action in
one read-only operator view. The CLI attaches the configured `identity.handle`
as `X-Zero-Operator-*` audit context, and `/live-cockpit` renders the resolved
operator identity so team and agentic runs are attributable. `/live-certify`
runs the dry-run fake-exchange certification harness and prints the drill pass
count before any real canary is considered. `/runtime-parity` renders the
production-parity OODA report and disabled live-shadow refusal boundary from
`/runtime/parity`. `/live-canary` renders the canary policy lifecycle from
`/live/canary-policy`: readiness, arm/disarm state, qualification,
publishability, exchange-evidence state, next action, and phase details.
`/live-evidence` renders the hash-only canary evidence bundle, including the
evidence hash, signature status, execution-receipt hash artifact, and artifact
hashes without exposing raw decisions or secrets.

Risk-reducing live controls are wired to the engine when an API client is
attached:

- `/kill` posts `POST /live/kill` and also tears down the local headless
  supervisor when present.
- `/flatten-all` posts `POST /live/flatten` for reduce-only close orders.
- `/pause-entries` posts `POST /live/pause` to stop new risk-increasing entries.
- `/resume-entries` posts `POST /live/resume` and is friction-gated because it
  can reopen risk-increasing entries.

If the connected engine is paper-only, these commands surface the engine's
`live executor not configured` refusal instead of pretending live risk changed.

---

## For contributors

### Build & test

```bash
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt    --all --check
```

All four must pass. Additional perf and doc gates
(all `#[ignore]`-by-default or env-gated):

```bash
# Perf tripwires — release-mode regression guards.
cargo test -p zero-operator-state --release classifier_tick_under_budget -- --include-ignored
cargo test -p zero --test version_startup --release -- --include-ignored

# Full criterion distribution.
cargo bench -p zero-operator-state

# Idle TUI RSS check (requires a controlling TTY).
./scripts/idle_rss_check.sh --profile release-small

# Command-reference regeneration (CI lane enforces freshness).
ZERO_REGENERATE_DOCS=1 cargo test -p zero --test commands_doc
```

### Honesty discipline

Three layers catch three different failure modes:

1. `Stat<T>` — every number carries `as_of` + `source`; no
   bare numerics cross the engine/TUI boundary.
2. Widget rules — staleness is visible (`ops:*`, `feed:<age>`
   with colour bands). The status-bar fault matrix
   (`crates/zero-tui/tests/statusbar_fault_matrix.rs`)
   pins the render contract in 16 snapshots.
3. Friction gates — risk direction is a sealed type.
   `FrictionGate<Reduces>` does not compile. Risk-reducing
   commands (`/kill`, `/flatten-all`, `/close`,
   `/pause-entries`, `/break`) are structurally
   friction-exempt.

Full explanation and cross-references:
[`docs/honesty.md`](docs/honesty.md).

### Crate layout

| Crate | Purpose |
|---|---|
| `zero` | binary entrypoint; argv parsing; dispatch; launches the TUI |
| `zero-tui` | ratatui app — shell, status bar, prompt, conversation, modes, widgets |
| `zero-engine-client` | unified HTTP + WS + MCP client mirroring engine state |
| `zero-session` | SQLite-backed session persistence (replay, resume, fork, daily wrap) |
| `zero-config` | config, operator-local paths, and OS keychain secret slots |
| `zero-commands` | slash-command framework + built-ins, with the RiskDirection invariant |
| `zero-operator-state` | pure state-vector + label + friction classifier (runs on engine & CLI) |
| `zero-onboarding` | first-run wizard per spec §11 |
| `zero-doctor` | self-diagnostic per spec §18 |
| `zero-testkit` | mock engine + fixtures + perf harness (dev-only) |

### Planning and spec

| File | What it is |
|---|---|
| [`M1_PLAN.md`](M1_PLAN.md) | Definition of Done for milestone 1 — what ships and what doesn't |
| [`M0_ADR.md`](M0_ADR.md) | Architecture Decision Records (ADR-001 … ADR-020) |
| [`ADDENDUM_A_RESPONSE.md`](ADDENDUM_A_RESPONSE.md) | How Addendum A (operator-state honesty) was absorbed into M1 |
| [`SPEC_v2.1_PATCHES.md`](SPEC_v2.1_PATCHES.md) | Red-team resolutions: security, cloud coupling, Heat-mode, `zero web` |
| [`SECURITY_THREAT_MODEL.md`](SECURITY_THREAT_MODEL.md) | Threat actors, trust boundaries, per-asset mitigations, recovery |
| [`CLOUD_COUPLING.md`](CLOUD_COUPLING.md) | Every cloud surface + offline-tolerance matrix |
| [`.lints/README.md`](.lints/README.md) | Custom anti-pattern lint gates |

### Status

M1 in progress. TUI shell, four-mode layout, command
dispatcher with the risk-asymmetry invariant, SQLite session
persistence with replay, operator-state classifier, doctor,
onboarding, daily-wrap, and engine-backed `/execute`. Remaining:
auto-overlay of `/risk` at friction level 3+ and the final CI
glue to bind every gate together.
