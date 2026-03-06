#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINK_DIR="${EFFIGY_LOCAL_BIN_DIR:-$HOME/.local/bin}"
STABLE_TARGET="$ROOT_DIR/.local-install/bin/effigy"
DEV_TARGET="$ROOT_DIR/scripts/effigy-dev"
STABLE_LINK="$LINK_DIR/effigy"
DEV_LINK="$LINK_DIR/effigy-dev"

ensure_file() {
  local path="$1"
  local label="$2"
  if [[ ! -f "$path" ]]; then
    echo "[error] missing $label: $path" >&2
    exit 1
  fi
}

replace_link() {
  local target="$1"
  local link_path="$2"

  if [[ -e "$link_path" || -L "$link_path" ]]; then
    if [[ ! -L "$link_path" ]]; then
      echo "[error] refusing to replace non-symlink path: $link_path" >&2
      exit 1
    fi
    rm -f "$link_path"
  fi

  ln -s "$target" "$link_path"
}

ensure_file "$STABLE_TARGET" "stable effigy binary"
ensure_file "$DEV_TARGET" "effigy-dev wrapper"

mkdir -p "$LINK_DIR"
replace_link "$STABLE_TARGET" "$STABLE_LINK"
replace_link "$DEV_TARGET" "$DEV_LINK"

echo "[ok] linked stable channel: $STABLE_LINK -> $STABLE_TARGET"
echo "[ok] linked dev channel: $DEV_LINK -> $DEV_TARGET"

if [[ ":$PATH:" != *":$LINK_DIR:"* ]]; then
  echo "[warn] $LINK_DIR is not currently on PATH" >&2
  echo "       add: export PATH=\"$LINK_DIR:\$PATH\"" >&2
fi

echo "[note] if your shell still resolves \`effigy\` as an alias, remove that alias from your shell rc and restart the shell"
