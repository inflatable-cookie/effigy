#!/bin/sh
# Fixture tests for scripts/worktree-lifecycle.sh. Everything lives under a
# fresh mktemp root: stub cargo, stub staged effigy, and a stale PATH effigy
# that fails the run if it is ever invoked. No real profile, registry,
# container, checkout or fixed /tmp path is touched.
set -u

here=$(cd "$(dirname "$0")/../.." && pwd -P)
fails=0
fail() { echo "FAIL: $*" >&2; fails=$((fails + 1)); }
ok() { echo "ok: $*"; }

new_fixture() {
  fx=$(mktemp -d "${TMPDIR:-/tmp}/effigy-lifecycle.XXXXXX") || exit 2
  fx=$(cd "$fx" && pwd -P)
  repo="$fx/checkout"
  mkdir -p "$repo/scripts" "$fx/bin" "$fx/stale" "$fx/skill" "$fx/home"
  cp "$here/scripts/worktree-lifecycle.sh" "$repo/scripts/"
  log="$fx/events.log"
  : >"$log"
  # Stale installed effigy: any call is a failure marker.
  cat >"$fx/stale/effigy" <<STALE
#!/bin/sh
echo "STALE-EFFIGY \$*" >>"$log"
exit 97
STALE
  # Stub cargo: records the pinned target dir, writes a fake effigy binary.
  cat >"$fx/bin/cargo" <<CARGO
#!/bin/sh
echo "cargo \$* target=\${CARGO_TARGET_DIR:-unset}" >>"$log"
[ -n "\${STUB_CARGO_FAIL:-}" ] && exit 5
mkdir -p "\$CARGO_TARGET_DIR/debug"
cat >"\$CARGO_TARGET_DIR/debug/effigy" <<BIN
#!/bin/sh
echo "built-effigy \\\$*" >>"$log"
case "\\\$*" in
  *"\\\${STUB_FAIL_ON:-@@none@@}"*) exit 7 ;;
esac
exit 0
BIN
chmod 0755 "\$CARGO_TARGET_DIR/debug/effigy"
CARGO
  chmod 0755 "$fx/stale/effigy" "$fx/bin/cargo"
  run_hook() { # mode
    (cd "$repo" && HOME="$fx/home" NORTHSTAR_SKILL_PATH="$fx/skill" \
      PATH="$fx/stale:$fx/bin:/usr/bin:/bin" sh scripts/worktree-lifecycle.sh "$1") \
      >"$fx/out.log" 2>&1
  }
}

# 1. setup: build precedes prepare/link; stale PATH effigy never called.
new_fixture
run_hook setup; rc=$?
[ "$rc" -eq 0 ] || fail "setup exit $rc"
grep -q STALE-EFFIGY "$log" && fail "setup invoked stale effigy"
grep -q "target=$repo/target/worktree-hooks" "$log" || fail "cargo target dir not pinned"
[ "$(grep -o 'cargo build\|prepare\|link' "$log" | tr '\n' ' ')" = 'cargo build prepare link ' ] ||
  fail "setup order wrong: $(cat "$log")"
[ -x "$repo/.local-install/worktree-hooks/effigy" ] && ok "setup stages binary + order" || fail "binary not staged"

# 2. teardown in a separate shell reuses the staged binary; retire before unlink.
run_hook teardown; rc=$?
[ "$rc" -eq 0 ] || fail "teardown exit $rc"
grep -q STALE-EFFIGY "$log" && fail "teardown invoked stale effigy"
tail -2 "$log" | tr '\n' ' ' | grep -q "container retire --yes.*unlink" && ok "teardown order, binary persisted" || fail "teardown order wrong: $(cat "$log")"
rm -rf "$fx"

# 3. teardown with no staged binary fails closed, no PATH fallback.
new_fixture
run_hook teardown; rc=$?
[ "$rc" -ne 0 ] || fail "teardown without binary succeeded"
grep -q STALE-EFFIGY "$log" && fail "teardown fell back to stale effigy"
ok "teardown missing binary fails closed"
rm -rf "$fx"

# 4. build failure: nonzero, nothing staged, no prepare/link; teardown then fails closed.
new_fixture
(STUB_CARGO_FAIL=1; export STUB_CARGO_FAIL; run_hook setup); rc=$?
[ "$rc" -ne 0 ] || fail "setup succeeded despite build failure"
grep -q "built-effigy\|STALE" "$log" && fail "ran effigy after build failure"
[ -e "$repo/.local-install/worktree-hooks/effigy" ] && fail "binary staged after failed build"
run_hook teardown && fail "teardown ran after failed build"
ok "build failure"
rm -rf "$fx"

# 5. a previously staged binary is discarded when the rebuild fails.
new_fixture
run_hook setup || fail "initial setup"
: >"$log"
(STUB_CARGO_FAIL=1; export STUB_CARGO_FAIL; run_hook setup) && fail "rebuild failure succeeded"
[ -e "$repo/.local-install/worktree-hooks/effigy" ] && fail "stale staged binary survived failed rebuild"
ok "stale staged binary not trusted"
rm -rf "$fx"

# 6. prepare failure: nonzero, link not run.
new_fixture
(STUB_FAIL_ON=prepare; export STUB_FAIL_ON; run_hook setup); rc=$?
[ "$rc" -ne 0 ] || fail "setup succeeded despite prepare failure"
grep -q "built-effigy.*link" "$log" && fail "link ran after prepare failure"
ok "prepare failure stops link"
rm -rf "$fx"

# 7. retire failure: nonzero, unlink not run.
new_fixture
run_hook setup || fail "setup"
(STUB_FAIL_ON="container retire"; export STUB_FAIL_ON; run_hook teardown); rc=$?
[ "$rc" -ne 0 ] || fail "teardown succeeded despite retire failure"
grep -q "built-effigy.*unlink" "$log" && fail "unlink ran after retire failure"
ok "retire failure stops unlink"
rm -rf "$fx"

[ "$fails" -eq 0 ] && echo "all lifecycle hook tests passed" || { echo "$fails failure(s)" >&2; exit 1; }
