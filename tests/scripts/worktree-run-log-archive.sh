#!/bin/sh
# Fixture tests for scripts/worktree-archive-run-logs.sh. Everything lives
# under a fresh mktemp root: a fake checkout, a fake archive base, and files
# outside the runs tree that must never be followed. No real checkout, HOME
# state, container, backend or fixed /tmp path is touched.
set -u

here=$(cd "$(dirname "$0")/../.." && pwd -P)
helper="$here/scripts/worktree-archive-run-logs.sh"
fails=0
fail() { echo "FAIL: $*" >&2; fails=$((fails + 1)); }
ok() { echo "ok: $*"; }

if command -v sha256sum >/dev/null 2>&1; then
  digest() { sha256sum "$1" | cut -d ' ' -f 1; }
else
  digest() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
fi

new_fixture() {
  fx=$(mktemp -d "${TMPDIR:-/tmp}/effigy-run-log-archive.XXXXXX") || exit 2
  fx=$(cd "$fx" && pwd -P)
  repo="$fx/checkout"
  base="$fx/archive-base"
  mkdir -p "$repo" "$base"
  out="$fx/out.log"
}

# 1. Exact bytes, manifest binding, multiple runs/segments; no log contents
# on stdout; archive base subdirectory is created on demand.
new_fixture
mkdir -p "$repo/.effigy/state/runs/7" "$repo/.effigy/state/runs/9"
printf 'hello stdout\n' >"$repo/.effigy/state/runs/7/0.stdout.0.log"
printf 'boom stderr\n' >"$repo/.effigy/state/runs/7/0.stderr.0.log"
printf 'second run\n' >"$repo/.effigy/state/runs/9/3.stdout.128.log"
: >"$repo/.effigy/state/runs/9/4.stderr.0.log"
sh "$helper" "$repo" "$base/nested" >"$out" 2>&1; rc=$?
[ "$rc" -eq 0 ] || fail "copy exit $rc: $(cat "$out")"
grep -q "status=ok" "$out" || fail "expected status=ok: $(cat "$out")"
dest=$(sed -n 's/.*archive=\([^ ]*\).*/\1/p' "$out")
[ -n "$dest" ] && [ -d "$dest" ] || fail "no archive dir: $(cat "$out")"
for rel in 7/0.stdout.0.log 7/0.stderr.0.log 9/3.stdout.128.log 9/4.stderr.0.log; do
  flat=$(printf '%s' "$rel" | tr '/' '.')
  cmp -s "$repo/.effigy/state/runs/$rel" "$dest/$flat" || fail "bytes differ for $rel"
  size=$(wc -c <"$repo/.effigy/state/runs/$rel" | tr -d ' ')
  grep -q "^ok	$rel	$size	$(digest "$dest/$flat")	" "$dest/manifest.tsv" ||
    fail "manifest binding wrong for $rel"
done
grep -q "hello stdout" "$out" && fail "log contents leaked to stdout"
[ "$(stat -f%Lp "$dest" 2>/dev/null || stat -c%a "$dest")" = "700" ] || fail "archive dir not 0700"
man=$(digest "$dest/manifest.tsv")
grep -q "manifest=$man" "$out" || fail "manifest hash not reported"
ok "exact copy, manifest binding, no content leak"
rm -rf "$fx"

# 2. Missing runs tree is a no-op: exit 0, no archive directory created.
new_fixture
sh "$helper" "$repo" "$base" >"$out" 2>&1; rc=$?
[ "$rc" -eq 0 ] || fail "no-state exit $rc"
grep -q "status=no-state" "$out" || fail "expected status=no-state: $(cat "$out")"
[ -z "$(ls -A "$base")" ] || fail "archive created without state"
ok "no-state no-op"
rm -rf "$fx"

# 2b. A symlinked state component is refused with a failure exit, never
# followed into another tree.
new_fixture
mkdir -p "$fx/elsewhere/state/runs/7"
printf 'foreign\n' >"$fx/elsewhere/state/runs/7/0.stdout.0.log"
mkdir -p "$repo/.effigy"
ln -s "$fx/elsewhere/state" "$repo/.effigy/state"
sh "$helper" "$repo" "$base" >"$out" 2>&1; rc=$?
[ "$rc" -ne 0 ] || fail "refused exit $rc"
grep -q "status=refused" "$out" || fail "expected status=refused: $(cat "$out")"
[ -z "$(ls -A "$base")" ] || fail "archive created from a refused tree"
ok "symlink-component refusal fails"
rm -rf "$fx"

# 2c. An unusable archive base is reported with a failure exit.
new_fixture
mkdir -p "$repo/.effigy/state/runs/7"
printf 'bytes\n' >"$repo/.effigy/state/runs/7/0.stdout.0.log"
touch "$fx/filebase"
sh "$helper" "$repo" "$fx/filebase/sub" >"$out" 2>&1; rc=$?
[ "$rc" -ne 0 ] || fail "unavailable exit $rc"
grep -q "status=unavailable" "$out" || fail "expected status=unavailable: $(cat "$out")"
ok "unavailable-base failure"
rm -rf "$fx"

