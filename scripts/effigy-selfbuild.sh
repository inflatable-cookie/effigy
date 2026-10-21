#!/bin/sh
# Build this checkout's own binary, then run it.
#
# `oxide build --bin effigy` keeps Rust intermediates in Oxide's shared slot
# pool (the external Builds drive) and uplifts the final binary to
# `target/debug/effigy`, the same path plain Cargo uses since Oxide #106.
# Executing that path directly - never `oxide run` - avoids holding the slot
# lease while a selector's nested Oxide calls wait behind it.
#
#   scripts/effigy-selfbuild.sh <effigy args...>
#   scripts/effigy-selfbuild.sh --bin effigy-qa <args...>
#
# Escapes for when Oxide itself is broken:
#   scripts/effigy-selfbuild.sh --cargo <args...>
#   EFFIGY_OXIDE_ESCAPE=cargo scripts/effigy-selfbuild.sh <args...>
#   EFFIGY_SELF_BUILD_TOOL=cargo scripts/effigy-selfbuild.sh <args...>
#   cargo run --bin effigy -- <selector>   (no wrapper at all)
# `EFFIGY_OXIDE_ESCAPE` is the same switch the selector wrapper reads, so one
# variable escapes a whole selector run, including its nested self-builds.
# Hosted runners have no Oxide, so `GITHUB_ACTIONS=true` also builds with
# Cargo; `.github/workflows` keep calling Cargo unchanged.
#
# The binary path lives here and nowhere else, so a future profile move is a
# one-line change in this script.
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

bin=effigy
tool=${EFFIGY_SELF_BUILD_TOOL:-${EFFIGY_OXIDE_ESCAPE:-oxide}}
while [ $# -gt 0 ]; do
  case $1 in
    --bin)
      [ $# -ge 2 ] || {
        echo "effigy-selfbuild: --bin needs a binary name" >&2
        exit 2
      }
      bin=$2
      shift 2
      ;;
    --cargo)
      tool=cargo
      shift
      ;;
    --)
      shift
      break
      ;;
    *)
      break
      ;;
  esac
done

case $tool in
  oxide | cargo) ;;
  *)
    echo "effigy-selfbuild: EFFIGY_OXIDE_ESCAPE/EFFIGY_SELF_BUILD_TOOL must be oxide or cargo, not '$tool'" >&2
    exit 2
    ;;
esac
[ "${GITHUB_ACTIONS:-}" = "true" ] && tool=cargo

if [ "$tool" = cargo ]; then
  (cd "$root" && cargo build --bin "$bin")
  exec "$root/target/debug/$bin" "$@"
fi

if ! command -v oxide >/dev/null 2>&1; then
  echo "effigy-selfbuild: oxide is not on PATH; run 'cargo run --bin $bin --' or set EFFIGY_OXIDE_ESCAPE=cargo" >&2
  exit 127
fi
(cd "$root" && oxide build --bin "$bin")
exec "$root/target/debug/$bin" "$@"
