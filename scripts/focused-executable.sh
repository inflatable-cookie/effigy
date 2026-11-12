#!/bin/sh
# Run one focused test selector with the Effigy binary its tests re-exec.
#
# A lib or integration test that runs a task step invoking Effigy is not the
# Effigy CLI itself: the runner refuses to re-exec a `deps/<test>` binary, so
# the nested invocation reads `EFFIGY_EXECUTABLE` and otherwise fails late,
# inside custody, with a relative path. `effigy test:focused` runs arbitrary
# `{args}` scope, so it cannot know which selected tests nest an invocation: it
# supplies this checkout's uplifted CLI for all of them and keeps an operator's
# explicit choice. A checkout with no built CLI gets one build through the
# repository's own Oxide route, so the first focused run cannot discover the gap
# as a confusing failure in a test that never mentions Effigy (effigy#946).
set -eu

if [ -n "${EFFIGY_EXECUTABLE:-}" ]; then
  exec "$@"
fi

script=$0
case $script in
  /*) ;;
  *) script=$PWD/$script ;;
esac
root=$(CDPATH= cd -- "$(dirname -- "$script")/.." 2>/dev/null && pwd) || root=
if [ -z "$root" ] || [ ! -f "$root/Cargo.toml" ]; then
  echo "focused-executable: cannot find the checkout root from '$0'" >&2
  exit 2
fi
executable=$root/target/debug/effigy
if [ ! -x "$executable" ]; then
  echo "focused-executable: building this checkout's CLI once so a nested Effigy invocation re-execs it, not a test-harness binary" >&2
  sh "$root/scripts/oxide-selector.sh" build --bin effigy
fi
if [ ! -x "$executable" ]; then
  echo "focused-executable: no EFFIGY_EXECUTABLE, and $executable is not built." >&2
  echo "Build this checkout's CLI once ('oxide build --bin effigy', which uplifts it to target/debug/effigy), or export EFFIGY_EXECUTABLE as the absolute path to an effigy binary." >&2
  exit 2
fi
EFFIGY_EXECUTABLE=$executable
export EFFIGY_EXECUTABLE
exec "$@"
