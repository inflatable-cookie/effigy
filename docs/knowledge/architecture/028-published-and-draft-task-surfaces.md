# Task Visibility And Grouping

Contract: [`046`](../contracts/046-published-and-draft-task-surface-contract.md)

A task has one runtime identity and one execution path. Public/internal
visibility is a discovery property, with an optional human-facing group.
The manifest crate owns declared metadata; `effigy-tasks::TaskVisibilityIndex`
owns effective discovery policy. Inventory
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

The draft namespace is retired (v0.15, Q-009): `[drafts]`, `draft` step
references, QA member `surface = "draft"` and the `effigy draft`/`effigy
drafts` commands are refused with their replacement, as specified in 046. The
former draft model, lifecycle inventory, parallel execution path and one-release
translation have no runtime owner. QA group lifecycle dates have their own
small date type, independent of task discovery.
