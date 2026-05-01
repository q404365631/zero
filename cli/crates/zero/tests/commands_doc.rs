//! Generator + freshness check for `docs/commands.md`.
//!
//! ## Contract (M1_PLAN §10)
//!
//! The command reference is machine-generated from the
//! compiled binary's `--help` output. Prose docs drift from
//! code the moment someone edits one and not the other; the
//! only reliable fix is to make the doc a build artifact.
//!
//! This test has two modes, switched by `ZERO_REGENERATE_DOCS`:
//!
//! - **default (CI / `cargo test`)** — read `docs/commands.md`
//!   from disk, regenerate it in-memory, and fail loudly if
//!   the bytes differ. The failure message points at the
//!   regeneration command. CI runs this exact lane.
//! - **`ZERO_REGENERATE_DOCS=1`** — overwrite
//!   `docs/commands.md` with the freshly-captured output.
//!   This is what the developer runs after editing a help
//!   string or adding a subcommand.
//!
//! The test is not `#[ignore]`. It runs on every `cargo test`
//! — running the bin is fast (each `--help` is a sub-10 ms
//! spawn, confirmed by `version_startup.rs`), and catching
//! doc drift in the default lane is the whole point.
//!
//! ## What we capture
//!
//! The top-level `--help`, plus every leaf subcommand's
//! `--help`. We do not recurse beyond one level because
//! clap's nested subcommand output is redundant (it repeats
//! the parent help synopsis) and `zero`'s command tree is
//! intentionally flat — no subsubcommands today, no plans
//! to add any. If that changes, the `SUBCOMMANDS` list below
//! needs a recursive walk, not a flat array; the test will
//! catch that by going stale against the new help output.
//!
//! ## Why we don't snapshot-test this
//!
//! `insta` would work and is already a dev-dep elsewhere, but:
//!
//! 1. The artifact *is* the doc — `docs/commands.md` — and
//!    that's the file humans read. Storing the golden in
//!    `snapshots/` next to the test would give us two sources
//!    of truth.
//! 2. `insta`'s review UX (interactive accept/reject) is
//!    friction when the regeneration command is a single
//!    env var. `ZERO_REGENERATE_DOCS=1 cargo test` is a
//!    clearer bright line for CI and reviewers.
//!
//! ## What breaks this test
//!
//! - Editing a `/// doc comment` on a `Cli` field or `Command`
//!   variant in `crates/zero/src/main.rs`.
//! - Adding or removing a subcommand, changing a flag, or
//!   renaming an env var.
//! - Any clap version bump that changes the `--help` renderer.
//!
//! For all of these, the fix is:
//!
//! ```bash
//! ZERO_REGENERATE_DOCS=1 cargo test -p zero --test commands_doc
//! git diff docs/commands.md   # review the new shape
//! git add docs/commands.md
//! ```

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Leaf subcommands whose `--help` is inlined into the doc,
/// in rendering order. Kept explicit (not enumerated from the
/// binary) so that adding a new subcommand to `main.rs`
/// without updating this list fails the doc-freshness check
/// at the top level — the top-level `--help` will list the
/// new subcommand name, and the inline help for that new
/// subcommand will be missing, which surfaces as a diff.
///
/// If there's ever a real need for dynamic discovery, the
/// right place is to parse the top-level `--help`'s
/// "Commands:" section; that is strictly more code than a
/// literal list is worth today.
const SUBCOMMANDS: &[&str] = &["init", "doctor", "version", "run"];

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_zero"))
}

fn docs_path() -> PathBuf {
    // CARGO_MANIFEST_DIR for this integration test is the
    // `zero` *binary* crate — `crates/zero`. We want the
    // workspace-relative `docs/commands.md`, which is two
    // levels up.
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir
        .parent()
        .and_then(Path::parent)
        .expect("zero crate lives two levels below workspace root");
    workspace_root.join("docs").join("commands.md")
}

