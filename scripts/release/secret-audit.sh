#!/bin/sh
# Release secret-history audit (lead 68110986; contract 054 §4.4 repo task).
#
# Scans the tracked tree at HEAD and the commits since the release base tag
# for credential-shaped text. It reports rule ids, paths and commit ids only,
# never the matched text. A complete result is cached under its commit key, so
# an unchanged HEAD repeats no git scan. Every refusal names its next action.
#
# Exit codes:
#   0    clean
#   1    findings (fail closed; the report names rule, path and commit)
#   2    refused or incomplete: no release tag, git failed, bad arguments
#   124  the time budget ran out; no result is recorded
#   130  cancelled by a signal; the running git process is killed
set -eu
export LC_ALL=C

SCHEMA=effigy.release.secret-audit.v1
VERSION=2
BUDGET=300
BASE=""
EXPECTED_BASE=""
CACHE_DIR=""
GIT=${EFFIGY_SECRET_AUDIT_GIT:-git}
ALLOWLIST=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/secret-audit-allowlist.tsv

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/release/secret-audit.sh [--base TAG] [--base-sha SHA] [--budget SECONDS] [--cache-dir DIR]

  --base TAG      release tag that bounds history (default: nearest v* tag reachable from HEAD)
  --base-sha SHA  expected commit of the release tag
  --budget N      total seconds for the whole audit (default 300)
  --cache-dir DIR where complete results are cached (default .effigy/release/secret-audit)
USAGE
}

fail() {
  printf '[error] %s\n' "$*" >&2
  exit 2
}

# The rules are regular expressions git grep and git log -G both accept.
rule_ids() {
  printf '%s\n' aws-access-key private-key github-token assigned-secret
}

rule_pattern() {
  case $1 in
    aws-access-key) printf '%s' 'AKIA[0-9A-Z]{16}' ;;
    private-key) printf '%s' '-----BEGIN ([A-Z]+ )?PRIVATE KEY-----' ;;
    github-token) printf '%s' 'gh[pousr]_[A-Za-z0-9]{36,}' ;;
    assigned-secret) printf '%s' "(secret|password|passwd|token|api[_-]?key)[A-Za-z0-9_]*[[:space:]]*[:=][[:space:]]*[\"'][^\"'[:space:]]{16,}[\"']" ;;
  esac
}

digest_stdin() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | cut -d ' ' -f 1
  else
    shasum -a 256 | cut -d ' ' -f 1
  fi
}

now() {
  date +%s
}

progress() {
  printf '[progress] %s (elapsed %ss of %ss)\n' "$1" "$(($(now) - START))" "$BUDGET" >&2
}

cleanup() {
  if [ -n "${TMP:-}" ]; then
    rm -rf "$TMP"
  fi
}

# A signal kills the running git process (our own child) and exits 130; the
# EXIT trap removes the scratch directory. Nothing is cached.
cancelled() {
  if [ -n "${CHILD:-}" ]; then
    kill "$CHILD" 2>/dev/null || true
  fi
  printf '[cancelled] secret audit stopped by signal; no result recorded\n' >&2
  exit 130
}

# bounded OUT ARGV...: runs ARGV with stdout in OUT under the time left in the
# budget. perl's alarm kills the process at the deadline, and exec keeps its pid,
# so CHILD is the git process itself. Returns 124 on timeout.
bounded() {
  bounded_out=$1
  shift
  left=$((DEADLINE - $(now)))
  if [ "$left" -le 0 ]; then
    return 124
  fi
  perl -e 'alarm shift; exec @ARGV or die "exec: $!\n"' "$left" "$@" >"$bounded_out" 2>>"$TMP/git.log" &
  CHILD=$!
  set +e
  wait "$CHILD"
  bounded_status=$?
  set -e
  CHILD=""
  if [ "$bounded_status" -eq 142 ]; then
    return 124
  fi
  return "$bounded_status"
}

# Maps a bounded result to the audit's own outcome. Both refusals stop the run
# with no cache entry, so a partial scan can never read as clean.
check_bounded() {
  check_label=$1
  check_status=$2
  case $check_status in
    0|1) return 0 ;;
    124)
      printf '[timeout] %s exceeded the %ss budget; no result recorded. Rerun with --budget N, or narrow --base.\n' "$check_label" "$BUDGET" >&2
      exit 124
      ;;
    *)
      printf '[incomplete] git failed during %s (status %s); no result recorded. See the git output in the scratch log for this run.\n' "$check_label" "$check_status" >&2
      exit 2
      ;;
  esac
}

