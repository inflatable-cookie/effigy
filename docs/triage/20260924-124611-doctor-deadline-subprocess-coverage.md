# Doctor deadline coverage for subprocess work

Raised: 2026-09-24. Release-readiness audit requested by the operator.
Status: open; not execution authority.
Owner: Effigy Chatterbox for planning; doctor and scan maintainers for repair.

## Evidence

Architecture `029` and contract `047` define one overall 10-second structural
or 120-second deep deadline, with visible non-zero partial failure and process
cleanup. Current structural doctor calls `run_dependency_health_check` after
deadline-aware registered checks. That dependency path can run Cargo metadata
through an unbounded subprocess `.output()`. Deep inventory calls
`git_identities` before its budget-checked walk; its `git ls-files` and
`git status` subprocesses also have no deadline. A check after either call
cannot stop an already-stalled process.

## Known direction and open checks

- Preserve structural/deep membership, catalog scope, exact cache trust, and
  the current report schema while carrying the remaining budget into these
  subprocess paths.
- Prove a deliberately blocked Cargo metadata or Git child exits within the
  selected overall deadline, is reaped, reports the phase, and leaves prior
  cache state intact. Check both default and explicit deep modes.
- Inspect any other blocking operation before declaring coverage complete.

## Promotion condition

Confirm the call graph and bounded cancellation design, then obtain operator
confirmation for a dedicated implementation task under contract `047`.

## Next check

Review the evidence with the operator and decide whether this is part of the
pre-release repair frontier.
