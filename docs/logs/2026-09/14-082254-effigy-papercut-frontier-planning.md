# Effigy Papercut Frontier Planning

Status: complete
Created: 2026-09-14
Roadmap: g10.004, g10.005
Batch: bounded papercut intake

## Summary

- Promoted two independent, behaviorally settled repairs as ready tasks.
- Closed the graph-explore papercut as stale because current main already has
  the shared wall-clock bound and typed timeout path.
- Kept vendored skill portfolio sync triage-only.

## Changes

- Added ready tasks `g10.004` and `g10.005` with separate mutable scopes,
  acceptance oracles, validation, continuation, and stop conditions.
- Added one ready-to-launch Queue handoff per task.
- Published both tasks as a parallel frontier with no dependency edge. Shared
  planning front doors and lifecycle closeout remain coordinator/hook-owned.

## Reconciliation evidence

- `src/runner/graph_command.rs` routes `GraphSubcommand::Explore` through
  `run_bounded_graph_operation`; commit `6ce994047c54486c2f38e9f33a882565b019ade7`
  introduced the shared bound on 2026-09-01. Guide 076 documents the 120000ms
  default and `effigy.graph.timeout.v1` failure. No duplicate graph lane exists.
- `crates/effigy-docs-policy/src/lib.rs::insert_log_index_entry` still searches
  for `## Archived Validation Logs` and otherwise appends at EOF. The current
  log front door requires insertion under `## Active logs`.
- `cli_container_attached_session_handles_sigint_during_startup` gives both the
  fake startup delay and marker wait a three-second budget. Runtime contracts
  already fix manager-owned clean interrupt semantics, so the ready lane is
  test-only unless evidence triggers its stop condition.

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`, `CONTRACT`
- Movement: baseline unscheduled papercuts -> current two-task ready reliability
  frontier plus one evidence-backed stale closure
- Remaining gap: Queue implementation, independent review, merge, and hook-owned
  closeout for `g10.004` and `g10.005`

## Validation Performed

- command: `effigy --json docs context "What architecture and contracts govern graph explore time bounds, docs log-index mutation placement, and container attached-session SIGINT startup behavior?"`
  - result: passed; current contract and ownership evidence returned
- command: `effigy qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- The log-index worker must fail closed rather than create a new fallback
  grammar.
- The SIGINT worker must escalate if the production path, not the fixture, is
  defective.
- Shared integration and hook-closeout surfaces serialize publication even
  though implementation scopes are independent.

## Next Task

- Dispatch `g10.004` and `g10.005` independently through Northstar Queue.