ROOT=""
HEAD_SHA=""
BASE_SHA=""
CHILD=""
TMP=""
START=$(now)
DEADLINE=0

while [ $# -gt 0 ]; do
  case "$1" in
    --base) BASE=${2:?--base needs a tag}; shift 2 ;;
    --base-sha) EXPECTED_BASE=${2:?--base-sha needs a commit}; shift 2 ;;
    --budget)
      BUDGET=${2:?--budget needs seconds}
      case "$BUDGET" in
        ''|*[!0-9]*) usage; fail "--budget needs a whole number of seconds" ;;
      esac
      shift 2
      ;;
    --cache-dir) CACHE_DIR=${2:?--cache-dir needs a directory}; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage; fail "unknown argument: $1" ;;
  esac
done

command -v perl >/dev/null 2>&1 || fail "perl is required to bound git work; it ships with macOS and Linux"
ROOT=$("$GIT" rev-parse --show-toplevel 2>/dev/null) || fail "not inside a git work tree"
HEAD_SHA=$("$GIT" -C "$ROOT" rev-parse --verify --quiet 'HEAD^{commit}') || fail "HEAD has no commit"
[ "$("$GIT" -C "$ROOT" rev-parse --is-shallow-repository)" = false ] || fail "checkout is shallow; fetch complete history before auditing."
[ -z "$EXPECTED_BASE" ] || [ -n "$BASE" ] || fail "--base-sha requires --base TAG."

if [ -z "$BASE" ]; then
  BASE=$("$GIT" -C "$ROOT" describe --tags --abbrev=0 --match 'v[0-9]*' HEAD 2>/dev/null) ||
    fail "no v* release tag is reachable from HEAD. Create the release tag, or pass --base TAG for the tag that bounds the history."
fi
BASE_SHA=$("$GIT" -C "$ROOT" rev-parse --verify --quiet "refs/tags/$BASE^{commit}") ||
  fail "release tag not found: $BASE. Fetch the tags, or pass --base with a tag that exists."
[ -z "$EXPECTED_BASE" ] || [ "$BASE_SHA" = "$EXPECTED_BASE" ] || fail "release tag $BASE resolves to $BASE_SHA, not expected commit $EXPECTED_BASE."
"$GIT" -C "$ROOT" merge-base --is-ancestor "$BASE_SHA" "$HEAD_SHA" ||
  fail "release tag $BASE is not an ancestor of HEAD; the history since it is undefined. Pass --base with a tag on this line of history."

if [ -z "$CACHE_DIR" ]; then
  CACHE_DIR="$ROOT/.effigy/release/secret-audit"
fi

rules_digest=$(for rule in $(rule_ids); do rule_pattern "$rule"; printf '\n'; done | digest_stdin)
[ -f "$ALLOWLIST" ] || fail "fixture allowlist is missing: $ALLOWLIST"
allowlist_digest=$(digest_stdin < "$ALLOWLIST")
key=$(printf '%s\n' "$SCHEMA" "$VERSION" "$HEAD_SHA" "$BASE" "$BASE_SHA" "$rules_digest" "$allowlist_digest" | digest_stdin)
cached="$CACHE_DIR/$key.report"

if [ -f "$cached" ]; then
  printf '[cached] result for HEAD %s since %s (%s)\n' "$HEAD_SHA" "$BASE" "$key" >&2
  cat "$cached"
  if grep -q '^status=clean$' "$cached"; then
    exit 0
  fi
  exit 1
fi

TMP=$(mktemp -d "${TMPDIR:-/tmp}/effigy-secret-audit.XXXXXX")
trap cleanup EXIT
trap cancelled INT TERM
: > "$TMP/git.log"
: > "$TMP/findings"
DEADLINE=$(($(now) + BUDGET))

# A match is exempt only when the complete blob is listed for this exact path.
# 0 = exact exemption, 1 = match not exempt, 2 = no match. Git errors and
# deadlines stop the audit instead of turning an incomplete check into a pass.
check_blob() {
  blob_rev=$1 blob_path=$2 blob_rule=$3
  blob_status=0
  bounded "$TMP/blob-match" "$GIT" -C "$ROOT" grep -q -I -i -E -e "$(rule_pattern "$blob_rule")" "$blob_rev" -- ":(literal)$blob_path" || blob_status=$?
  check_bounded "fixture match lookup" "$blob_status"
  [ "$blob_status" -eq 0 ] || return 2
  blob_status=0
  bounded "$TMP/blob" "$GIT" -C "$ROOT" show "$blob_rev:$blob_path" || blob_status=$?
  if [ "$blob_status" -ne 0 ]; then
    check_bounded "fixture blob lookup" "$blob_status"
    fail "could not read a matching blob; no result recorded."
  fi
  blob_digest=$(digest_stdin < "$TMP/blob")
  awk -F '\t' -v path="$blob_path" -v digest="$blob_digest" '$1 == digest && $2 == path { found=1 } END { exit !found }' "$ALLOWLIST"
}

