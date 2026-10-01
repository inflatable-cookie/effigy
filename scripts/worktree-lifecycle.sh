#!/bin/sh
# Paseo worktree lifecycle hooks for this checkout.
#
# Both hooks run an Effigy binary built from this checkout, never the
# installed one on PATH. `setup` rebuilds and stages the binary; `teardown`
# reuses the staged binary and fails closed when it is missing.
#
#   worktree-lifecycle.sh setup      build, prepare catalog sibling, link
#   worktree-lifecycle.sh teardown   retire container scope, unlink
#
# The staged binary lives under .local-install/ (gitignored) so it survives
# between the separate setup and teardown shells. The cargo target directory
# is pinned so CARGO_TARGET_DIR or .cargo/config.toml cannot move the output.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd -P)
stage_dir="$root/.local-install/worktree-hooks"
staged="$stage_dir/effigy"
target_dir="$root/target/worktree-hooks"
skill_path=${NORTHSTAR_SKILL_PATH:-$HOME/.agents/skills/northstar}

die() {
  echo "[error] worktree-lifecycle: $*" >&2
  exit 1
}

skill() {
  "$staged" skill run --path "$skill_path" paseo:worktree -- "$@"
}

cd "$root"

case "${1:-}" in
setup)
  rm -f "$staged"
  mkdir -p "$stage_dir"
  CARGO_TARGET_DIR="$target_dir" cargo build --bin effigy ||
    die "cargo build --bin effigy failed"
  built="$target_dir/debug/effigy"
  [ -f "$built" ] && [ -x "$built" ] || die "missing built binary: $built"
  cp "$built" "$staged.new" || die "failed to stage binary"
  chmod 0755 "$staged.new"
  mv "$staged.new" "$staged" || die "failed to activate staged binary"
  skill prepare ../effigy-catalog-pack
  skill link
  ;;
teardown)
  [ -f "$staged" ] && [ -x "$staged" ] ||
    die "no checkout-built binary at $staged; run setup first (no fallback to PATH)"
  "$staged" container retire --yes
  skill unlink
  ;;
*)
  die "usage: worktree-lifecycle.sh setup|teardown"
  ;;
esac
