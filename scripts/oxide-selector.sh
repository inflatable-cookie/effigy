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
#
# A harness with a private HOME still uses the real shared pool (see below).
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

# Oxide resolves its default pool from $HOME, so a review or validation harness
# that runs a selector with a temporary HOME would create a cold pool under that
# HOME on the internal SSD: about 3.3-3.8 GB per run (measured 2026-10-24).
# Oxide shares one pool between concurrent runs, so Effigy's own HOME isolation
# is unaffected by passing the real pool through. Resolve it the way a normal
# run does - `oxide status` under the account's real home - and never hard-code
# a path. The same block covers the `exec -- <tool>` route, which reaches Oxide
# through this script too.
if [ -z "${OXIDE_POOL:-}" ]; then
  real_home=$( (eval echo "~$(id -un)") 2>/dev/null ) || real_home=
  case $real_home in
    /?*) ;;
    *) real_home= ;;
  esac
  if [ -z "$real_home" ]; then
    echo "oxide-selector: could not find the account's real home; set OXIDE_POOL to the shared Oxide pool so this run does not create a cold pool under ${HOME:-an unset HOME}" >&2
  elif [ "${HOME:-}" != "$real_home" ]; then
    real_pool=$(HOME="$real_home" oxide status 2>/dev/null |
      awk '/^pool:/ { sub(/^pool:[ \t]*/, ""); sub(/[ \t]*\(.*$/, ""); print; exit }') || real_pool=
    case ${real_pool:-} in
      /?*) export OXIDE_POOL="$real_pool" ;;
      *)
        echo "oxide-selector: no pool reported by 'oxide status' under $real_home; set OXIDE_POOL to the shared Oxide pool so this private-HOME run (${HOME:-unset HOME}) does not create a cold pool on the internal SSD" >&2
        ;;
    esac
  fi
fi
exec oxide "$@"
