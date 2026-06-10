# g07.048 - Fixture Backed Parity Proof

Status: Complete
Depends on: `g07.047`

## Goal

Turn the deferred parity cases into executable benchmark proof instead of
permanent placeholders.

## Scope

- add a bounded fixture-runner path for parity cases that need temporary repos
- execute the deferred affected-test case
- execute the deferred cross-language PHP case
- record file-read posture, owner quality, and exact residual gaps
- keep the runner local and deterministic

## Guardrails

- no giant checked-in fixture repos
- no opaque shell harness that agents cannot inspect
- no test-only parity case rewriting to make results look better
- no mixing exact-token `rg` cases into graph-navigation claims

## Acceptance Criteria

- the deferred parity cases in the gold query file are executable
- fixture-backed results are recorded in the same evidence style as live-repo
  cases
- any remaining weak case is visible in the closeout

## Evidence

- [`2026-05/18-183615-fixture-backed-parity-proof.md`](../../logs/archive/2026-05/18-183615-fixture-backed-parity-proof.md)

## Next Task

Execute `999`.
