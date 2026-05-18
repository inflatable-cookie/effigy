# g07.046 - CodeGraph Parity Follow-Up Suite

Status: Complete
Depends on: `g07.045`

## Goal

Finish the specific follow-up work that still blocks an honest "as good as or
better than CodeGraph" claim for `effigy graph`.

## Why This Exists

`g07.035` materially improved graph usefulness, but the closeout proved two
remaining blockers:

- warm `graph explore` latency on the live Effigy repo is too high
- parity proof still lacks fixture-backed affected-test and cross-language cases

There is no value in reopening the whole parity suite. The remaining work is
bounded and measurable.

## Scope

- reduce warm `graph explore` query latency on the live Effigy repo
- specifically investigate why some implementation-shaped queries now take
  tens or hundreds of seconds
- fix the remaining release-architecture ranking miss if it survives the
  latency work
- add a fixture-backed benchmark runner for the deferred parity cases
- rerun closeout with fresh evidence and either:
  - close parity honestly
  - or defer the exact remaining gap without leaving a vague lane active

## Non-Goals

- no MCP server
- no graph daemon
- no JavaScript runtime dependency
- no external language plugin system
- no new broad graph feature suite unrelated to measured parity gaps

## Ordered Follow-Up Lanes

1. [`047-warm-query-latency-and-release-ranking.md`](./047-warm-query-latency-and-release-ranking.md)
2. [`048-fixture-backed-parity-proof.md`](./048-fixture-backed-parity-proof.md)
3. [`049-codegraph-parity-follow-up-closeout.md`](./049-codegraph-parity-follow-up-closeout.md)

## Acceptance Criteria

- warm-index graph navigation on the live Effigy repo is back inside a credible
  agent-friendly range
- release-architecture ranking is either fixed or explicitly accepted with
  evidence
- deferred fixture-backed parity cases are executable
- the follow-up closeout leaves no stale ready card

## Batch Cards

- [`996-open-codegraph-parity-follow-up-lane.md`](./batch-cards/996-open-codegraph-parity-follow-up-lane.md)
- [`997-reduce-warm-query-latency-and-fix-release-ranking.md`](./batch-cards/997-reduce-warm-query-latency-and-fix-release-ranking.md)
- [`998-add-fixture-backed-parity-runner.md`](./batch-cards/998-add-fixture-backed-parity-runner.md)
- [`999-close-codegraph-parity-follow-up.md`](./batch-cards/999-close-codegraph-parity-follow-up.md)

## Next Task

No active ready card.
