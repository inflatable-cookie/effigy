# Edit Target And Test Packets

Date: 2026-05-20
Card: [`1025`](../../roadmaps/g07/batch-cards/1025-add-edit-target-and-test-packet-proof.md)
Strict lane: [`096`](../../specs/096-graph-agent-adoption-follow-through-strict-lane.md)

## Summary

Extended `graph explore` with bounded edit-target and likely-test projections.

The packet now gives agents:

- `edit_targets`
  - top implementation owner
  - adjacent wiring/config target when graph evidence is strong enough
- `likely_test_files`
- `likely_test_tasks`

This is additive. The lower-level graph commands are unchanged, and the packet
still states plainly that likely tests are bounded candidates rather than
exhaustive proof.

## Vision Target Delta

Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`

Baseline:

- `graph explore` gave primary owners, excerpts, and relations
- agents still had to infer which file to edit first and which tests to run
- split-feature cases were especially awkward because the packet did not
  distinguish implementation from adjacent wiring

Current:

- `graph explore --json` now includes `edit_targets`
- `graph explore --json` now includes `likely_test_files`
- `graph explore --json` now includes `likely_test_tasks`
- text output now surfaces those projections directly too

Remaining:

- the benchmark lane still needs to measure whether these packets reduce
  fallback navigation work in practice
- skill/docs guidance still needs to be updated around the stronger packet

## Implementation Notes

Changed surfaces:

- `crates/effigy-codegraph/src/json.rs`
- `crates/effigy-codegraph/src/query/mod.rs`
- `src/runner/graph_command.rs`
- `crates/effigy-codegraph/src/tests/context_quality.rs`
- `crates/effigy-codegraph/src/tests/storage_contracts.rs`
- `src/tests/runner_tests/runner_core_tests/graph_tests.rs`
- `docs/guides/076-code-graph-and-agent-workflows.md`

Design choices:

- `explore` reuses the same graph-walk model as `affected` instead of adding a
  second test-target heuristic path
- edit targets stay bounded
  - top owner is `ranked`
  - adjacent wiring/config targets reuse `exact` or `heuristic` confidence from
    the graph walk
- `contains`-only adjacency is filtered out for wiring, because it inflated live
  packets with structural ownership noise rather than useful next-edit files
- `likely_test_tasks` are narrowed for `explore` to explicit test-shaped task
  names instead of generic `qa` buckets

## Proof

Focused codegraph proof:

- split-ownership fixture now returns:
  - implementation edit target
  - wiring edit target
  - likely test file
  - likely test task
- JSON round-trip tests pin the new payload fields

Runner proof:

- graph JSON output tests now pin the new explore fields
- text-mode explore output now prints edit-target and likely-test lines

Live repo check:

```bash
effigy graph explore "where does effigy prompt to shut containers down on shell exit" --json
```

Observed packet:

- `edit_targets[0].path = src/runner/container_command/closeout.rs`
- no fake wiring target from `contains` edges
- likely test tasks stayed bounded to explicit test-shaped tasks

## Next

Move to `1026`: build the cross-repo agent-usage benchmark.
