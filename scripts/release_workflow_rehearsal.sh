#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="${ZERO_REPO:-zero-intel/zero}"
TAG=""
EXECUTE=0
KEEP=0
TIMEOUT_SECONDS=2400

usage() {
  cat >&2 <<'USAGE'
usage: scripts/release_workflow_rehearsal.sh [--execute] [--keep] [--tag TAG] [--timeout-seconds N]

Dry-run by default. With --execute, creates a temporary prerelease tag on
origin/main, waits for .github/workflows/release.yml, verifies the generated
draft GitHub Release from a clean download, then deletes the draft release and
temporary tag unless --keep is set.
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
    --timeout-seconds)
      TIMEOUT_SECONDS="${2:-}"
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
  TAG="v0.0.0-release-workflow-rehearsal-$(date -u +%Y%m%d%H%M%S)"
fi
if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+[-+][0-9A-Za-z.-]+$ ]]; then
  echo "rehearsal tag must look like v0.0.0-release-workflow-rehearsal-YYYYMMDDHHMMSS" >&2
  exit 2
fi
if [[ ! "$TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || [[ "$TIMEOUT_SECONDS" -lt 300 ]]; then
  echo "timeout must be an integer >= 300 seconds" >&2
  exit 2
fi

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

run_release_checks() {
  local download_dir="$1"

  (
    cd "$download_dir"
    shasum -a 256 -c SHA256SUMS >/dev/null
  )
  python3 "$ROOT/scripts/release_verify.py" "$download_dir" >/dev/null
  python3 "$ROOT/scripts/homebrew_formula.py" \
    "$download_dir" \
    --tag "$TAG" \
    --output "$download_dir/zero.rb" \
    >/dev/null
  (
    cd "$download_dir"
    gh attestation verify zero-linux --repo "$REPO" >/dev/null
    gh attestation verify zero-macos --repo "$REPO" >/dev/null
  )
}

wait_for_release_run() {
  local deadline=$((SECONDS + TIMEOUT_SECONDS))
  local run_id=""

  while [[ "$SECONDS" -lt "$deadline" ]]; do
    run_id="$(
      gh run list \
        --repo "$REPO" \
        --workflow release.yml \
        --limit 30 \
        --json databaseId,headBranch \
        --jq ".[] | select(.headBranch == \"$TAG\") | .databaseId" \
        | head -n 1
    )"
    if [[ -n "$run_id" ]]; then
      echo "$run_id"
      return 0
    fi
    sleep 5
  done

  echo "timed out waiting for release workflow run for $TAG" >&2
  return 1
}

assert_required_jobs() {
  local run_id="$1"
  local jobs_json="$2"

  python3 - "$run_id" "$jobs_json" <<'PY'
import json
import sys

run_id = sys.argv[1]
jobs = json.loads(sys.argv[2])["jobs"]
required = {
    "public-proof",
    "registry-readiness",
    "python-package",
    "cli-binary (ubuntu-latest, zero-linux)",
    "cli-binary (macos-latest, zero-macos)",
    "container-smoke",
    "Draft GitHub Release",
}
by_name = {job["name"]: job["conclusion"] for job in jobs}
missing = sorted(required - set(by_name))
failed = sorted(name for name in required if by_name.get(name) != "success")
if missing or failed:
    raise SystemExit(
        json.dumps(
            {
                "run_id": run_id,
                "missing_required_jobs": missing,
                "failed_required_jobs": {
                    name: by_name.get(name) for name in failed if name in by_name
                },
            },
            indent=2,
            sort_keys=True,
        )
    )
PY
}

WORK_DIR="$(mktemp -d)"
TAG_CREATED=0

cleanup() {
  if [[ "$EXECUTE" -eq 1 && "$KEEP" -eq 0 ]]; then
    gh release delete "$TAG" --repo "$REPO" --yes --cleanup-tag >/dev/null 2>&1 || true
    gh api -X DELETE "repos/$REPO/git/refs/tags/$TAG" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

require_command gh
require_command python3
require_command shasum

if [[ "$EXECUTE" -eq 0 ]]; then
  gh auth status >/dev/null
  gh workflow view release.yml --repo "$REPO" >/dev/null
  echo "release workflow rehearsal dry-run passed: $TAG"
  echo "run with --execute to create the temporary tag and verify the real workflow"
  exit 0
fi

gh auth status >/dev/null
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

RUN_ID="$(wait_for_release_run)"
echo "release workflow run: https://github.com/$REPO/actions/runs/$RUN_ID"
gh run watch "$RUN_ID" --repo "$REPO" --exit-status

RUN_JSON="$(gh run view "$RUN_ID" --repo "$REPO" --json conclusion,jobs,status)"
python3 - "$RUN_JSON" <<'PY'
import json
import sys

run = json.loads(sys.argv[1])
if run["status"] != "completed" or run["conclusion"] != "success":
    raise SystemExit(json.dumps(run, indent=2, sort_keys=True))
PY
assert_required_jobs "$RUN_ID" "$RUN_JSON"

RELEASE_JSON="$(
  gh release view "$TAG" \
    --repo "$REPO" \
    --json assets,isDraft,url \
)"
ENGINE_VERSION="$(
  python3 - "$ROOT/engine/pyproject.toml" <<'PY'
import sys
import tomllib

with open(sys.argv[1], "rb") as handle:
    print(tomllib.load(handle)["project"]["version"])
PY
)"
python3 - "$TAG" "$ENGINE_VERSION" "$RELEASE_JSON" <<'PY'
import json
import sys

tag = sys.argv[1]
engine_version = sys.argv[2]
release = json.loads(sys.argv[3])
asset_names = {asset["name"] for asset in release["assets"]}
required_assets = {
    "PROVENANCE.json",
    "SBOM.spdx.json",
    "SHA256SUMS",
    f"zero_engine-{engine_version}.tar.gz",
    f"zero_engine-{engine_version}-py3-none-any.whl",
    "zero-linux",
    "zero-macos",
    "zero-paper-image.tar",
}
missing = sorted(required_assets - asset_names)
if not release["isDraft"] or missing:
    raise SystemExit(
        json.dumps(
            {
                "tag": tag,
                "isDraft": release["isDraft"],
                "missing_assets": missing,
            },
            indent=2,
            sort_keys=True,
        )
    )
PY

DOWNLOAD_DIR="$WORK_DIR/fresh-download"
mkdir -p "$DOWNLOAD_DIR"
gh release download "$TAG" --repo "$REPO" --dir "$DOWNLOAD_DIR"
run_release_checks "$DOWNLOAD_DIR"

if [[ "$KEEP" -eq 0 ]]; then
  gh release delete "$TAG" --repo "$REPO" --yes --cleanup-tag
  TAG_CREATED=0
  if gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
    echo "draft release still exists after rollback: $TAG" >&2
    exit 1
  fi
  if gh api "repos/$REPO/git/ref/tags/$TAG" >/dev/null 2>&1; then
    echo "temporary tag still exists after rollback: $TAG" >&2
    exit 1
  fi
  echo "release workflow rehearsal passed and rolled back: $TAG"
else
  echo "release workflow rehearsal passed and kept for inspection: $TAG"
fi
