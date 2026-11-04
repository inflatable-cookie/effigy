#!/bin/sh
# Archive this checkout's kernel run logs before worktree teardown removes it.
#
# QA checkouts vanish with their `.effigy/state/runs/` child journals, while
# the bounded human fold omits nextest assertion bodies, so exact failure
# bytes would be unrecoverable. This helper copies the available original
# segment files into a fresh private directory under the scratch volume and
# writes a small manifest binding each archived copy to its checkout-relative
# source path, byte count and SHA-256. It prints only the artifact location,
# the manifest hash and the status; never log contents.
#
# Scope is ONLY the resolved checkout's `.effigy/state/runs/`, exactly two
# levels deep (`<run>/<segment>`), regular files only. Symlinks, nonregular
# files and anything outside that tree are refused and recorded, never
# followed; there is no broad tree walk. A missing runs tree is a no-op; a
# symlinked state component or an unusable scratch base is reported and
# fails (exit 1) without archiving, so the hook still runs cleanup and then
# reports the failure. Nothing here deletes, rebuilds, contacts a backend, or touches a
# foreign checkout, HOME state or VM contents. Archives are retained; this
# helper never deletes an archive it or anyone else made.
#
#   worktree-archive-run-logs.sh [checkout-root [archive-base]]
#
# `archive-base` defaults to /Volumes/Builds/scratch/effigy, which is private
# host scratch OUTSIDE Queue run cleanup so the archive survives checkout
# removal; the run's TMPDIR must never be the destination. The
# EFFIGY_RUN_LOG_ARCHIVE_BASE environment override exists for deterministic
# fixtures only: it redirects the destination base and never disables
# archiving. Bounds are fixed: at most 200 files, 1 MiB per file, 16 MiB
# total, so the copy stays inside the existing teardown limits; no timeout,
# log limit or QA budget is changed.
set -u

DEFAULT_BASE="/Volumes/Builds/scratch/effigy"
MAX_FILES=200
MAX_PER_FILE=1048576
MAX_TOTAL=16777216

here=$(cd "$(dirname "$0")" && pwd -P)
root=${1:-$here/..}
base=${2:-${EFFIGY_RUN_LOG_ARCHIVE_BASE:-$DEFAULT_BASE}}

say() { echo "worktree-archive-run-logs: $*"; }
fail() { echo "worktree-archive-run-logs: error: $*" >&2; exit 1; }

root=$(cd "$root" && pwd -P) || fail "cannot resolve checkout root $root"
runs="$root/.effigy/state/runs"
[ -d "$runs" ] || { say "status=no-state runs=$runs"; exit 0; }
for component in "$root/.effigy" "$root/.effigy/state" "$runs"; do
  if [ -L "$component" ]; then
    say "status=refused reason=symlink-component path=$component"
    exit 1
  fi
done

mkdir -p "$base" 2>/dev/null
[ -d "$base" ] || { say "status=unavailable reason=no-archive-base base=$base"; exit 1; }
dest=$(mktemp -d "$base/qa-run-logs.XXXXXX") || fail "cannot create archive dir under $base"
chmod 0700 "$dest" || fail "cannot secure $dest"

if command -v sha256sum >/dev/null 2>&1; then
  sum="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  sum="shasum -a 256"
else
  fail "no SHA-256 utility (need sha256sum or shasum)"
fi

# Byte count without reading the whole file: a multi-gigabyte log must not be
# streamed just to learn it exceeds the per-file cap.
fsize() {
  if out=$(stat -f%z "$1" 2>/dev/null); then
    printf '%s' "$out"
  elif out=$(stat -c%s "$1" 2>/dev/null); then
    printf '%s' "$out"
  else
    return 1
  fi
}

