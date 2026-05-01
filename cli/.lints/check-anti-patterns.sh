#!/usr/bin/env bash
# Anti-pattern lint — spec §25 enforced as a grep gate.
#
# These checks scan `crates/` for forbidden strings. Keep the list
# short; real enforcement lives in clippy lints and code review. The
# point is to fail loudly on the obvious regressions (celebration
# copy, model names in chrome, ambient animations, etc.).

set -euo pipefail

cd "$(dirname "$0")/.."

fail=0

check() {
  local pattern="$1"
  local reason="$2"
  local scope="${3:-crates}"
  # -I skips binary files, -n prints line numbers, -r recursive.
  # We grep only .rs files to avoid noise from docs + TOML.
  local hits
  hits=$(grep -rnI --include='*.rs' -E "$pattern" "$scope" || true)
  if [[ -n "$hits" ]]; then
    echo "✗ $reason"
    echo "  pattern: $pattern"
    echo "$hits" | sed 's/^/    /'
    echo
    fail=1
  fi
}

echo "zero CLI — anti-pattern lint (§25)"
echo

# Celebration / gamification copy.
check '🎉|🎊|🚀|✨|🔥|🎯|💎|🏆' "no celebration emoji in TUI renderers"
check '\bstreak!|congrats|achievement unlocked|you crushed|nice job\b' \
  "no celebration copy"

# Marketing chrome.
check '\bAI[ -]powered\b|\bpowered by (Claude|GPT|Anthropic|OpenAI)\b' \
  "no marketing copy in chrome"

# Model strings in chrome (widgets/ / status bar).
check '\b(gpt-[0-9]|claude-[0-9]|opus|sonnet|haiku)\b' \
  "no model strings in chrome" \
  "crates/zero-tui/src"

# Hidden or default-admin modes that bypass Plan mode.
check '\bauto_execute\b.*=\s*true\b' \
  "Auto-execute flag must not default to true"

# Naked numeric formatting of engine state outside <Stat>.
# Advisory pattern — not all violations are real; code review backs this.
# Intentionally scoped narrowly to avoid false positives in tests.
check 'format!\("\{[^}]*\}",\s*state\.' \
  "engine state numbers must render through StatWidget, not format!" \
  "crates/zero-tui/src/widgets"

if [[ $fail -ne 0 ]]; then
  echo "anti-pattern lint failed."
  exit 1
fi

echo "✓ anti-pattern lint clean"
