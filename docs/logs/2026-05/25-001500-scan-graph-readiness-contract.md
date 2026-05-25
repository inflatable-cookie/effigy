# Scan Graph Readiness Contract

Date: 2026-05-25
Roadmap: [`g08.002`](../roadmaps/g08/002-scan-graph-contract-and-readiness-model.md)
Strict lane: [`097`](../specs/097-graph-aware-scan-intelligence-strict-lane.md)
Batch card: [`1030`](../roadmaps/g08/batch-cards/1030-define-scan-graph-readiness-contract.md)

## What Landed

`effigy scan` now accepts `--graph-context` on existing scan families.

This flag does not enable enrichment yet. It exposes graph readiness in a
stable additive contract so later cards can attach real graph context without
rewriting scan payloads again.

Current contract:

- no flag:
  - existing scan output is unchanged
- `--graph-context`:
  - JSON gains a `graph` object
  - text output gains a one-line note after the scan report

## Chosen Shape

The lane now has one explicit additive request shape for current scan families:

- `effigy scan <family> --graph-context`

Interpretation:

- the operator is asking for optional graph context
- scan still runs even when the graph is stale, missing, or unavailable
- the command reports graph readiness honestly instead of pretending
  enrichment happened

The lane deliberately does not introduce graph-required scan commands yet.
Those can be added later only if a graph-native family needs stricter behavior.

## JSON Contract

When `--graph-context` is present, scan payloads now add:

```json
"graph": {
  "requested": true,
  "applied": false,
  "state": "refresh-recommended",
  "usable": true,
  "summary": "graph index is stale; run `effigy graph index --json`",
  "reason": "graph context is not implemented for `god-files` yet; this request only reports graph readiness"
}
```

Properties:

- additive only
- no existing top-level fields were renamed or removed
- schema names remain `effigy.scan.*.v1`
- no hidden graph indexing occurs

State coverage currently supported:

- `ready`
- `refresh-recommended`
- `degraded`
- `missing-index`
- `unavailable` when graph status lookup itself fails

## Text Contract

When `--graph-context` is present in text mode, scan output appends:

- `Graph context: requested, not applied (...)`

This keeps the human output honest:

- readiness is visible
- scan findings are still plain filesystem findings
- no false suggestion that graph enrichment already exists

## Proof

Focused test proof:

- parser coverage in `effigy-builtin`
- JSON contract proof for `god-files`
- text output proof for `god-files`

Live command proof with rebuilt binary:

- `cargo run --bin effigy -- --json scan god-files --graph-context`
- `cargo run --bin effigy -- scan god-files --graph-context`

Observed live behavior in this repo:

- JSON emitted `graph.state = "refresh-recommended"`
- JSON emitted `graph.usable = true`
- text mode printed the graph note after the scan report

## Non-Goals Preserved

- no graph enrichment yet
- no changes to scan-engine heuristics
- no graph-required scan families yet
- no auto-indexing
- no breakage of existing `effigy.scan.*.v1` consumers

## Read For 1031

`1031` can now enrich existing scans by:

- keeping `graph.requested`
- switching `graph.applied` to `true` where enrichment actually happens
- reusing `state`, `usable`, and `summary`
- attaching graph evidence without reworking the top-level scan schema
