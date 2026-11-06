#!/bin/sh
# Paseo worktree lifecycle hooks for this checkout.
#
# Both hooks run an Effigy binary built from this checkout, never the
# installed one on PATH. `setup` rebuilds and stages the binary; `teardown`
# reuses the staged binary and fails closed when it is missing.
#
#   worktree-lifecycle.sh setup      prepare, build, link
#   worktree-lifecycle.sh teardown   archive run logs, retire container scope, unlink
#
# The staged binary lives under .local-install/ (gitignored) so it survives
# between the separate setup and teardown shells. The cargo target directory
# is pinned so CARGO_TARGET_DIR or .cargo/config.toml cannot move the output.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd -P)
stage_dir="$root/.local-install/worktree-hooks"
staged="$stage_dir/effigy"
target_dir="$root/target/worktree-hooks"
die() {
  echo "[error] worktree-lifecycle: $*" >&2
  exit 1
}

cd "$root"

case "${1:-}" in
setup)
  rm -f "$staged"
  mkdir -p "$stage_dir"
  nucleus worktree prepare || die "nucleus worktree prepare failed"
  CARGO_TARGET_DIR="$target_dir" cargo build --bin effigy ||
    die "cargo build --bin effigy failed"
  built="$target_dir/debug/effigy"
  [ -f "$built" ] && [ -x "$built" ] || die "missing built binary: $built"
  cp "$built" "$staged.new" || die "failed to stage binary"
  chmod 0755 "$staged.new"
  mv "$staged.new" "$staged" || die "failed to activate staged binary"
  nucleus worktree link || die "nucleus worktree link failed"
  ;;
teardown)
  [ -f "$staged" ] && [ -x "$staged" ] ||
    die "no checkout-built binary at $staged; run setup first (no fallback to PATH)"
  # Preserve owned kernel failure evidence before the checkout disappears:
  # a bounded copy of this checkout's run logs. An archive failure is
  # reported but never skips the required retire/unlink cleanup below.
  archive_rc=0
  sh "$root/scripts/worktree-archive-run-logs.sh" "$root" || archive_rc=$?
  if "$staged" container retire --yes; then
    retire_rc=0
  else
    retire_rc=$?
  fi
  if [ "$retire_rc" -ne 0 ]; then
    echo "[error] worktree-lifecycle: container retire failed (exit $retire_rc); Nucleus unlink skipped" >&2
    if [ "$archive_rc" -ne 0 ]; then
      echo "[error] worktree-lifecycle: run-log archive also failed (exit $archive_rc)" >&2
    fi
    exit "$retire_rc"
  fi
  nucleus worktree unlink || die "nucleus worktree unlink failed"
  if [ "$archive_rc" -ne 0 ]; then
    echo "[error] worktree-lifecycle: run-log archive failed (exit $archive_rc); cleanup completed" >&2
    exit "$archive_rc"
  fi
  ;;
*)
  die "usage: worktree-lifecycle.sh setup|teardown"
  ;;
esac