# 3. Symlinks, outside trees and nonregular files are refused, never
# followed; the regular control file is still archived.
new_fixture
mkdir -p "$repo/.effigy/state/runs/7" "$repo/outside"
printf 'regular\n' >"$repo/.effigy/state/runs/7/0.stdout.0.log"
printf 'outside-secret\n' >"$repo/outside/secret.log"
ln -s "$repo/outside/secret.log" "$repo/.effigy/state/runs/7/evil.log"
ln -s "$repo/outside" "$repo/.effigy/state/runs/linked-run"
mkfifo "$repo/.effigy/state/runs/7/pipe.log"
mkdir -p "$repo/.effigy/state/runs/7/nested-dir"
sh "$helper" "$repo" "$base" >"$out" 2>&1; rc=$?
[ "$rc" -eq 0 ] || fail "refusal exit $rc"
grep -q "status=partial" "$out" || fail "expected status=partial: $(cat "$out")"
dest=$(sed -n 's/.*archive=\([^ ]*\).*/\1/p' "$out")
grep -qr "outside-secret" "$dest" && fail "outside bytes entered the archive"
grep -q "^refused	7/evil.log	0	-	symlink$" "$dest/manifest.tsv" || fail "file symlink not recorded"
grep -q "^refused	linked-run	0	-	symlink-run-dir$" "$dest/manifest.tsv" || fail "run symlink not recorded"
grep -q "^refused	7/pipe.log	0	-	nonregular$" "$dest/manifest.tsv" || fail "fifo not recorded"
grep -q "^refused	7/nested-dir	0	-	nonregular$" "$dest/manifest.tsv" || fail "subdir not recorded"
grep -q "^ok	7/0.stdout.0.log	" "$dest/manifest.tsv" || fail "control file not archived"
ok "symlink/escape/nonregular refusal"
rm -rf "$fx"

# 4. A file past the per-file cap is archived as a capped prefix.
new_fixture
mkdir -p "$repo/.effigy/state/runs/7"
head -c 1048676 /dev/zero | tr '\0' 'B' >"$repo/.effigy/state/runs/7/0.stdout.0.log"
sh "$helper" "$repo" "$base" >"$out" 2>&1; rc=$?
[ "$rc" -eq 0 ] || fail "capped exit $rc"
dest=$(sed -n 's/.*archive=\([^ ]*\).*/\1/p' "$out")
[ "$(wc -c <"$dest/7.0.stdout.0.log" | tr -d ' ')" = "1048576" ] || fail "capped bytes wrong"
head -c 1048576 "$repo/.effigy/state/runs/7/0.stdout.0.log" | cmp -s - "$dest/7.0.stdout.0.log" ||
  fail "capped prefix differs"
grep -q "^capped	7/0.stdout.0.log	1048576	.*per-file-limit 1048576 of 1048676$" "$dest/manifest.tsv" ||
  fail "cap not recorded"
ok "per-file cap"
rm -rf "$fx"

# 5. Files past the total cap stop the archive with an honest record.
# Each file is exactly the per-file cap so the total (not per-file) limit
# is what stops the seventeenth file.
new_fixture
mkdir -p "$repo/.effigy/state/runs/7"
for n in 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15 16 17; do
  head -c 1048576 /dev/zero >"$repo/.effigy/state/runs/7/b$n.log"
done
sh "$helper" "$repo" "$base" >"$out" 2>&1; rc=$?
[ "$rc" -eq 0 ] || fail "total-cap exit $rc"
dest=$(sed -n 's/.*archive=\([^ ]*\).*/\1/p' "$out")
grep -q "stopped=total-limit" "$out" || fail "total stop not reported: $(cat "$out")"
grep -q "^ok	7/b16.log	1048576	" "$dest/manifest.tsv" || fail "b16 not archived"
grep -q "^capped	7/b17.log	0	-	total-limit-reached$" "$dest/manifest.tsv" || fail "b17 not capped"
ok "total cap"
rm -rf "$fx"

# 6. An unreadable source file is recorded, not copied; others still land.
new_fixture
mkdir -p "$repo/.effigy/state/runs/7"
printf 'readable\n' >"$repo/.effigy/state/runs/7/0.stdout.0.log"
printf 'locked\n' >"$repo/.effigy/state/runs/7/0.stderr.0.log"
chmod 000 "$repo/.effigy/state/runs/7/0.stderr.0.log"
[ -r "$repo/.effigy/state/runs/7/0.stderr.0.log" ] &&
  fail "fixture requires a non-root user for the unreadable case"
sh "$helper" "$repo" "$base" >"$out" 2>&1; rc=$?
chmod 644 "$repo/.effigy/state/runs/7/0.stderr.0.log"
[ "$rc" -eq 0 ] || fail "unreadable exit $rc"
dest=$(sed -n 's/.*archive=\([^ ]*\).*/\1/p' "$out")
grep -q "^unreadable	7/0.stderr.0.log	0	-	" "$dest/manifest.tsv" || fail "unreadable not recorded"
grep -q "^ok	7/0.stdout.0.log	" "$dest/manifest.tsv" || fail "readable file not archived"
ok "unreadable handling"
rm -rf "$fx"

# 7. A source file that grows during the copy is kept as a snapshot and
# marked changed, not passed off as a clean read.
new_fixture
mkdir -p "$repo/.effigy/state/runs/3"
head -c 1048576 /dev/zero >"$repo/.effigy/state/runs/3/0.stdout.0.log"
( n=0
  while [ ! -e "$fx/stop" ] && [ "$n" -lt 2000000 ]; do
    printf '0123456789abcdef' >>"$repo/.effigy/state/runs/3/0.stdout.0.log"
    n=$((n + 1))
  done ) &
appender=$!
sh "$helper" "$repo" "$base" >"$out" 2>&1; rc=$?
touch "$fx/stop"
wait "$appender"
[ "$rc" -eq 0 ] || fail "changed exit $rc"
dest=$(sed -n 's/.*archive=\([^ ]*\).*/\1/p' "$out")
grep -q "^changed	3/0.stdout.0.log	" "$dest/manifest.tsv" || fail "change not recorded: $(cat "$dest/manifest.tsv")"
ok "changed-source handling"
rm -rf "$fx"

[ "$fails" -eq 0 ] && echo "all run-log archive tests passed" || { echo "$fails failure(s)" >&2; exit 1; }