manifest="$dest/manifest.tsv"
created=$(date -u +%Y-%m-%dT%H:%M:%SZ)
{
  printf '# checkout\t%s\n' "$root"
  printf '# created\t%s\n' "$created"
  printf '# bounds\tfiles=%s per-file-bytes=%s total-bytes=%s\n' "$MAX_FILES" "$MAX_PER_FILE" "$MAX_TOTAL"
  printf '# columns\tstatus\tsource\tbytes\tsha256\tnote\n'
} >"$manifest" || fail "cannot write $manifest"
record() { printf '%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$5" >>"$manifest" || fail "cannot append $manifest"; }

files=0
bytes=0
refused=0
capped=0
changed=0
unreadable=0
stopped=""

for rundir in "$runs"/*; do
  [ -n "$stopped" ] && break
  if [ -L "$rundir" ]; then
    record refused "$(basename "$rundir")" 0 "-" "symlink-run-dir"
    refused=$((refused + 1))
    continue
  fi
  [ -e "$rundir" ] || continue
  if [ ! -d "$rundir" ]; then
    record refused "$(basename "$rundir")" 0 "-" "nonregular-run-entry"
    refused=$((refused + 1))
    continue
  fi
  id=$(basename "$rundir")
  for src in "$rundir"/*; do
    [ -n "$stopped" ] && break
    if [ -L "$src" ]; then
      record refused "$id/$(basename "$src")" 0 "-" "symlink"
      refused=$((refused + 1))
      continue
    fi
    [ -e "$src" ] || continue
    name=$(basename "$src")
    if [ ! -f "$src" ]; then
      record refused "$id/$name" 0 "-" "nonregular"
      refused=$((refused + 1))
      continue
    fi
    if [ "$files" -ge "$MAX_FILES" ]; then
      record capped "$id/$name" 0 "-" "file-limit-reached"
      capped=$((capped + 1))
      stopped="file-limit"
      break
    fi
    if ! pre=$(fsize "$src"); then
      record unreadable "$id/$name" 0 "-" "stat-failed"
      unreadable=$((unreadable + 1))
      continue
    fi
    want=$pre
    [ "$want" -gt "$MAX_PER_FILE" ] && want=$MAX_PER_FILE
    if [ $((bytes + want)) -gt "$MAX_TOTAL" ]; then
      record capped "$id/$name" 0 "-" "total-limit-reached"
      capped=$((capped + 1))
      stopped="total-limit"
      break
    fi
    out="$dest/$id.$name"
    if ! head -c "$want" <"$src" >"$out" 2>/dev/null; then
      rm -f "$out"
      if [ -r "$src" ]; then
        fail "cannot write archive copy of $id/$name"
      fi
      record unreadable "$id/$name" 0 "-" "read-failed"
      unreadable=$((unreadable + 1))
      continue
    fi
    if ! got=$(fsize "$out"); then
      rm -f "$out"
      fail "cannot stat archive copy of $id/$name"
    fi
    if ! post=$(fsize "$src"); then
      post="unknown"
    fi
    if [ "$post" != "$pre" ]; then
      record changed "$id/$name" "$got" "$($sum "$out" | cut -d ' ' -f 1)" "source $pre->$post during copy"
      changed=$((changed + 1))
    elif [ "$pre" -gt "$want" ]; then
      record capped "$id/$name" "$got" "$($sum "$out" | cut -d ' ' -f 1)" "per-file-limit $want of $pre"
      capped=$((capped + 1))
    else
      record ok "$id/$name" "$got" "$($sum "$out" | cut -d ' ' -f 1)" "-"
    fi
    files=$((files + 1))
    bytes=$((bytes + got))
  done
done

overall=ok
[ $((refused + capped + changed + unreadable)) -gt 0 ] && overall=partial
man_sum=$($sum "$manifest" | cut -d ' ' -f 1)
line="status=$overall archive=$dest manifest=$man_sum files=$files bytes=$bytes refused=$refused capped=$capped changed=$changed unreadable=$unreadable"
[ -n "$stopped" ] && line="$line stopped=$stopped"
say "$line"
