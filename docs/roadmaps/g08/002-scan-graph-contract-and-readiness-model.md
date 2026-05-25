# g08.002 - Scan Graph Contract And Readiness Model

Status: Complete
Depends on: `g08.001`

## Goal

Define the contract between `effigy scan` and `effigy graph`.

Scan commands need to know whether graph data is usable, stale, missing, or
degraded. That decision must be visible in output and must not surprise users
by mutating the graph index.

## Scope

- inspect current `effigy-scan` and `effigy-codegraph` contracts
- define a small graph-readiness payload for scan output
- choose command shape for graph-backed scan behavior
- keep no-index behavior deterministic and documented
- add fixture coverage for ready, stale, missing, and degraded graph states

## Command Shape To Evaluate

Prefer one of these, after code inspection:

- `effigy scan <family> --use-graph`
- `effigy scan <family> --graph-context`
- `effigy scan graph <family>`

The chosen shape must make it clear whether graph data is optional enrichment
or required input.

## JSON Contract Requirements

Graph-aware scan output should include:

- `graph.used`
- `graph.state`
- `graph.reason`
- `graph.indexed_at` when available
- `graph.degraded_paths_count` or equivalent summary when relevant

Findings enriched by graph data should include:

- the original filesystem finding fields
- graph facts used as evidence
- whether severity changed
- a stable reason string suitable for tests and agents

## Guardrails

- no hidden `graph index`
- no global scan behavior change when a stale graph exists
- no mandatory graph dependency for existing scan families
- no DB-layout leakage into public JSON

## Acceptance Criteria

- contract is documented before broad implementation
- JSON tests pin graph-ready and graph-missing outputs
- human output clearly says when graph enrichment was skipped
- later roadmap files can build on the same readiness payload

## Next Task

Start `g08.003`.
