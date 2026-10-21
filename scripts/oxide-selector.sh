#!/bin/sh
# Run one selector's Cargo command through Oxide.
#
#   scripts/oxide-selector.sh nextest run -p effigy --lib ...
#   scripts/oxide-selector.sh check -p effigy --all-targets
#
# Oxide leases a shared slot on the external Builds drive and writes the Rust
# intermediates there instead of this checkout's `target/`, so selectors stop
# writing hundreds of gigabytes to the internal SSD. Final binaries are still
# uplifted to `<workspace>/target/<profile>/`, so anything that names that
# path keeps working.
#
# Escapes, in one place:
#   EFFIGY_OXIDE_ESCAPE=cargo effigy <selector>   plain Cargo for that run
#   effigy cargo <cargo args>                     plain Cargo, one command
# Hosted runners have no Oxide, so `GITHUB_ACTIONS=true` also runs plain
# Cargo; `.github/workflows` keep calling Cargo and these selectors unchanged.
#
# Until Oxide #106 moves `quick` output to `target/debug`, nextest needs
# `--cargo-profile oxide-quick` to share the quick slot. It is added here,
# never in the manifest.
#
# `--profile-dir` prints the directory Cargo puts this profile's final
# binaries in, so a selector that has to name a built helper binary works
# under both tools. It goes away with #106.
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

tool=${EFFIGY_OXIDE_ESCAPE:-oxide}
case $tool in
  oxide | cargo) ;;
  *)
    echo "oxide-selector: EFFIGY_OXIDE_ESCAPE must be oxide or cargo, not '$tool'" >&2
    exit 2
    ;;
esac
[ "${GITHUB_ACTIONS:-}" = "true" ] && tool=cargo

if [ "${1:-}" = "--profile-dir" ]; then
  if [ "$tool" = cargo ]; then
    echo "$root/target/debug"
  else
    echo "$root/target/oxide-quick"
  fi
  exit 0
fi

if [ "$tool" = cargo ]; then
  exec cargo "$@"
fi

if [ "${1:-}" = nextest ]; then
  shift
  case " $* " in
    *" --cargo-profile "* | *" --cargo-profile="*) ;;
    *)
      if [ "${1:-}" = run ]; then
        shift
        set -- run --cargo-profile oxide-quick "$@"
      else
        set -- "$@" --cargo-profile oxide-quick
      fi
      ;;
  esac
  set -- nextest "$@"
fi

if ! command -v oxide >/dev/null 2>&1; then
  echo "oxide-selector: oxide is not on PATH; set EFFIGY_OXIDE_ESCAPE=cargo for the plain-Cargo escape" >&2
  exit 127
fi
exec oxide "$@"
