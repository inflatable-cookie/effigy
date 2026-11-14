#!/bin/sh
# lint:linux driver: strict Clippy for aarch64-unknown-linux-gnu across the
# workspace, using zig as the Linux-target C compiler.
#
# The CC/CXX/AR_aarch64_unknown_linux_gnu exports are local to this process
# and its children (the oxide-selector Clippy runs below), so normal macOS
# builds and other projects are unaffected. The actual Cargo invocation still
# goes through scripts/oxide-selector.sh, keeping Oxide pool routing,
# private-HOME pass-through and the EFFIGY_OXIDE_ESCAPE=cargo escape.
#
# Without zig on PATH the selector keeps today's narrowed scope
# (effigy-process, which needs no cross C toolchain) and prints one line
# saying so; it never fails just because zig is missing.
#
# The widened scope mirrors the feature sets CI's `lint` job checks
# (.github/workflows/ci.yml), each with the Linux target appended.
set -eu

script=$0
case $script in
  /*) ;;
  *) script=$PWD/$script ;;
esac
root=$(CDPATH= cd -- "$(dirname -- "$script")/.." 2>/dev/null && pwd) || root=
if [ ! -f "$root/Cargo.toml" ]; then
  root=$(git rev-parse --show-toplevel)
fi

if ! command -v zig >/dev/null 2>&1; then
  echo "lint:linux: zig not on PATH; running narrowed effigy-process scope"
  exec sh "$root/scripts/oxide-selector.sh" clippy -p effigy-process --lib \
    --target aarch64-unknown-linux-gnu -- -D warnings
fi

export CC_aarch64_unknown_linux_gnu="$root/scripts/lint-linux-cc.sh"
export CXX_aarch64_unknown_linux_gnu="$root/scripts/lint-linux-cxx.sh"
export AR_aarch64_unknown_linux_gnu="$root/scripts/lint-linux-ar.sh"

sh "$root/scripts/oxide-selector.sh" clippy --workspace --all-targets \
  --target aarch64-unknown-linux-gnu -- -D warnings
sh "$root/scripts/oxide-selector.sh" clippy --workspace --all-targets \
  --target aarch64-unknown-linux-gnu \
  --features effigy-kernel/fault-injection -- -D warnings
sh "$root/scripts/oxide-selector.sh" clippy -p effigy -p effigy-containers -p effigy-kernel \
  --all-targets --target aarch64-unknown-linux-gnu \
  --features effigy/workspace-private-qualification,effigy-kernel/fault-injection -- -D warnings
sh "$root/scripts/oxide-selector.sh" clippy -p effigy \
  --bin effigy --test workspace_client --target aarch64-unknown-linux-gnu \
  --features workspace-private-qualification,workspace-serving-fixture -- -D warnings
sh "$root/scripts/oxide-selector.sh" clippy -p effigy-kernel \
  --target aarch64-unknown-linux-gnu \
  --features fault-injection,workspace-private-qualification \
  --bin kernel-custody-fixture -- -D warnings
