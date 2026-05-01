#!/usr/bin/env bash
# idle_rss_check.sh — enforce M1_PLAN §9 "idle TUI RSS ≤ 40 MB".
#
# The TUI renders once on launch, parks on `tokio::select!`
# waiting for input and engine events, and does nothing else.
# That is what "idle" means here: the first frame is drawn,
# no keys have been pressed, no engine event has landed. That
# steady-state is where the operator spends most of their time,
# so it is the honest number to budget.
#
# Why a shell script and not a `#[test]`
# --------------------------------------
# The TUI refuses to initialise unless stdout is a TTY. Rust's
# integration-test harness runs with inherited stdio, which is
# a pipe to the cargo runner — no TTY. We could spin up a PTY
# from Rust using e.g. `rustix_openpty`, but a) that adds a
# dev-dep to the critical path just for one measurement, and
# b) on macOS the `ps rss` number is the most honest when the
# process is observed from outside Rust's test harness. A
# script keeps the measurement boundary crisp.
#
# Runtime dependencies (all standard on macOS and Linux):
#   - bash 4+
#   - ps (POSIX)
#   - a PTY allocator: `script` (BSD and util-linux variants)
#     or `expect`. On macOS the default `script` is BSD.
#
# Usage
# -----
#   scripts/idle_rss_check.sh [--profile release-small] [--budget-kb 40960]
#
# With no args it uses `release-small` (the shipped profile) and
# the 40 MB spec budget. Exit 0 on success, non-zero on budget
# overrun or missing tooling. Prints the measured RSS to stdout
# so CI logs capture the actual number, not just pass/fail.
#
# Output line (stable for log grepping):
#   idle-rss: pid=<pid> rss_kb=<number> budget_kb=<number> profile=<name>

set -euo pipefail

# `script` requires a real controlling terminal to allocate a
# PTY. If this process is itself running without a TTY (e.g.
# inside a CI step that pipes output, a Docker container
# started without -t, or an editor sub-shell), we cannot boot
# the TUI at all and the check is not runnable. Bail early
# with a distinctive exit code (77, the autoconf convention
# for "skipped") so CI matrices can treat it as "skipped",
# not "failed".
if ! [ -t 0 ] && ! [ -t 1 ]; then
    echo "skip: no controlling TTY — rerun from an interactive shell or a CI job with -t" >&2
    exit 77
fi

profile="release-small"
budget_kb=40960   # 40 MB · 1024
settle_secs=3      # how long we let the TUI reach steady-state
                   # before reading RSS. 3 s is enough to render
                   # the first frame, drain tokio's bootstrap,
                   # and let the allocator flush small scratch
                   # allocations. Shorter gives noisy numbers.

while [ $# -gt 0 ]; do
    case "$1" in
        --profile)   profile="$2"; shift 2 ;;
        --budget-kb) budget_kb="$2"; shift 2 ;;
        --settle)    settle_secs="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,40p' "$0"; exit 0 ;;
        *)
            echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

# Locate repo root relative to this script. The script must
# work whether called via absolute path, relative path, or
# symlink.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

bin_dir="$repo_root/target/$profile"
if [ "$profile" = "release" ] || [ "$profile" = "release-small" ]; then
    cargo_profile_arg="--profile $profile"
else
    echo "unsupported profile: $profile (use release or release-small)" >&2
    exit 2
fi

echo "building zero under --profile $profile ..." >&2
( cd "$repo_root" && cargo build -p zero $cargo_profile_arg ) >/dev/null

bin="$bin_dir/zero"
if [ ! -x "$bin" ]; then
    echo "binary not found at $bin after build" >&2
    exit 2
fi

# Pick a PTY allocator. macOS ships BSD `script` which uses
# `script -q /dev/null <cmd>`; util-linux's takes
# `script -q -c <cmd> /dev/null`. Detect by the man-page
# signature (probing `--version` is unreliable across both).
pty_run() {
    if script -q /dev/null /bin/echo probe >/dev/null 2>&1; then
        # BSD (macOS default)
        script -q /dev/null "$@"
    elif script -q -c "/bin/echo probe" /dev/null >/dev/null 2>&1; then
        # util-linux (most Linux distros)
        script -q -c "$*" /dev/null
    else
        echo "no usable 'script' tool found — install bsdmainutils or util-linux" >&2
        return 3
    fi
}

# Start the TUI under a PTY in the background. Redirect the
# pseudo-terminal output to /dev/null so we don't pollute the
# script's own stdout — we only care about RSS.
tmp_err="$(mktemp)"
trap 'rm -f "$tmp_err"' EXIT

# shellcheck disable=SC2086
pty_run "$bin" >/dev/null 2>"$tmp_err" &
pty_pid=$!

# The TUI launches as a child of `script`. We want the RSS of
# the `zero` process itself, not `script`'s. Find it by walking
# descendants of $pty_pid. On macOS ps -o ppid= works; on
# Linux too.
find_zero_pid() {
    # Try up to ~2 s to find the child; TUI startup can take
    # a moment for Rust's panic hook, tokio runtime init, and
    # the first ratatui draw.
    for _ in $(seq 1 20); do
        local pid
        pid="$(pgrep -P "$pty_pid" zero 2>/dev/null | head -n1 || true)"
        if [ -n "$pid" ]; then
            echo "$pid"; return 0
        fi
        sleep 0.1
    done
    return 1
}

if ! zero_pid="$(find_zero_pid)"; then
    echo "failed to locate zero child pid under script wrapper $pty_pid" >&2
    kill "$pty_pid" 2>/dev/null || true
    exit 4
fi

# Let the TUI reach steady-state. See `settle_secs` comment
# above for the rationale.
sleep "$settle_secs"

# ps -o rss= reports RSS in KB on both macOS and Linux. No
# trailing header, just the number.
rss_kb="$(ps -o rss= -p "$zero_pid" 2>/dev/null | tr -d ' ')"
if [ -z "$rss_kb" ]; then
    echo "failed to read RSS for pid $zero_pid" >&2
    kill "$pty_pid" 2>/dev/null || true
    exit 5
fi

# Tear down cleanly — send Ctrl-C equivalent through a TERM.
# The TUI's signal handler will flatten state and exit; we
# wait briefly then kill the script wrapper.
kill "$zero_pid" 2>/dev/null || true
sleep 0.3
kill "$pty_pid" 2>/dev/null || true
wait "$pty_pid" 2>/dev/null || true

echo "idle-rss: pid=$zero_pid rss_kb=$rss_kb budget_kb=$budget_kb profile=$profile"

if [ "$rss_kb" -gt "$budget_kb" ]; then
    mb_used=$(( rss_kb / 1024 ))
    mb_budget=$(( budget_kb / 1024 ))
    echo "FAIL: idle TUI RSS ${mb_used} MB exceeds budget ${mb_budget} MB" >&2
    echo "      (see M1_PLAN §9 — 'idle TUI RSS ≤ 40 MB')" >&2
    exit 1
fi

mb_used=$(( rss_kb / 1024 ))
mb_budget=$(( budget_kb / 1024 ))
echo "PASS: idle TUI RSS ${mb_used} MB ≤ ${mb_budget} MB budget"
