#!/bin/sh

set -eu

candidate_sha="$(git rev-parse HEAD)"
ci_sha="$(
  gh run list \
    --workflow ci.yml \
    --branch main \
    --commit "$candidate_sha" \
    --event workflow_dispatch \
    --status success \
    --limit 1 \
    --json headSha \
    --jq '.[0].headSha // ""'
)"

if [ "$ci_sha" != "$candidate_sha" ]; then
  echo "release blocked: CI is not green for candidate commit $candidate_sha" >&2
  echo "dispatch ci.yml on main for this exact commit, wait for success, then retry" >&2
  exit 1
fi

echo "CI is green for candidate commit $candidate_sha"
