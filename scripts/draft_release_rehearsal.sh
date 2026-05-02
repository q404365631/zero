#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="${ZERO_REPO:-zero-intel/zero}"
TAG=""
EXECUTE=0
KEEP=0

usage() {
  cat >&2 <<'USAGE'
usage: scripts/draft_release_rehearsal.sh [--execute] [--keep] [--tag TAG]

Dry-run by default. With --execute, creates a temporary draft GitHub Release,
downloads and verifies every asset, then deletes the draft release and tag.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --execute)
      EXECUTE=1
      shift
      ;;
    --keep)
      KEEP=1
      shift
      ;;
    --tag)
      TAG="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$TAG" ]]; then
  TAG="v0.0.0-release-rehearsal-$(date -u +%Y%m%d%H%M%S)"
fi
if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+[-+][0-9A-Za-z.-]+$ ]]; then
  echo "rehearsal tag must look like v0.0.0-release-rehearsal-YYYYMMDDHHMMSS" >&2
  exit 2
fi

WORK_DIR="$(mktemp -d)"
TAG_CREATED=0
RELEASE_CREATED=0

cleanup() {
  if [[ "$EXECUTE" -eq 1 && "$RELEASE_CREATED" -eq 1 && "$KEEP" -eq 0 ]]; then
    gh release delete "$TAG" --repo "$REPO" --yes --cleanup-tag >/dev/null 2>&1 || true
  fi
  if [[ "$EXECUTE" -eq 1 && "$TAG_CREATED" -eq 1 && "$KEEP" -eq 0 ]]; then
    gh api -X DELETE "repos/$REPO/git/refs/tags/$TAG" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

mkdir -p \
  "$WORK_DIR/downloaded/zero-engine-python" \
  "$WORK_DIR/downloaded/zero-linux" \
  "$WORK_DIR/downloaded/zero-macos" \
  "$WORK_DIR/downloaded/zero-paper-image"

printf 'fake wheel for draft release rehearsal\n' \
  >"$WORK_DIR/downloaded/zero-engine-python/zero_engine-0.0.0-py3-none-any.whl"
printf 'fake sdist for draft release rehearsal\n' \
  >"$WORK_DIR/downloaded/zero-engine-python/zero_engine-0.0.0.tar.gz"
printf '#!/usr/bin/env sh\necho zero linux draft rehearsal\n' \
  >"$WORK_DIR/downloaded/zero-linux/zero-linux"
printf '#!/usr/bin/env sh\necho zero macos draft rehearsal\n' \
  >"$WORK_DIR/downloaded/zero-macos/zero-macos"
printf 'fake paper image tar for draft release rehearsal\n' \
  >"$WORK_DIR/downloaded/zero-paper-image/zero-paper-image.tar"

"$ROOT/scripts/assemble_release_assets.sh" \
  "$WORK_DIR/downloaded" \
  "$WORK_DIR/release-dist"

python3 "$ROOT/scripts/release_verify.py" "$WORK_DIR/release-dist" >/dev/null
python3 "$ROOT/scripts/homebrew_formula.py" \
  "$WORK_DIR/release-dist" \
  --tag "$TAG" \
  --output "$WORK_DIR/zero.rb"

if [[ "$EXECUTE" -eq 0 ]]; then
  echo "draft release rehearsal dry-run passed: $TAG"
  echo "release assets: $WORK_DIR/release-dist"
  echo "homebrew formula: $WORK_DIR/zero.rb"
  exit 0
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "gh is required for --execute" >&2
  exit 1
fi
if gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  echo "release already exists: $TAG" >&2
  exit 1
fi
if gh api "repos/$REPO/git/ref/tags/$TAG" >/dev/null 2>&1; then
  echo "tag already exists: $TAG" >&2
  exit 1
fi

TARGET_SHA="$(gh api "repos/$REPO/git/ref/heads/main" --jq '.object.sha')"
gh api "repos/$REPO/git/refs" \
  -f "ref=refs/tags/$TAG" \
  -f "sha=$TARGET_SHA" \
  >/dev/null
TAG_CREATED=1

gh release create "$TAG" "$WORK_DIR/release-dist"/* \
  --repo "$REPO" \
  --draft \
  --prerelease \
  --latest=false \
  --verify-tag \
  --title "$TAG" \
  --notes "Temporary ZERO draft release rollback rehearsal. This release must be deleted by the rehearsal script."
RELEASE_CREATED=1

mkdir -p "$WORK_DIR/fresh-download"
gh release download "$TAG" --repo "$REPO" --dir "$WORK_DIR/fresh-download"
python3 "$ROOT/scripts/release_verify.py" "$WORK_DIR/fresh-download" >/dev/null
python3 "$ROOT/scripts/homebrew_formula.py" \
  "$WORK_DIR/fresh-download" \
  --tag "$TAG" \
  --output "$WORK_DIR/fresh-download/zero.rb"

if [[ "$KEEP" -eq 0 ]]; then
  gh release delete "$TAG" --repo "$REPO" --yes --cleanup-tag
  RELEASE_CREATED=0
  TAG_CREATED=0
  if gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
    echo "draft release still exists after rollback: $TAG" >&2
    exit 1
  fi
  if gh api "repos/$REPO/git/ref/tags/$TAG" >/dev/null 2>&1; then
    echo "temporary tag still exists after rollback: $TAG" >&2
    exit 1
  fi
  echo "draft release rehearsal passed and rolled back: $TAG"
else
  echo "draft release rehearsal passed and kept for inspection: $TAG"
fi
