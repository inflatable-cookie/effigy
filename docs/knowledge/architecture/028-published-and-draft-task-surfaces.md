# Task Visibility And Grouping

Contract: [`046`](../contracts/046-published-and-draft-task-surface-contract.md)

A task has one runtime identity and one execution path. Public/internal
visibility is a discovery property, with an optional human-facing group.
The manifest crate owns declared metadata and legacy syntax translation;
`effigy-tasks::TaskVisibilityIndex` owns effective discovery policy. Inventory
and completion consume that index instead of maintaining separate rules.

The index resolves task references through the ordinary catalog resolver,
pinning bare references to the declaring catalog. Unnamespaced names remain
public entry points, namespaced referenced helpers default to internal, and
explicit visibility wins. The contract owns the exact rules and their consumer
proof rationale.

Text renders public rows in deterministic groups. JSON always projects the
whole inventory with effective metadata. Completion filters using the same
index and stamps composed sources for cache invalidation. Direct invocation,
plans and status resolve every task regardless of visibility; kernel identity,
gates and custody are unchanged.

Legacy task syntax translates at the manifest decoder before runtime
selection. The former draft model, lifecycle inventory and parallel execution
path have no runtime owner. QA group lifecycle dates have their own small date
type, independent of task discovery. Compatibility commands either translate
to ordinary invocation or explain their replacement, as specified in 046.
