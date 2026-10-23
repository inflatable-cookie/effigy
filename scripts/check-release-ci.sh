#!/bin/sh

# The release's hosted CI gate: exactly one `workflow_dispatch` run of
# `ci.yml`, for this exact commit.
#
# Never the newest run on the ref: a sibling dispatch — another branch's
# promotion, an operator's re-dispatch, a re-run — is a different run, and
# accepting it would certify a workflow this release did not start. Zero or
# several matches refuse and name the candidates, so the operator dispatches
# once and re-runs the gate.

set -eu

candidate_sha="$(git rev-parse HEAD)"
# One line per run: "<databaseId> <headSha> <status> <conclusion>-or-dash <url>".
# `--commit` and `--event` already narrow the query; the gate re-checks the SHA.
runs="$(
  gh run list \
    --workflow ci.yml \
    --branch main \
    --commit "$candidate_sha" \
    --event workflow_dispatch \
    --limit 20 \
    --json databaseId,headSha,status,conclusion,url \
    --jq '.[] | "\(.databaseId) \(.headSha) \(.status) \(.conclusion // "-") \(.url)"'
)"

if [ -z "$runs" ]; then
  echo "release blocked: no workflow_dispatch run of ci.yml exists for candidate commit $candidate_sha" >&2
  echo "dispatch ci.yml on main for this exact commit, wait for success, then retry" >&2
  exit 1
fi

count="$(printf '%s\n' "$runs" | wc -l | tr -d ' ')"
if [ "$count" -ne 1 ]; then
  echo "release blocked: $count workflow_dispatch runs of ci.yml exist for candidate commit $candidate_sha, so this release cannot tell which run it started" >&2
  echo "candidate runs:" >&2
  printf '%s\n' "$runs" | while read -r run_id run_sha run_status run_conclusion run_url; do
    printf '  run #%s %s (%s/%s)\n' "$run_id" "$run_url" "$run_status" "$run_conclusion" >&2
  done
  echo "wait for the other dispatch to finish, or re-dispatch once and retry when only this release's run is live" >&2
  exit 1
fi

# The fields carry no spaces, so word splitting reads them back.
# shellcheck disable=SC2086
set -- $runs
run_id="$1"
run_sha="$2"
run_status="$3"
run_conclusion="$4"
run_url="$5"

if [ "$run_sha" != "$candidate_sha" ]; then
  echo "release blocked: hosted CI run #$run_id is for commit $run_sha, not candidate commit $candidate_sha" >&2
  exit 1
fi
if [ "$run_status" != "completed" ]; then
  echo "release blocked: hosted CI run #$run_id for candidate commit $candidate_sha is $run_status, not completed" >&2
  echo "wait for $run_url, then retry" >&2
  exit 1
fi
if [ "$run_conclusion" != "success" ]; then
  echo "release blocked: hosted CI run #$run_id for candidate commit $candidate_sha concluded $run_conclusion" >&2
  exit 1
fi

echo "CI is green for candidate commit $candidate_sha (run #$run_id $run_url)"
