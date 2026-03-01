#!/usr/bin/env bash

set -euo pipefail

if [[ "$#" -eq 0 ]]; then
  set -- README.md docs/README.md docs/guides/*.md
fi

failures=0

is_ignored_link() {
  local link="$1"
  [[ "$link" =~ ^https?:// ]] && return 0
  [[ "$link" =~ ^mailto: ]] && return 0
  [[ "$link" =~ ^# ]] && return 0
  return 1
}

check_file() {
  local file="$1"

  # Extract markdown inline links: [label](target)
  while IFS= read -r line; do
    local link="${line#*|}"
    [[ -z "$link" ]] && continue
    is_ignored_link "$link" && continue

    local target="$link"
    target="${target%%\#*}"
    [[ -z "$target" ]] && continue

    local resolved
    resolved="$(cd "$(dirname "$file")" && realpath "$target" 2>/dev/null || true)"
    if [[ -z "$resolved" || ! -e "$resolved" ]]; then
      printf 'broken link: %s -> %s\n' "$file" "$link"
      failures=$((failures + 1))
    fi
  done < <(
    perl -ne 'while (/\[[^\]]+\]\(([^)]+)\)/g) { print "$1\n"; }' "$file" \
      | awk -v f="$file" '{ print f "|" $0 }'
  )
}

for f in "$@"; do
  [[ -f "$f" ]] || continue
  check_file "$f"
done

if [[ "$failures" -gt 0 ]]; then
  printf '\nlink check failed: %d broken link(s)\n' "$failures"
  exit 1
fi

printf 'link check passed\n'
