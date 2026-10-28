#!/usr/bin/env bash
# Fixture proofs for scripts/release/secret-audit.sh. Every repository is built
# inside one mktemp directory; no fixed path and no real checkout is written.
set -euo pipefail
export LC_ALL=C

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
scanner="$repo_root/scripts/release/secret-audit.sh"
tmp_root=$(mktemp -d "${TMPDIR:-/tmp}/effigy-secret-audit-test.XXXXXX")
trap 'rm -rf "$tmp_root"' EXIT

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

# Credential-shaped values are assembled at run time, so the repository never
# tracks a literal that its own scanner would flag.
secret_value=$(printf 'AKIA%s' 'IOSFODNN7FIXTURE')

git_fx() {
  git -c user.name=fixture -c user.email=fixture@example.invalid -c commit.gpgsign=false "$@"
}

new_repo() {
  local dir="$tmp_root/$1"
  mkdir -p "$dir"
  git_fx -C "$dir" init -q -b main
  printf '%s\n' "$dir"
}

commit_all() {
  git_fx -C "$1" add -A
  git_fx -C "$1" commit -q --allow-empty -m "$2"
}

write_file() {
  mkdir -p "$(dirname "$1")"
  printf '%s\n' "$2" > "$1"
}

# run_scan DIR NAME [ARGS...]: runs the scanner from DIR, capturing stdout,
# stderr and the exit status into $tmp_root/NAME.{out,err,status}.
run_scan() {
  local dir=$1 name=$2
  shift 2
  set +e
  (cd "$dir" && sh "$scanner" "$@") > "$tmp_root/$name.out" 2> "$tmp_root/$name.err"
  last_status=$?
  set -e
}

expect_status() {
  local name=$1 want=$2
  if [ "$last_status" != "$want" ]; then
    cat "$tmp_root/$name.out" "$tmp_root/$name.err" >&2 || true
    fail "$name exited $last_status, expected $want"
  fi
}

expect_contains() {
  local file=$1 needle=$2
  grep -Fq -- "$needle" "$file" || {
    cat "$file" >&2
    fail "expected $file to contain: $needle"
  }
}

# Redaction: the matched value must appear nowhere the scanner writes.
expect_redacted() {
  local name=$1 cache=$2
  if grep -RFq -- "$secret_value" "$tmp_root/$name.out" "$tmp_root/$name.err" "$cache" 2>/dev/null; then
    fail "$name leaked the matched value into output or cache"
  fi
}

cache_count() {
  if [ -d "$1" ]; then
    find "$1" -maxdepth 1 -name '*.report' | wc -l | tr -d ' '
  else
    printf '0\n'
  fi
}

# Clean repository: release tag, then benign commits.
clean="$(new_repo clean)"
write_file "$clean/README.md" 'release notes'
commit_all "$clean" 'release'
git_fx -C "$clean" tag v0.1.0
write_file "$clean/src/lib.rs" 'pub fn answer() -> u8 { 42 }'
commit_all "$clean" 'benign change'
clean_cache="$tmp_root/clean-cache"

run_scan "$clean" clean --budget 60 --cache-dir "$clean_cache"
expect_status clean 0
expect_contains "$tmp_root/clean.out" 'status=clean'
expect_contains "$tmp_root/clean.out" 'base=v0.1.0 '
expect_contains "$tmp_root/clean.err" '[progress] tracked tree at HEAD: rule 1/4'
expect_contains "$tmp_root/clean.out" '[ok] secret audit clean'

# Tracked finding at HEAD: the file still exists, so the tree scan reports it.
tracked="$(new_repo tracked)"
write_file "$tracked/README.md" 'release notes'
commit_all "$tracked" 'release'
git_fx -C "$tracked" tag v0.1.0
write_file "$tracked/config/app.env" "AWS_ACCESS_KEY_ID=$secret_value"
commit_all "$tracked" 'add config'
tracked_cache="$tmp_root/tracked-cache"

run_scan "$tracked" tracked --budget 60 --cache-dir "$tracked_cache"
expect_status tracked 1
expect_contains "$tmp_root/tracked.out" 'status=findings'
expect_contains "$tmp_root/tracked.out" $'finding\ttracked\taws-access-key\tconfig/app.env'
expect_contains "$tmp_root/tracked.err" 'Values are not printed'
expect_redacted tracked "$tracked_cache"

# Cached repeat: the same HEAD answers from its report, with no git progress.
run_scan "$tracked" tracked-repeat --budget 60 --cache-dir "$tracked_cache"
expect_status tracked-repeat 1
expect_contains "$tmp_root/tracked-repeat.err" '[cached] result for HEAD'
if grep -Fq '[progress]' "$tmp_root/tracked-repeat.err"; then
  fail "a cached repeat must not run the git scan again"
fi
[ "$(cache_count "$tracked_cache")" = 1 ] || fail "a repeat must keep one cache entry"
expect_redacted tracked-repeat "$tracked_cache"

