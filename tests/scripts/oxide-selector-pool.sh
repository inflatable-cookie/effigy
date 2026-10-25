#!/bin/sh
# Fixture tests for the OXIDE_POOL pass-through in scripts/oxide-selector.sh.
#
# Oxide resolves its default pool from $HOME. A harness that runs a selector
# under a temporary HOME must not make Oxide create a cold pool under that HOME
# (3.3-3.8 GB per run on the internal SSD), so the selector asks the real
# home's own `oxide status` for the shared pool and exports OXIDE_POOL.
#
# A stub `oxide` on PATH records its arguments and the OXIDE_POOL it received
# and answers `status` from the fixture, so nothing builds, no real pool is
# touched and the fixture HOME stays empty.
set -u

here=$(cd "$(dirname "$0")/../.." && pwd -P)
selector="$here/scripts/oxide-selector.sh"

fails=0
fail() { echo "FAIL: $*" >&2; fails=$((fails + 1)); }
ok() { echo "ok: $*"; }

fx=$(mktemp -d "${TMPDIR:-/tmp}/effigy-oxide-pool.XXXXXX") || exit 2
fx=$(cd "$fx" && pwd -P)
cleanup() { rm -rf "$fx"; }
trap cleanup EXIT HUP INT TERM

real_home=$( (eval echo "~$(id -un)") 2>/dev/null ) || real_home=
private_home="$fx/home"
pool="$fx/shared-pool"
other_pool="$fx/other-pool"
log="$fx/oxide.log"
mkdir -p "$private_home" "$pool" "$other_pool" "$fx/bin"
: >"$log"

# Stub oxide: `status` reports the fixture pool the way the real command does
# ("pool: <path> (from config file)"), and every other invocation prints the
# OXIDE_POOL it inherited. FAKE_OXIDE_STATUS=empty models a status that reports
# no pool.
cat >"$fx/bin/oxide" <<'STUB'
#!/bin/sh
printf '%s|%s\n' "${1:-none}" "${OXIDE_POOL:-unset}" >>"$FAKE_OXIDE_LOG"
if [ "${1:-}" = "status" ]; then
  [ "${FAKE_OXIDE_STATUS:-ok}" = "ok" ] || exit 0
  printf 'pool: %s (from config file)\n' "$FAKE_OXIDE_POOL"
  printf 'slots: 2 free, 0 busy\n'
  exit 0
fi
printf 'OXIDE_POOL=%s\n' "${OXIDE_POOL:-unset}"
STUB
chmod 0755 "$fx/bin/oxide"

# Run the selector with a controlled environment: no OXIDE_POOL, no escape, no
# hosted-runner override, the stub first on PATH.
run_selector() { # home env-args... -- selector args...
  home=$1
  shift
  env -u OXIDE_POOL -u GITHUB_ACTIONS -u EFFIGY_OXIDE_ESCAPE \
    HOME="$home" FAKE_OXIDE_LOG="$log" FAKE_OXIDE_POOL="$pool" \
    PATH="$fx/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    sh "$selector" "$@"
}

run_with_pool() { # pool home -- selector args...
  value=$1
  home=$2
  shift 2
  env -u GITHUB_ACTIONS -u EFFIGY_OXIDE_ESCAPE \
    OXIDE_POOL="$value" HOME="$home" FAKE_OXIDE_LOG="$log" \
    FAKE_OXIDE_POOL="$pool" PATH="$fx/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    sh "$selector" "$@"
}

assert_pool() { # label output expected summarised-log
  if [ "$2" = "OXIDE_POOL=$3" ]; then
    ok "$1 passes OXIDE_POOL=$3 to Oxide"
  else
    fail "$1: expected OXIDE_POOL=$3, got [$2]"
  fi
}

