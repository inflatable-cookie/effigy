# g07.032 - Explore Context Assembly Command

Status: Complete
Depends on: `g07.031`

## Goal

Implement `effigy graph explore "<question>"` as the one-call agent navigation
surface.

## Scope

- add CLI parsing for `graph explore`
- add text and JSON renderers
- reuse current context ranking rather than fork a second ranker
- assemble bounded excerpts around the best evidence ranges
- include neighboring symbols and files from graph relations
- include docs/tests only when relevant to the query or adjacent to selected
  owners
- add focused tests for command parsing, JSON shape, excerpt bounds, and ranking
  behavior

## Implementation Notes

- start in `crates/effigy-codegraph/src/query/mod.rs` for query-level assembly
- keep CLI parsing in `crates/effigy-cli/src/command_parsing_graph.rs`
- route command execution through the existing graph command owner under `src/`
- keep JSON helpers near existing graph JSON projection code
- prefer small internal structs over ad hoc maps
- expose limits as flags only if needed:
  - `--limit` for primary entries
  - `--excerpt-lines` for each excerpt window
  - `--max-bytes` for total output cap

## Guardrails

- no global path hacks for Effigy-specific files
- no unbounded file reads
- no silent fallback to stale graph data without a freshness warning
- no command behavior that differs between text and JSON except formatting
- no breaking changes to existing `graph context`, `search`, `files`, `node`,
  `callers`, `callees`, or `impact`

## Acceptance Criteria

- `effigy graph explore "trace graph watch implementation"` returns primary
  implementation files plus useful excerpts
- JSON output is stable enough for contract tests
- text output is readable without hiding provenance
- existing graph tests remain green

## Evidence

- [`2026-05/18-133020-graph-explore-implementation-closeout.md`](../../logs/archive/2026-05/18-133020-graph-explore-implementation-closeout.md)

## Next Task

Execute `983`.
