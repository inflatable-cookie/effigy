# g08.008 - Graph-Aware Scan Closeout

Status: Complete
Depends on: `g08.007`

## Goal

Close `g08` with proof that graph-aware scans add real value while preserving
the original scan contract.

## Scope

- rerun focused scan and graph test suites
- run docs checks for new command examples and JSON examples
- record performance and noise tradeoffs
- record which findings are strict enough for future gates and which remain
  advisory
- update roadmap/spec front doors
- leave no active ready card unless a concrete follow-up lane is justified

## Closeout Questions

- Do existing scans still work without an index?
- Are graph-backed scans useful when the index is ready?
- Are stale/missing graph states reported clearly?
- Are findings explainable from concrete graph evidence?
- Did any command become too slow for normal agent use?
- Is the guidance balanced enough that agents will not run scans ritualistically?

## Guardrails

- no broad graph rewrite during closeout
- no release mutation
- no `.github/workflows/` edits
- no marketing-style performance claims

## Acceptance Criteria

- focused tests pass
- docs checks pass
- JSON examples remain valid
- closeout log records wins, limits, and residual debt
- `g08` front doors show accurate next state

## Next Task

No active ready card until closeout finishes.
