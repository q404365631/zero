#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python_bin="${PYTHON:-python3}"
tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/zero-package-dry-run.XXXXXX")"

cleanup() {
    rm -rf "$tmpdir"
    rm -rf "$repo_root/engine/build" "$repo_root/engine/src/zero_engine.egg-info"
}
trap cleanup EXIT

echo "== python package dry run =="
"$python_bin" -m build --sdist --wheel --outdir "$tmpdir/python-dist" "$repo_root/engine"

wheel_count="$(find "$tmpdir/python-dist" -maxdepth 1 -name '*.whl' | wc -l | tr -d ' ')"
sdist_count="$(find "$tmpdir/python-dist" -maxdepth 1 -name '*.tar.gz' | wc -l | tr -d ' ')"
if [[ "$wheel_count" != "1" || "$sdist_count" != "1" ]]; then
    echo "expected one wheel and one sdist, got wheels=$wheel_count sdists=$sdist_count" >&2
    exit 1
fi

echo "== rust crate package dry run =="
(
    cd "$repo_root/cli"
    cargo package --workspace --no-verify --allow-dirty --target-dir "$tmpdir/cargo-target"
)

crate_count="$(find "$tmpdir/cargo-target/package" -maxdepth 1 -name '*.crate' | wc -l | tr -d ' ')"
if [[ "$crate_count" -lt "1" ]]; then
    echo "expected cargo package to produce at least one .crate file" >&2
    exit 1
fi

echo "package dry run passed: wheel=1 sdist=1 crates=$crate_count"
