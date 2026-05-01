#!/usr/bin/env bash
set -euo pipefail

repo="${ZERO_REPO:-zero-intel/zero}"
version="${ZERO_VERSION:-latest}"
install_dir="${ZERO_INSTALL_DIR:-$HOME/.local/bin}"
tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/zero-install.XXXXXX")"

cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

case "$(uname -s)" in
    Linux) asset="zero-linux" ;;
    Darwin) asset="zero-macos" ;;
    *)
        echo "unsupported OS: $(uname -s)" >&2
        exit 1
        ;;
esac

if ! command -v gh >/dev/null 2>&1; then
    echo "gh is required to download and verify ZERO release assets" >&2
    exit 1
fi

release_args=()
if [[ "$version" == "latest" ]]; then
    release_args+=(--repo "$repo")
else
    release_args+=("$version" --repo "$repo")
fi

gh release download "${release_args[@]}" --pattern "$asset" --dir "$tmpdir"
gh release download "${release_args[@]}" --pattern SHA256SUMS --dir "$tmpdir"

(
    cd "$tmpdir"
    shasum -a 256 -c SHA256SUMS --ignore-missing
    gh attestation verify "$asset" --repo "$repo"
)

mkdir -p "$install_dir"
install -m 0755 "$tmpdir/$asset" "$install_dir/zero"
echo "installed zero to $install_dir/zero"
