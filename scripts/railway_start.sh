#!/usr/bin/env bash
set -euo pipefail

PORT="${PORT:-8765}"
ZERO_JOURNAL_PATH="${ZERO_JOURNAL_PATH:-/data/decisions.jsonl}"
ZERO_HYPERLIQUID_LIVE_PRICES="${ZERO_HYPERLIQUID_LIVE_PRICES:-true}"

mkdir -p "$(dirname "${ZERO_JOURNAL_PATH}")"

args=(
  zero-paper-api
  --host 0.0.0.0
  --port "${PORT}"
  --journal "${ZERO_JOURNAL_PATH}"
)

case "${ZERO_HYPERLIQUID_LIVE_PRICES}" in
  1|true|TRUE|yes|YES|on|ON)
    args+=(--hyperliquid-live-prices)
    ;;
  0|false|FALSE|no|NO|off|OFF)
    ;;
  *)
    echo "ZERO_HYPERLIQUID_LIVE_PRICES must be true or false" >&2
    exit 64
    ;;
esac

exec "${args[@]}"
