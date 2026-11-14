#!/bin/sh
# lint:linux C++ wrapper: companion to lint-linux-cc.sh for C++ sources.
#
# Used only as CXX_aarch64_unknown_linux_gnu inside scripts/lint-linux.sh.
# Drops cc-rs's --target=<rust triple> the same way (see lint-linux-cc.sh).
set -eu
count=$#
while [ "$count" -gt 0 ]; do
  a=$1; shift; count=$((count - 1))
  case $a in
    --target=*) ;;
    --target|-target)
      shift; count=$((count - 1)) ;;
    *) set -- "$@" "$a" ;;
  esac
done
exec zig c++ -target aarch64-linux-gnu "$@"
