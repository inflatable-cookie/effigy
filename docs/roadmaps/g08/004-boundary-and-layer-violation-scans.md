# g08.004 - Boundary And Layer Violation Scans

Status: Complete
Depends on: `g08.003`

## Goal

Add graph-native scans that detect unexpected dependency edges between declared
areas of a repo.

This is one of the strongest uses of graph data: filesystem scans cannot tell
which modules call across a boundary, but the graph can.

## Scope

- design a repo-agnostic boundary rule model
- support path-based and symbol/module-based boundary groups
- scan graph edges for disallowed references, calls, imports, or manifest task
  dependencies
- emit precise source and destination evidence
- keep rules optional; repos without boundary config should not fail

## Rule Shape To Explore

The rule model should be simple enough for ordinary repos:

```toml
[scan.boundaries.layers.app]
paths = ["src/app/**"]
may_depend_on = ["domain", "shared"]

[scan.boundaries.layers.domain]
paths = ["src/domain/**"]
may_depend_on = ["shared"]

[scan.boundaries.layers.shared]
paths = ["src/shared/**"]
```

This shape is illustrative, not final. The implementation should reuse
existing manifest/config conventions where possible.

## Guardrails

- do not hard-code Effigy crate names
- do not require every repo to define layers
- do not fail on heuristic edges unless rules explicitly include them
- do not report noisy self-edges or test-only edges unless configured
- keep each finding tied to a concrete graph edge

## Acceptance Criteria

- fixture repo proves an allowed edge and a rejected edge
- JSON output identifies source layer, target layer, edge kind, and path/range
  evidence
- no-config repos produce a clear no-rules result, not a failure
- docs show one small generic config example

## Next Task

Start `g08.005`.
