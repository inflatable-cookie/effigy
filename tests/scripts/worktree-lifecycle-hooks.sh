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
  cp "$here/scripts/worktree-archive-run-logs.sh" "$repo/scripts/"
  mkdir -p "$fx/archive-base"
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
      EFFIGY_RUN_LOG_ARCHIVE_BASE="$fx/archive-base" \
      PATH="$fx/stale:$fx/bin:/usr/bin:/bin" sh scripts/worktree-lifecycle.sh "$1") \
      >"$fx/out.log" 2>&1
  }
}

# A failed config lookup must still retire and unlink.
new_fixture
run_hook setup || exit 2
mkdir -p "$repo/.effigy/state/runs/1"
: >"$repo/.effigy/state/runs/1/log"
(cd "$repo" && HOME="$fx/home" NORTHSTAR_SKILL_PATH="$fx/skill" \
  STUB_FAIL_ON=__worktree-archive-scratch-root PATH="$fx/stale:$fx/bin:/usr/bin:/bin" \
  sh scripts/worktree-lifecycle.sh teardown) >"$fx/out.log" 2>&1; rc=$?
[ "$rc" -ne 0 ] || fail "config failure not reported"
grep -q 'container retire --yes' "$log" || fail "retire skipped after config failure"
grep -q 'skill run.*unlink' "$log" || fail "unlink skipped after config failure"
grep -q 'run-log archive failed' "$fx/out.log" || fail "archive error missing"
ok "config failure preserves cleanup"

# 1. setup: build precedes link; stale PATH effigy never called.
new_fixture
run_hook setup; rc=$?
[ "$rc" -eq 0 ] || fail "setup exit $rc"
grep -q STALE-EFFIGY "$log" && fail "setup invoked stale effigy"
grep -q "target=$repo/target/worktree-hooks" "$log" || fail "cargo target dir not pinned"
[ "$(grep -o 'cargo build\|link' "$log" | tr '\n' ' ')" = 'cargo build link ' ] ||
  fail "setup order wrong: $(cat "$log")"
[ -x "$repo/.local-install/worktree-hooks/effigy" ] && ok "setup stages binary + order" || fail "binary not staged"

# 2. teardown in a separate shell reuses the staged binary; the run-log
# archive runs first, then retire before unlink.
run_hook teardown; rc=$?
[ "$rc" -eq 0 ] || fail "teardown exit $rc"
grep -q STALE-EFFIGY "$log" && fail "teardown invoked stale effigy"
grep -q "worktree-archive-run-logs: status=no-state" "$fx/out.log" ||
  fail "archive step did not run first: $(cat "$fx/out.log")"
tail -2 "$log" | tr '\n' ' ' | grep -q "container retire --yes.*unlink" && ok "teardown order, binary persisted" || fail "teardown order wrong: $(cat "$log")"
rm -rf "$fx"

# 3. teardown with no staged binary fails closed, no PATH fallback.
new_fixture
run_hook teardown; rc=$?
[ "$rc" -ne 0 ] || fail "teardown without binary succeeded"
grep -q STALE-EFFIGY "$log" && fail "teardown fell back to stale effigy"
ok "teardown missing binary fails closed"
rm -rf "$fx"

# 4. build failure: nonzero, nothing staged, no link; teardown then fails closed.
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

# 6. link failure: nonzero.
new_fixture
(STUB_FAIL_ON=link; export STUB_FAIL_ON; run_hook setup); rc=$?
[ "$rc" -ne 0 ] || fail "setup succeeded despite link failure"
grep -q STALE-EFFIGY "$log" && fail "setup fell back to stale effigy"
ok "link failure stops setup"
rm -rf "$fx"

# 7. retire failure: nonzero, unlink not run.
new_fixture
run_hook setup || fail "setup"
(STUB_FAIL_ON="container retire"; export STUB_FAIL_ON; run_hook teardown); rc=$?
[ "$rc" -ne 0 ] || fail "teardown succeeded despite retire failure"
grep -q "built-effigy.*unlink" "$log" && fail "unlink ran after retire failure"
ok "retire failure stops unlink"
rm -rf "$fx"

# 8. teardown archives this checkout's run logs before retiring, with
# exact bytes; retire and unlink still run after a successful archive.
new_fixture
run_hook setup || fail "setup"
mkdir -p "$repo/.effigy/state/runs/7"
printf 'failure bytes\n' >"$repo/.effigy/state/runs/7/0.stdout.0.log"
run_hook teardown; rc=$?
[ "$rc" -eq 0 ] || fail "teardown exit $rc"
archived=$(ls -d "$fx/archive-base"/qa-run-logs.* 2>/dev/null | head -1)
[ -n "$archived" ] || fail "no archive dir under the fixture base"
cmp -s "$repo/.effigy/state/runs/7/0.stdout.0.log" "$archived/7.0.stdout.0.log" ||
  fail "archived bytes differ"
grep -q "^ok	7/0.stdout.0.log	" "$archived/manifest.tsv" || fail "manifest missing the run log"
grep -q "worktree-archive-run-logs: status=ok" "$fx/out.log" ||
  fail "archive status not reported: $(cat "$fx/out.log")"
tail -2 "$log" | tr '\n' ' ' | grep -q "container retire --yes.*unlink" ||
  fail "cleanup did not follow the archive: $(cat "$log")"
ok "teardown archives run logs before cleanup"
rm -rf "$fx"

# 9. an archive failure is reported but retire and unlink are still
# attempted, and the teardown stays non-zero.
new_fixture
run_hook setup || fail "setup"
mkdir -p "$repo/.effigy/state/runs/7"
printf 'failure bytes\n' >"$repo/.effigy/state/runs/7/0.stdout.0.log"
chmod 0555 "$fx/archive-base"
run_hook teardown; rc=$?
chmod 0755 "$fx/archive-base"
[ "$rc" -ne 0 ] || fail "teardown succeeded despite archive failure"
grep -q "worktree-archive-run-logs: error:" "$fx/out.log" ||
  fail "archive failure not reported: $(cat "$fx/out.log")"
grep -q "built-effigy container retire --yes" "$log" || fail "retire skipped after archive failure"
grep -q "built-effigy.*unlink" "$log" || fail "unlink skipped after archive failure"
ok "archive failure still cleans up, teardown stays non-zero"
rm -rf "$fx"

# 10. an unavailable archive base fails the teardown after retire and
# unlink are still attempted.
new_fixture
run_hook setup || fail "setup"
mkdir -p "$repo/.effigy/state/runs/7"
printf 'failure bytes\n' >"$repo/.effigy/state/runs/7/0.stdout.0.log"
rm -rf "$fx/archive-base"
touch "$fx/archive-base"
run_hook teardown; rc=$?
rm -f "$fx/archive-base"
mkdir -p "$fx/archive-base"
[ "$rc" -ne 0 ] || fail "teardown succeeded despite unavailable archive base"
grep -q "worktree-archive-run-logs: status=unavailable" "$fx/out.log" ||
  fail "unavailable archive not reported: $(cat "$fx/out.log")"
grep -q "built-effigy container retire --yes" "$log" || fail "retire skipped after unavailable archive"
grep -q "built-effigy.*unlink" "$log" || fail "unlink skipped after unavailable archive"
ok "unavailable archive still cleans up, teardown stays non-zero"
rm -rf "$fx"

[ "$fails" -eq 0 ] && echo "all lifecycle hook tests passed" || { echo "$fails failure(s)" >&2; exit 1; }