# History-only finding: the secret is removed at HEAD, so only the history since
# the tag can show it. Removing it also moves HEAD, which is a changed commit.
history="$(new_repo history)"
write_file "$history/README.md" 'release notes'
commit_all "$history" 'release'
git_fx -C "$history" tag v0.1.0
write_file "$history/config/app.env" "AWS_ACCESS_KEY_ID=$secret_value"
commit_all "$history" 'leak'
git_fx -C "$history" rm -q config/app.env
commit_all "$history" 'remove leak'
history_cache="$tmp_root/history-cache"

run_scan "$history" history --base v0.1.0 --budget 60 --cache-dir "$history_cache"
expect_status history 1
if grep -Fq $'finding\ttracked' "$tmp_root/history.out"; then
  fail "history fixture must be clean at HEAD"
fi
grep -Eq $'^finding\thistory\taws-access-key\t[0-9a-f]{40}\tconfig/app.env$' "$tmp_root/history.out" ||
  fail "history finding must name the rule, commit and path"
expect_redacted history "$history_cache"

# Changed commit: a new HEAD gets a new key and a fresh scan.
before=$(cache_count "$history_cache")
write_file "$history/README.md" 'more notes'
commit_all "$history" 'docs'
run_scan "$history" history-moved --base v0.1.0 --budget 60 --cache-dir "$history_cache"
expect_status history-moved 1
if grep -Fq '[cached]' "$tmp_root/history-moved.err"; then
  fail "a changed commit must not reuse the previous HEAD's cache"
fi
[ "$(cache_count "$history_cache")" -gt "$before" ] || fail "a changed commit must write a new cache entry"

# Absent release tag: refuses and names the next action.
untagged="$(new_repo untagged)"
write_file "$untagged/README.md" 'no release yet'
commit_all "$untagged" 'first'
run_scan "$untagged" untagged --budget 60 --cache-dir "$tmp_root/untagged-cache"
expect_status untagged 2
expect_contains "$tmp_root/untagged.err" 'no v* release tag is reachable from HEAD'
expect_contains "$tmp_root/untagged.err" '--base TAG'
[ ! -d "$tmp_root/untagged-cache" ] || fail "a refused audit must not write a cache"

run_scan "$clean" missing-tag --base v9.9.9 --budget 60 --cache-dir "$clean_cache"
expect_status missing-tag 2
expect_contains "$tmp_root/missing-tag.err" 'release tag not found: v9.9.9'

run_scan "$clean" bad-budget --budget soon --cache-dir "$clean_cache"
expect_status bad-budget 2

# Timeout: a git stand-in that never finishes its grep or log. The audit must
# stop at its budget, name the timeout, and cache nothing.
bin_dir="$tmp_root/bin"
mkdir -p "$bin_dir"
slow_git="$bin_dir/slow-git"
cat > "$slow_git" <<SLOW
#!/bin/sh
case " \$* " in
  *" grep "*|*" log "*)
    echo \$\$ > "$tmp_root/slow-git.pid"
    exec sleep 30
    ;;
esac
exec git "\$@"
SLOW
chmod +x "$slow_git"

timed="$tmp_root/timed-cache"
set +e
(cd "$tracked" && EFFIGY_SECRET_AUDIT_GIT="$slow_git" sh "$scanner" --budget 2 --cache-dir "$timed") \
  > "$tmp_root/timed.out" 2> "$tmp_root/timed.err"
timed_status=$?
set -e
[ "$timed_status" = 124 ] || fail "timed-out audit exited $timed_status, expected 124"
expect_contains "$tmp_root/timed.err" '[timeout]'
[ "$(cache_count "$timed")" = 0 ] || fail "a timed-out audit must not cache a result"

# Cancellation: a TERM to the audit stops the running git stand-in and exits
# 130 with nothing cached. The stand-in's pid comes from its own marker file.
cancel_cache="$tmp_root/cancel-cache"
rm -f "$tmp_root/slow-git.pid"
(cd "$tracked" && EFFIGY_SECRET_AUDIT_GIT="$slow_git" exec sh "$scanner" --budget 60 --cache-dir "$cancel_cache") \
  > "$tmp_root/cancel.out" 2> "$tmp_root/cancel.err" &
scanner_pid=$!
for _ in $(seq 1 100); do
  [ -s "$tmp_root/slow-git.pid" ] && break
  sleep 0.1
done
[ -s "$tmp_root/slow-git.pid" ] || fail "the git stand-in never started"
stand_in_pid=$(cat "$tmp_root/slow-git.pid")
kill -TERM "$scanner_pid"
cancel_status=0
wait "$scanner_pid" || cancel_status=$?
[ "$cancel_status" = 130 ] || fail "cancelled audit exited $cancel_status, expected 130"
expect_contains "$tmp_root/cancel.err" '[cancelled]'
for _ in $(seq 1 50); do
  kill -0 "$stand_in_pid" 2>/dev/null || break
  sleep 0.1
done
if kill -0 "$stand_in_pid" 2>/dev/null; then
  fail "the git stand-in outlived the cancelled audit"
fi
[ "$(cache_count "$cancel_cache")" = 0 ] || fail "a cancelled audit must not cache a result"

printf 'release secret-audit fixtures: ok\n'
