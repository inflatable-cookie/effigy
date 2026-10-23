#!/bin/sh
# Run one selector's Cargo command through Oxide.
#
#   scripts/oxide-selector.sh nextest run -p effigy --lib ...
#   scripts/oxide-selector.sh check -p effigy --all-targets
#
# Oxide leases a shared slot on the external Builds drive and writes the Rust
# intermediates there instead of this checkout's `target/`, so selectors stop
# writing hundreds of gigabytes to the internal SSD. Final binaries are still
# uplifted to `<workspace>/target/debug/`, so anything that names that path
# keeps working (Oxide #106).
#
# Escapes, in one place:
#   EFFIGY_OXIDE_ESCAPE=cargo effigy <selector>   plain Cargo for that run
#   effigy cargo <cargo args>                     plain Cargo, one command
# Hosted runners have no Oxide, so `GITHUB_ACTIONS=true` also runs plain
# Cargo; `.github/workflows` keep calling Cargo and these selectors unchanged.
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

# Tests model independent invocations in private roots, including foreground
# admission and fault traces. Keep host admission, but do not make those roots
# descendants of this selector's runtime grant. Normal task children still
# inherit their capabilities; opaque durable tasks must declare that intent.
case "${1:-}" in
  test | nextest) unset EFFIGY_PARENT_CAPABILITY ;;
esac

tool=${EFFIGY_OXIDE_ESCAPE:-oxide}
case $tool in
  oxide | cargo) ;;
  *)
    echo "oxide-selector: EFFIGY_OXIDE_ESCAPE must be oxide or cargo, not '$tool'" >&2
    exit 2
    ;;
esac
[ "${GITHUB_ACTIONS:-}" = "true" ] && tool=cargo

if [ "$tool" = cargo ]; then
  exec cargo "$@"
fi

if ! command -v oxide >/dev/null 2>&1; then
  echo "oxide-selector: oxide is not on PATH; set EFFIGY_OXIDE_ESCAPE=cargo for the plain-Cargo escape" >&2
  exit 127
fi
exec oxide "$@"
