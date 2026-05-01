#!/usr/bin/env bash
# binary_size_guard.sh — enforce M1_PLAN §9 "release binary ≤ 5 MB".
#
# The budget is a proxy for two things the spec cares about:
#
# 1. **Dependency discipline.** Rust's dependency tree can
#    quietly balloon; a size regression is usually a new heavy
#    crate that wasn't strictly necessary. Flagging it in CI
#    forces a "did we need that?" conversation at PR time, not
#    post-release.
# 2. **Cold install latency.** Operators install `zero` with a
#    single `cargo install …` line. A 20 MB binary compresses
#    differently, caches differently, and feels heavier than
#    a 4 MB one — even when the CPU time is identical. 5 MB is
#    the threshold where "just download it" stops needing a
#    disclaimer.
#
# We guard the `release-small` profile specifically because that
# is the artifact shipped to users (`cargo install --profile
# release-small`). The default `release` profile is ~30 % larger
# because it trades size for compile time — fine for dev loops,
# not the shipped binary.
#
# Usage
# -----
#   scripts/binary_size_guard.sh [--profile release-small] [--budget-mb 5]
#
# Exits 0 (pass), 1 (over budget), 2 (usage / missing binary).
# Stable single-line output for CI log grep:
#   size: bytes=<n> mb=<m.mm> budget_mb=<n> profile=<name>

set -euo pipefail

profile="release-small"
budget_mb="5"

while [ $# -gt 0 ]; do
    case "$1" in
        --profile)   profile="$2"; shift 2 ;;
        --budget-mb) budget_mb="$2"; shift 2 ;;
        -h|--help)   sed -n '2,40p' "$0"; exit 0 ;;
        *)           echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

echo "building zero under --profile $profile ..." >&2
( cd "$repo_root" && cargo build -p zero --profile "$profile" ) >/dev/null

# `cargo build --profile release-small` writes to
# target/release-small/ regardless of host triple when no
# --target is passed. If a caller needs cross-target
# measurement they can export CARGO_BUILD_TARGET before
# invoking; we'd find the binary under target/<triple>/<profile>/
# instead. For now M1 only ships the host triple.
bin="$repo_root/target/$profile/zero"
if [ ! -f "$bin" ]; then
    echo "binary not found at $bin (did the build succeed?)" >&2
    exit 2
fi

# `stat -c %s` is GNU; BSD/macOS needs `stat -f %z`. Try both.
if bytes="$(stat -c %s "$bin" 2>/dev/null)"; then
    :
elif bytes="$(stat -f %z "$bin" 2>/dev/null)"; then
    :
else
    echo "no usable stat(1) for file size on this host" >&2
    exit 2
fi

# Convert bytes → MB with one decimal place, without needing
# bc or python. `awk` is POSIX and ubiquitous.
mb="$(awk -v b="$bytes" 'BEGIN { printf "%.2f", b / 1048576 }')"
budget_bytes=$(( budget_mb * 1024 * 1024 ))

echo "size: bytes=$bytes mb=$mb budget_mb=$budget_mb profile=$profile"

if [ "$bytes" -gt "$budget_bytes" ]; then
    echo "FAIL: release binary $mb MB exceeds budget ${budget_mb} MB" >&2
    echo "      (see M1_PLAN §9 — 'release binary ≤ 5 MB on darwin-arm64')" >&2
    echo "      Most likely cause: a new heavy dep in the main-binary crate." >&2
    exit 1
fi

echo "PASS: $mb MB ≤ ${budget_mb} MB budget"
