# zero — command reference

**This file is generated.** Do not edit by hand; changes will be overwritten on the next doc sync.

To regenerate after editing flag docs or adding a subcommand in `crates/zero/src/main.rs`:

```bash
ZERO_REGENERATE_DOCS=1 cargo test -p zero --test commands_doc
```

The `commands_doc_is_fresh` test runs in the default `cargo test` lane and will fail CI if this file is stale relative to the compiled binary's `--help`.

## Contents

- [Top-level](#top-level)
- [`zero init`](#zero-init)
- [`zero doctor`](#zero-doctor)
- [`zero version`](#zero-version)
- [`zero run`](#zero-run)

## Top-level

```
$ zero --help
The CLI-native surface for ZERO self-custodial onchain operations. Plan → Auto → Headless, with
Shadow → Paper → Live for every composition change. Engine is source of truth; CLI is a renderer +
dispatcher.

Usage: zero [OPTIONS] [COMMAND]

Commands:
  init     First-run setup wizard
  doctor   Local diagnostic. Exits non-zero on failed checks
  version  Print CLI + engine version info
  run      Run a single slash-command non-interactively and exit
  help     Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Engine API endpoint (default: <https://api.getzero.dev>)
          
          [env: ZERO_API_URL=]

      --token <TOKEN>
          Operator token (or set `ZERO_API_TOKEN`)
          
          [env: ZERO_API_TOKEN=]

      --paper
          Start in paper mode

      --no-persist
          Disable session persistence for this invocation. The TUI runs from a fresh in-memory log
          and nothing is written to `~/.zero/state.db`

      --json
          Emit machine-readable output (where supported). Implies no color and no widgets

  -v, --verbose...
          Increase log verbosity (`-v` info, `-vv` debug, `-vvv` trace)

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## `zero init`

```
$ zero init --help
First-run setup wizard.

Interactive by default. Supply flags and `--yes` for non-interactive use (CI, scripts).

Usage: zero init [OPTIONS]

Options:
      --handle <HANDLE>
          Operator handle (any non-whitespace string). In non-interactive mode, required unless
          `--yes`

      --yes
          Accept defaults and skip confirmation prompts. Required for non-interactive runs without a
          handle

      --non-interactive
          Run non-interactively — never prompt, even for missing pieces. Useful in CI

      --dry-run
          Produce the plan but do not write it. Pairs well with `--non-interactive` for rehearsing a
          config before committing

      --api <API>
          Engine API endpoint (default: <https://api.getzero.dev>)
          
          [env: ZERO_API_URL=]

      --force
          Overwrite existing config without prompting

      --token <TOKEN>
          Operator token (or set `ZERO_API_TOKEN`)
          
          [env: ZERO_API_TOKEN=]

      --paper
          Start in paper mode

      --no-persist
          Disable session persistence for this invocation. The TUI runs from a fresh in-memory log
          and nothing is written to `~/.zero/state.db`

      --json
          Emit machine-readable output (where supported). Implies no color and no widgets

  -v, --verbose...
          Increase log verbosity (`-v` info, `-vv` debug, `-vvv` trace)

  -h, --help
          Print help (see a summary with '-h')
```

## `zero doctor`

```
$ zero doctor --help
Local diagnostic. Exits non-zero on failed checks

Usage: zero doctor [OPTIONS]

Options:
      --fix              Attempt auto-repair of repairable failures. **M1 stub.**
      --format <FORMAT>  Override output format [possible values: text, json]
      --api <API>        Engine API endpoint (default: <https://api.getzero.dev>) [env:
                         ZERO_API_URL=]
      --token <TOKEN>    Operator token (or set `ZERO_API_TOKEN`) [env: ZERO_API_TOKEN=]
      --paper            Start in paper mode
      --no-persist       Disable session persistence for this invocation. The TUI runs from a fresh
                         in-memory log and nothing is written to `~/.zero/state.db`
      --json             Emit machine-readable output (where supported). Implies no color and no
                         widgets
  -v, --verbose...       Increase log verbosity (`-v` info, `-vv` debug, `-vvv` trace)
  -h, --help             Print help
```

## `zero version`

```
$ zero version --help
Print CLI + engine version info

Usage: zero version [OPTIONS]

Options:
      --api <API>      Engine API endpoint (default: <https://api.getzero.dev>) [env: ZERO_API_URL=]
      --token <TOKEN>  Operator token (or set `ZERO_API_TOKEN`) [env: ZERO_API_TOKEN=]
      --paper          Start in paper mode
      --no-persist     Disable session persistence for this invocation. The TUI runs from a fresh
                       in-memory log and nothing is written to `~/.zero/state.db`
      --json           Emit machine-readable output (where supported). Implies no color and no
                       widgets
  -v, --verbose...     Increase log verbosity (`-v` info, `-vv` debug, `-vvv` trace)
  -h, --help           Print help
```

## `zero run`

```
$ zero run --help
Run a single slash-command non-interactively and exit.

Example: `zero run status`, `zero run risk`, `zero run regime BTC`, `zero run pulse 20`.

The exact input (minus the leading slash) is forwarded to the same dispatcher the TUI uses, so every
`Neutral` / `Reduces` command that makes sense without a conversation pane is available.
Risk-*Increasing* commands (`/execute`, `/state-override`, `/disclosure-override`) are refused: they
require a typed-confirm overlay that cannot exist without a TTY, and running them through a scripted
pipe would bypass the friction ladder on purpose — a safety regression we will not ship. The refusal
is always `Usage` (exit 1), never silent.

Usage: zero run [OPTIONS] <INPUT>...

Arguments:
  <INPUT>...
          The slash-command to run. Leading `/` is optional; arguments follow as separate tokens

Options:
      --api <API>
          Engine API endpoint (default: <https://api.getzero.dev>)
          
          [env: ZERO_API_URL=]

      --token <TOKEN>
          Operator token (or set `ZERO_API_TOKEN`)
          
          [env: ZERO_API_TOKEN=]

      --paper
          Start in paper mode

      --no-persist
          Disable session persistence for this invocation. The TUI runs from a fresh in-memory log
          and nothing is written to `~/.zero/state.db`

      --json
          Emit machine-readable output (where supported). Implies no color and no widgets

  -v, --verbose...
          Increase log verbosity (`-v` info, `-vv` debug, `-vvv` trace)

  -h, --help
          Print help (see a summary with '-h')
```

## Exit codes

Every subcommand uses the same taxonomy. The canonical definition lives on `enum ExitKind` in `crates/zero/src/main.rs`; this table is kept in sync by hand because clap does not emit exit-code docs into `--help`.

| Code | Name | Meaning |
|---|---|---|
| 0 | `Ok` | Command succeeded. |
| 1 | `Usage` | Invalid arguments, missing required flag, refusing to overwrite config without `--force`, or refusing a risk-increasing command in non-interactive mode. Anything the operator can fix by editing the invocation. |
| 2 | `EngineUnreachable` | Engine reachable check failed (DNS, TCP, 5xx, timeout). The CLI is healthy; the server is not. |
| 3 | `AuthInvalid` | Authentication failed (reserved; no call site emits this today — all engine errors collapse to code 2 until HTTP status is threaded through in M2). |
| 4 | `Internal` | Something the CLI did went wrong that is neither the operator's fault nor the engine's: disk I/O, JSON serialization, a caught panic. Always worth a bug report. |
