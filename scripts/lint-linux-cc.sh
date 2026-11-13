#!/bin/sh
# lint:linux C wrapper: compile C for aarch64 Linux with zig's bundled clang.
#
# Used only as CC_aarch64_unknown_linux_gnu inside scripts/lint-linux.sh, so
# normal macOS builds and other projects never see it. zig 0.17.1 via
# Homebrew (Tom-approved 2026-11-13); the target defaults to zig's bundled
# glibc headers, which is fine for a lint check that never runs the output.
#
# cc-rs appends --target=<rust triple> for the cross target, which zig
# rejects (it wants -target <zig triple>). The zig target is supplied at the
# exec below, so the rust-triple form is dropped here.
set -eu
count=$#
while [ "$count" -gt 0 ]; do
  a=$1; shift; count=$((count - 1))
  case $a in
    --target=*) ;;
    --target|-target)
      # Two-word form: drop the value too.
      shift; count=$((count - 1)) ;;
    *) set -- "$@" "$a" ;;
  esac
done
exec zig cc -target aarch64-linux-gnu "$@"
