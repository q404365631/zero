#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON_BIN="${PYTHON:-python3}"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/zero-fresh-clone.XXXXXX")"
EXPORT_DIR="$WORK_DIR/zero"
KEEP="${ZERO_KEEP_FRESH_CLONE_REHEARSAL:-0}"

cleanup() {
  if [[ "$KEEP" == "1" ]]; then
    echo "zero fresh clone rehearsal: kept $EXPORT_DIR"
    return
  fi

  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

"$PYTHON_BIN" - "$ROOT" "$EXPORT_DIR" <<'PY'
import os
import shutil
import subprocess
import sys

root, dest = sys.argv[1], sys.argv[2]
os.makedirs(dest, exist_ok=True)

raw = subprocess.check_output(
    [
        "git",
        "-C",
        root,
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
    ],
)

paths = [path.decode("utf-8") for path in raw.split(b"\0") if path]
copied = 0

for rel in paths:
    if rel.startswith((".git/", "cli/target/", "node_modules/")):
        continue

    src = os.path.join(root, rel)
    if not os.path.isfile(src):
        continue

    dst = os.path.join(dest, rel)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy2(src, dst)
    os.chmod(dst, os.stat(src).st_mode & 0o777)
    copied += 1

print(f"zero fresh clone rehearsal: copied {copied} files")
PY

cd "$EXPORT_DIR"
test ! -d .git
test -f README.md
test -x scripts/public_readiness_gate.sh
test -x scripts/paper_api_smoke.sh

scripts/public_readiness_gate.sh >/tmp/zero-fresh-clone-public-readiness.log
scripts/hardening_gate.sh >/tmp/zero-fresh-clone-hardening.log
PYTHONPATH="$PWD/engine/src" "$PYTHON_BIN" examples/paper-trading/run.py \
  >/tmp/zero-fresh-clone-paper-example.log
scripts/paper_api_smoke.sh >/tmp/zero-fresh-clone-paper-api-smoke.log

echo "zero fresh clone rehearsal: ok path=$EXPORT_DIR"
