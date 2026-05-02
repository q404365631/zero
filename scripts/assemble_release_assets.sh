#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <downloaded-artifacts-dir> <release-dist-dir>" >&2
    exit 2
fi

artifacts_dir="$1"
release_dir="$2"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

mkdir -p "$release_dir"
cp "$artifacts_dir"/zero-engine-python/*.whl "$release_dir"/
cp "$artifacts_dir"/zero-engine-python/*.tar.gz "$release_dir"/
cp "$artifacts_dir"/zero-linux/zero-linux "$release_dir"/
cp "$artifacts_dir"/zero-macos/zero-macos "$release_dir"/
cp "$artifacts_dir"/zero-paper-image/zero-paper-image.tar "$release_dir"/
python3 "$repo_root/scripts/release_provenance.py" "$release_dir"
python3 "$repo_root/scripts/write_sha256s.py" "$release_dir/SHA256SUMS" "$release_dir"/*