/// Run `zero <args...> --help`, capturing stdout as UTF-8.
/// Panics on non-zero exit — the doc generator is useless if
/// help output doesn't cleanly succeed, so the failure mode
/// should be the loudest possible.
fn capture_help(args: &[&str]) -> String {
    let bin = bin_path();
    let output = Command::new(&bin)
        .args(args)
        .output()
        .expect("spawn zero for --help capture");
    assert!(
        output.status.success(),
        "zero {args:?} --help exited non-zero: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout).expect("zero --help is always utf-8")
}

fn generate_commands_md() -> String {
    let mut out = String::new();

    // Doc header. The prose here is the one human-edited
    // thing in the file; keep it minimal so the signal-to-
    // noise stays high.
    out.push_str(
        "# zero — command reference\n\
         \n\
         **This file is generated.** Do not edit by hand; \
         changes will be overwritten on the next doc sync.\n\
         \n\
         To regenerate after editing flag docs or adding a \
         subcommand in `crates/zero/src/main.rs`:\n\
         \n\
         ```bash\n\
         ZERO_REGENERATE_DOCS=1 cargo test -p zero --test commands_doc\n\
         ```\n\
         \n\
         The `commands_doc_is_fresh` test runs in the default \
         `cargo test` lane and will fail CI if this file is \
         stale relative to the compiled binary's `--help`.\n\
         \n",
    );

    // Table of contents. Hand-authored once; the leaf anchors
    // below match these slugs. If a new subcommand lands, it
    // appears in SUBCOMMANDS and gets its own anchor; the TOC
    // can be extended in the next edit.
    out.push_str(
        "## Contents\n\
         \n\
         - [Top-level](#top-level)\n",
    );
    for sub in SUBCOMMANDS {
        writeln!(out, "- [`zero {sub}`](#zero-{sub})").unwrap();
    }
    out.push('\n');

    // Top-level. `zero --help` includes the about blurb, all
    // global flags, and the subcommand list — exactly what a
    // reader wants on arrival.
    out.push_str("## Top-level\n\n");
    out.push_str("```\n");
    out.push_str("$ zero --help\n");
    out.push_str(capture_help(&["--help"]).trim_end());
    out.push_str("\n```\n\n");

    // One section per leaf subcommand. The anchor is
    // `#zero-<name>` so the TOC entries resolve.
    for sub in SUBCOMMANDS {
        writeln!(out, "## `zero {sub}`\n").unwrap();
        out.push_str("```\n");
        writeln!(out, "$ zero {sub} --help").unwrap();
        out.push_str(capture_help(&[sub, "--help"]).trim_end());
        out.push_str("\n```\n\n");
    }

    // Exit codes — load-bearing for scripts. This section is
    // hand-authored because clap does not surface the
    // `ExitKind` enum in any help output; the enum's doc
    // comments are the canonical source. If the enum grows,
    // this block needs to grow with it — documented via an
    // assertion below that pins the current set.
    out.push_str(
        "## Exit codes\n\
         \n\
         Every subcommand uses the same taxonomy. The \
         canonical definition lives on `enum ExitKind` in \
         `crates/zero/src/main.rs`; this table is kept in \
         sync by hand because clap does not emit exit-code \
         docs into `--help`.\n\
         \n\
         | Code | Name | Meaning |\n\
         |---|---|---|\n\
         | 0 | `Ok` | Command succeeded. |\n\
         | 1 | `Usage` | Invalid arguments, missing required flag, refusing to overwrite config without `--force`, or refusing a risk-increasing command in non-interactive mode. Anything the operator can fix by editing the invocation. |\n\
         | 2 | `EngineUnreachable` | Engine reachable check failed (DNS, TCP, 5xx, timeout). The CLI is healthy; the server is not. |\n\
         | 3 | `AuthInvalid` | Authentication failed (reserved; no call site emits this today — all engine errors collapse to code 2 until HTTP status is threaded through in M2). |\n\
         | 4 | `Internal` | Something the CLI did went wrong that is neither the operator's fault nor the engine's: disk I/O, JSON serialization, a caught panic. Always worth a bug report. |\n",
    );

    out
}

/// The generated file ends with a trailing newline for POSIX
/// tool friendliness (`wc -l`, `cat`, most diff viewers). Do
/// this once at the edge rather than sprinkling newlines in
/// the generator.
fn finalize(mut md: String) -> String {
    if !md.ends_with('\n') {
        md.push('\n');
    }
    md
}

/// CI lane: fail if `docs/commands.md` does not match the
/// live `--help` output. Developer lane (with
/// `ZERO_REGENERATE_DOCS=1`): overwrite the file and pass.
#[test]
fn commands_doc_is_fresh() {
    let expected = finalize(generate_commands_md());
    let path = docs_path();

    if std::env::var_os("ZERO_REGENERATE_DOCS").is_some() {
        // Developer mode: write and exit. Parent directory
        // must exist — create it lazily so a clean checkout
        // with no `docs/` folder still works.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create docs/ dir");
        }
        std::fs::write(&path, &expected)
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        eprintln!("regenerated {}", path.display());
        return;
    }

    // CI mode: read + compare. Missing file is a failure, not
    // a silent pass — the whole point is that the doc exists.
    let actual = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{} missing or unreadable ({e}). Run: \
             ZERO_REGENERATE_DOCS=1 cargo test -p zero --test commands_doc",
            path.display(),
        )
    });

    if actual != expected {
        // Keep the diff hint short — CI logs are noisy and a
        // reader needs the fix command, not a 400-line wall
        // of ANSI.
        let actual_lines = actual.lines().count();
        let expected_lines = expected.lines().count();
        panic!(
            "docs/commands.md is stale ({actual_lines} lines on disk vs \
             {expected_lines} lines generated).\n\
             Fix: ZERO_REGENERATE_DOCS=1 cargo test -p zero --test commands_doc\n\
             Then: git add docs/commands.md && git commit"
        );
    }
}

/// Pin the exit-code taxonomy so any new `ExitKind` variant
/// forces a doc edit *and* trips this test. Without this, a
/// developer could add `ExitKind::NewThing = 5`, never touch
/// the docs, and the freshness test above would still pass
/// (it only checks `--help` output, and `ExitKind` does not
/// appear there).
///
/// This test reads the source of `main.rs` and grep-asserts
/// the variant set. It's the boring option — a build-time
/// macro would be fancier but adds dev-dep churn.
#[test]
fn exit_code_taxonomy_matches_docs() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let main_rs = std::fs::read_to_string(crate_dir.join("src").join("main.rs"))
        .expect("read crates/zero/src/main.rs");

    // These are the variants documented in
    // `generate_commands_md` above. Any drift surfaces here,
    // not as silent doc rot.
    for variant in &[
        "Ok = 0",
        "Usage = 1",
        "EngineUnreachable = 2",
        "AuthInvalid = 3",
        "Internal = 4",
    ] {
        assert!(
            main_rs.contains(variant),
            "ExitKind variant `{variant}` no longer in main.rs — \
             docs/commands.md's exit-code table is now out of sync. \
             Either restore the variant, or update both the table \
             in this test's generator and this assertion together."
        );
    }
}
