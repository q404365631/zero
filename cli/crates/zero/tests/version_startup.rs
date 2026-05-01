//! `zero --version` startup-time regression (M1_PLAN §9, ≤ 150 ms p95).
//!
//! ## What this test actually measures
//!
//! We spawn the real compiled `zero` binary N times with
//! `--version`, time the wall-clock from `Command::spawn` to
//! `Child::wait`, and assert that the p95 of those samples sits
//! below the spec budget of 150 ms.
//!
//! This is *end-to-end startup latency*: process creation,
//! dynamic linker warmup, `clap` parsing, the `--version`
//! short-circuit, and teardown. It is the honest number an
//! operator sees when they type the command at a shell prompt.
//! A criterion microbench cannot measure this — criterion
//! operates inside a single process and can't observe `execve`.
//!
//! ## Why `#[ignore]` by default
//!
//! 1. We want the release binary, and the default `cargo test`
//!    profile is `dev`. A debug build of `zero` spawns in
//!    ~25–80 ms on modern hardware depending on the dynamic
//!    loader's state; a release build is typically <10 ms.
//!    Measuring debug vs 150 ms would be spurious either way.
//! 2. The test spends ~5 s wall clock by design (warmup + 50
//!    timed spawns) — we don't want that in every `cargo test`
//!    run. It belongs in a dedicated perf lane.
//!
//! Run it explicitly:
//!
//! ```bash
//! cargo test --release -p zero --test version_startup -- --ignored --nocapture
//! ```
//!
//! Or with the small-size profile to verify the shipped binary:
//!
//! ```bash
//! cargo test --profile release-small -p zero --test version_startup \
//!     -- --ignored --nocapture
//! ```
//!
//! ## Why we pick p95, not mean
//!
//! The operator's perception of "instant" is dominated by the
//! worst cases, not the average. A p95 budget catches
//! occasional linker-cache misses and FS warm-up stalls that a
//! mean would hide. Criterion reports mean because its scope is
//! CPU-bound work; spawn latency is dominated by I/O, where
//! the tail is the story.
//!
//! ## What happens if the binary isn't built
//!
//! `CARGO_BIN_EXE_zero` is injected by cargo for integration
//! tests, so the binary is always built before this runs. No
//! manual setup needed.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// Spec budget from Addendum A §2 and M1_PLAN §9. A p95 under
/// this means the operator experiences `zero --version` as
/// indistinguishable from a shell builtin.
const BUDGET_P95: Duration = Duration::from_millis(150);

/// Number of timed samples. 50 gives a stable p95 without
/// dominating CI time (~2–5 s total on release). Smaller Ns
/// make the p95 estimator noisy; larger ones waste CI budget.
const SAMPLES: usize = 50;

/// Warmup spawns thrown away before the real measurement.
/// The first spawn of any binary pays the dynamic-linker tax
/// (page-in, relocation, ld.so cache miss). After a couple of
/// runs the hot-path stabilises. Three warmup runs is the
/// minimum that reliably eliminates the cold outlier on both
/// macOS and Linux in my testing.
const WARMUP: usize = 3;

fn bin_path() -> PathBuf {
    // Cargo sets this for integration tests. It points to the
    // freshly-compiled `zero` binary in the current profile's
    // target dir, so release vs debug is whatever the test run
    // used. That's what we want — a test run with
    // `cargo test --release` measures the release binary.
    PathBuf::from(env!("CARGO_BIN_EXE_zero"))
}

fn time_one(bin: &PathBuf) -> Duration {
    let start = Instant::now();
    // `.output()` waits for exit *and* captures stdio, which
    // adds a small pipe-drain cost that would inflate the
    // measurement. Use `.spawn()` + `.wait()` and null the
    // streams explicitly so we measure only startup, not
    // string formatting into our harness.
    let mut child = Command::new(bin)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn zero --version");
    let status = child.wait().expect("wait on zero --version");
    let elapsed = start.elapsed();
    assert!(status.success(), "zero --version exited non-zero: {status}");
    elapsed
}

/// Release-mode startup tripwire. See module docs for the
/// "why `#[ignore]`" rationale and the recommended invocations.
#[test]
#[ignore = "perf lane: run with --ignored, prefer --release"]
fn version_startup_under_p95_budget() {
    let bin = bin_path();

    // Warmup. We deliberately discard these timings.
    for _ in 0..WARMUP {
        let _ = time_one(&bin);
    }

    let mut samples: Vec<Duration> = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        samples.push(time_one(&bin));
    }

    samples.sort_unstable();
    let p50 = samples[SAMPLES / 2];
    // p95 as the 95th percentile of the sorted samples. With
    // 50 samples, index 47 is the 95th percentile under the
    // nearest-rank method. Avoiding off-by-one here matters:
    // index 48 would be p97.
    let p95_idx = (SAMPLES * 95).div_ceil(100) - 1;
    let p95 = samples[p95_idx];
    let max = *samples.last().unwrap();

    eprintln!(
        "version startup: n={SAMPLES} p50={p50:?} p95={p95:?} max={max:?} \
         budget_p95={BUDGET_P95:?}"
    );

    assert!(
        p95 <= BUDGET_P95,
        "zero --version p95 {p95:?} exceeded spec budget {BUDGET_P95:?}. \
         Distribution: p50={p50:?} max={max:?} (n={SAMPLES}). \
         Likely culprits: a new heavy dep pulled into main, a \
         global static with nontrivial init, or a tracing \
         subscriber built on the eager path."
    );
}

/// Smoke test that `--version` even produces a parseable line.
/// Runs in the default `cargo test` lane because it's fast
/// (single spawn, no timing). If this test fails, the startup
/// bench result above is meaningless — we'd be measuring a
/// broken binary.
#[test]
fn version_prints_a_semver_line() {
    let bin = bin_path();
    let output = Command::new(&bin)
        .arg("--version")
        .output()
        .expect("run zero --version");
    assert!(
        output.status.success(),
        "non-zero exit: {:?}",
        output.status
    );
    let stdout = String::from_utf8(output.stdout).expect("utf-8 version output");
    // clap's default `--version` renderer is `<bin> <version>\n`.
    // We don't hardcode the version string — that would churn
    // on every release — but we do check the shape so a
    // regression that e.g. prints JSON or nothing is caught.
    assert!(
        stdout.starts_with("zero "),
        "expected 'zero <version>', got: {stdout:?}"
    );
    assert!(
        stdout.split_whitespace().count() >= 2,
        "expected at least two tokens, got: {stdout:?}"
    );
}