total=$(rule_ids | wc -l | tr -d ' ')
index=0
for rule in $(rule_ids); do
  index=$((index + 1))
  progress "tracked tree at HEAD: rule $index/$total $rule"
  status=0
  bounded "$TMP/tracked" "$GIT" -C "$ROOT" grep -l -I -i -E -e "$(rule_pattern "$rule")" "$HEAD_SHA" -- || status=$?
  check_bounded "tracked tree scan ($rule)" "$status"
  if [ "$status" -eq 0 ]; then
    sed 's/^[^:]*://' "$TMP/tracked" > "$TMP/paths"
    while IFS= read -r path; do
      blob_result=0
      check_blob "$HEAD_SHA" "$path" "$rule" || blob_result=$?
      if [ "$blob_result" -ne 0 ]; then
        printf 'tracked\t%s\t%s\n' "$rule" "$path"
      fi
    done < "$TMP/paths" >> "$TMP/findings"
  fi
done

index=0
: > "$TMP/history-findings"
for rule in $(rule_ids); do
  index=$((index + 1))
  progress "history since $BASE: rule $index/$total $rule"
  status=0
  bounded "$TMP/history" "$GIT" -C "$ROOT" log -m --no-renames -E -i -G "$(rule_pattern "$rule")" --format='@@%H' --name-only "$BASE_SHA..$HEAD_SHA" -- || status=$?
  check_bounded "history scan ($rule)" "$status"
  awk -v rule="$rule" '
    /^@@/ { commit = substr($0, 3); next }
    NF > 0 { printf "history\t%s\t%s\t%s\n", rule, commit, $0 }
  ' "$TMP/history" >> "$TMP/history-findings"
done

# Check every matching side, including each merge parent. A deletion can be
# exempt only if its old blob is exact; a replacement must prove both sides.
tab=$(printf '\t')
while IFS="$tab" read -r kind rule commit path; do
  status=0
  bounded "$TMP/parents" "$GIT" -C "$ROOT" rev-list --parents -n 1 "$commit" || status=$?
  check_bounded "history parent lookup" "$status"
  [ "$status" -eq 0 ] || fail "could not read history parents; no result recorded."
  exempt=true
  matched=false
  for revision in $(cat "$TMP/parents"); do
    blob_result=0
    check_blob "$revision" "$path" "$rule" || blob_result=$?
    case $blob_result in
      0) matched=true ;;
      1) matched=true; exempt=false ;;
      2) ;;
      *) fail "fixture check failed; no result recorded." ;;
    esac
  done
  if [ "$exempt" != true ] || [ "$matched" != true ]; then
    printf '%s\t%s\t%s\t%s\n' "$kind" "$rule" "$commit" "$path" >> "$TMP/findings"
  fi
done < "$TMP/history-findings"

if [ -s "$TMP/findings" ]; then
  result=findings
else
  result=clean
fi

mkdir -p "$CACHE_DIR"
report="$TMP/report"
{
  printf 'schema=%s\n' "$SCHEMA"
  printf 'status=%s\n' "$result"
  printf 'head=%s\n' "$HEAD_SHA"
  printf 'base=%s %s\n' "$BASE" "$BASE_SHA"
  printf 'rules=%s\n' "$rules_digest"
  printf 'allowlist=%s\n' "$allowlist_digest"
  if [ "$result" = findings ]; then
    awk -F '\t' '{ printf "finding\t%s\n", $0 }' "$TMP/findings"
  fi
} > "$report"

# Atomic publish: a reader never sees a half-written cache entry.
cp "$report" "$CACHE_DIR/.$key.tmp"
mv "$CACHE_DIR/.$key.tmp" "$cached"

cat "$report"
if [ "$result" = findings ]; then
  printf '[error] secret audit found %s match(es) since %s. Values are not printed: inspect each path and commit, rotate any real credential, then rewrite or revoke as the release owner decides.\n' "$(wc -l < "$TMP/findings" | tr -d ' ')" "$BASE" >&2
  exit 1
fi
printf '[ok] secret audit clean: HEAD tracked tree and history since %s\n' "$BASE"
