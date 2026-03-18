#!/usr/bin/env bash
set -euo pipefail

BIN_PATH="${1:-}"
MAX_GLIBC="${2:-}"

if [[ -z "$BIN_PATH" || -z "$MAX_GLIBC" ]]; then
  echo "usage: $0 <binary-path> <max-glibc-version>" >&2
  exit 64
fi

if [[ ! -f "$BIN_PATH" ]]; then
  echo "[error] binary not found: $BIN_PATH" >&2
  exit 1
fi

collect_glibc_versions() {
  if command -v readelf >/dev/null 2>&1; then
    readelf --version-info "$BIN_PATH" 2>/dev/null
  elif command -v objdump >/dev/null 2>&1; then
    objdump -T "$BIN_PATH" 2>/dev/null
  else
    strings "$BIN_PATH"
  fi | grep -o 'GLIBC_[0-9]\+\.[0-9]\+' | sort -Vu
}

glibc_versions="$(collect_glibc_versions || true)"

if [[ -z "$glibc_versions" ]]; then
  echo "[ok] no dynamic GLIBC symbol requirements found: $BIN_PATH"
  exit 0
fi

required_glibc="$(printf '%s\n' "$glibc_versions" | tail -n1)"
required_glibc="${required_glibc#GLIBC_}"
highest_version="$(printf '%s\n%s\n' "$MAX_GLIBC" "$required_glibc" | sort -V | tail -n1)"

if [[ "$highest_version" != "$MAX_GLIBC" ]]; then
  echo "[error] $BIN_PATH requires GLIBC_$required_glibc but the release floor is GLIBC_$MAX_GLIBC" >&2
  exit 1
fi

echo "[ok] $BIN_PATH GLIBC floor is compatible (requires GLIBC_$required_glibc, max GLIBC_$MAX_GLIBC)"
