#!/usr/bin/env bash
set -euo pipefail

mode="${1:---check}"
if [[ "$mode" != "--check" && "$mode" != "--clean" ]]; then
  echo "usage: scripts/stale_artifact_check.sh [--check|--clean]" >&2
  exit 2
fi

tracked_artifacts() {
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    return
  fi

  git ls-files | awk '
    /(^|\/)__pycache__(\/|$)/ ||
    /(^|\/)\.pytest_cache(\/|$)/ ||
    /(^|\/)\.ruff_cache(\/|$)/ ||
    /(^|\/)\.mypy_cache(\/|$)/ ||
    /(^|\/)htmlcov(\/|$)/ ||
    /(^|\/)dist(\/|$)/ ||
    /(^|\/)build(\/|$)/ ||
    /(^|\/)[^\/]+\.egg-info(\/|$)/ ||
    /\.py[co]$/ ||
    /\.db(-.*)?$/ ||
    /\.sqlite3?$/ ||
    /\.wal$/ ||
    /\.shm$/ ||
    /\.log$/ ||
    /\.err$/ ||
    /(^|\/)\.coverage$/ ||
    /(^|\/)coverage\.xml$/ ||
    /(^|\/)\.env(\.local)?$/ ||
    /\.pem$/ ||
    /\.key$/ {
      print
    }
  '
}

find_artifacts() {
  find . \
    -path ./.git -prune -o \
    -path ./cli/target -prune -o \
    -path ./node_modules -prune -o \
    -path ./.venv -prune -o \
    \( \
      -type d \( \
        -name __pycache__ -o \
        -name .pytest_cache -o \
        -name .ruff_cache -o \
        -name .mypy_cache -o \
        -name htmlcov -o \
        -name dist -o \
        -name build -o \
        -name '*.egg-info' \
      \) -o \
      -type f \( \
        -name '*.pyc' -o \
        -name '*.pyo' -o \
        -name '*.db' -o \
        -name '*.db-*' -o \
        -name '*.sqlite' -o \
        -name '*.sqlite3' -o \
        -name '*.wal' -o \
        -name '*.shm' -o \
        -name '*.log' -o \
        -name '*.err' -o \
        -name '.coverage' -o \
        -name 'coverage.xml' -o \
        -name '.env' -o \
        -name '.env.local' -o \
        -name '*.pem' -o \
        -name '*.key' \
      \) \
    \) -print | sort
}

tracked="$(tracked_artifacts)"
if [[ -n "$tracked" ]]; then
  echo "tracked stale artifacts are forbidden:" >&2
  printf '%s\n' "$tracked" >&2
  exit 1
fi

found="$(find_artifacts)"
if [[ -z "$found" ]]; then
  echo "stale artifact check passed"
  exit 0
fi

if [[ "$mode" == "--check" ]]; then
  echo "stale generated artifacts found:" >&2
  printf '%s\n' "$found" >&2
  echo "run: just stale-artifact-clean" >&2
  exit 1
fi

while IFS= read -r path; do
  [[ -z "$path" ]] && continue
  rm -rf "$path"
done <<< "$found"

remaining="$(find_artifacts)"
if [[ -n "$remaining" ]]; then
  echo "failed to remove stale artifacts:" >&2
  printf '%s\n' "$remaining" >&2
  exit 1
fi

echo "stale artifacts removed"