# 1. A private HOME with OXIDE_POOL unset: the selector looks the real pool up
# under the account's real home and passes it to Oxide.
: >"$log"
out=$(run_selector "$private_home" check -p effigy 2>"$fx/err1") || fail "private HOME run exited $?"
assert_pool "private HOME check" "$out" "$pool"
[ -s "$fx/err1" ] && fail "private HOME run warned: $(cat "$fx/err1")"
expect_line() { # label line-number expected
  actual=$(sed -n "$2p" "$log")
  if [ "$actual" = "$3" ]; then
    ok "$1"
  else
    fail "$1: line $2 was [$actual], expected [$3]"
  fi
}
expect_line "pool lookup runs as 'oxide status' with OXIDE_POOL still unset" 1 "status|unset"
expect_line "the selector passes the resolved pool to its command" 2 "check|$pool"
if [ "$(wc -l <"$log" | tr -d ' ')" = "2" ]; then
  ok "one status lookup and one command"
else
  fail "unexpected oxide calls: $(tr '\n' ';' <"$log")"
fi
if [ -e "$private_home/Library" ] || [ -e "$private_home/.cache" ]; then
  fail "private HOME gained a pool cache"
else
  ok "private HOME stays empty"
fi
if [ -e "$pool/Library" ] || [ -e "$other_pool/Library" ]; then
  fail "fixture wrote a pool cache"
else
  ok "fixture never creates a real pool directory"
fi

# 2. The `exec -- <tool>` route reaches Oxide through the same script, so it
# gets the same pool.
: >"$log"
out=$(run_selector "$private_home" exec -- leased-tool build 2>"$fx/err2") ||
  fail "exec route exited $?"
assert_pool "private HOME exec route" "$out" "$pool"
expect_exec=$(sed -n '2p' "$log")
if [ "$expect_exec" = "exec|$pool" ]; then
  ok "exec route passes the resolved pool"
else
  fail "exec route ran [$expect_exec], expected [exec|$pool]"
fi

# 3. An OXIDE_POOL that the caller already set wins, with no extra lookup.
: >"$log"
out=$(run_with_pool "$other_pool" "$private_home" check -p effigy 2>"$fx/err3") ||
  fail "explicit pool run exited $?"
assert_pool "explicit OXIDE_POOL" "$out" "$other_pool"
if grep -q '^status|' "$log"; then
  fail "explicit OXIDE_POOL still ran a status lookup"
else
  ok "explicit OXIDE_POOL skips the status lookup"
fi

# 4. The account's own HOME needs no lookup: Oxide resolves the pool itself.
if [ -n "$real_home" ] && [ -d "$real_home" ]; then
  : >"$log"
  out=$(run_selector "$real_home" check -p effigy 2>"$fx/err4") ||
    fail "real HOME run exited $?"
  if grep -q '^status|' "$log"; then
    fail "real HOME run still resolved a pool: $(tr '\n' ';' <"$log")"
  else
    ok "real HOME run leaves pool resolution to Oxide"
  fi
  assert_pool "real HOME check" "$out" "unset"
else
  echo "skip: no real home to compare against" >&2
fi

# 5. A status lookup that reports no pool warns once, names OXIDE_POOL and
# still runs the command.
: >"$log"
export FAKE_OXIDE_STATUS=empty
out=$(run_selector "$private_home" check -p effigy 2>"$fx/err5") ||
  fail "unresolved-pool run exited $?"
unset FAKE_OXIDE_STATUS
assert_pool "unresolved pool" "$out" "unset"
warnings=$(grep -c 'OXIDE_POOL' "$fx/err5" || true)
if [ "$warnings" = "1" ]; then
  ok "unresolved pool warns once naming OXIDE_POOL"
else
  fail "expected one OXIDE_POOL warning, got [$warnings]: $(cat "$fx/err5")"
fi
if grep -q '^check|' "$log"; then
  ok "unresolved pool still runs the command"
else
  fail "unresolved pool never ran the command"
fi

if [ "$fails" -ne 0 ]; then
  echo "$fails check(s) failed" >&2
  exit 1
fi
echo "all oxide-selector pool checks passed"
