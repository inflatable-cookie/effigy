# Graph-Aware Scan Lane Opened

Date: 2026-05-25
Roadmap: [`g08.001`](../roadmaps/g08/001-graph-aware-scan-intelligence-suite.md)
Strict lane: [`097`](../specs/097-graph-aware-scan-intelligence-strict-lane.md)
Batch card: [`1029`](../roadmaps/g08/batch-cards/1029-open-graph-aware-scan-lane.md)

## Baseline

Current scan surface is filesystem-first and independent of graph state.

Observed scan families from `crates/effigy-scan` and the built-in help:

- `god-files`
- `duplicate-blocks`
- `comment-ratio`
- `generated-assets`
- `generated-in-src`
- `attention-markers`
- `stale-suppressions`

Current live JSON examples confirm scan-family-specific result schemas under
the command envelope:

- `effigy --json scan god-files` -> `effigy.scan.god-files.v1`
- `effigy --json scan duplicate-blocks` -> `effigy.scan.duplicate-blocks.v1`
- `effigy --json scan attention-markers` -> `effigy.scan.attention-markers.v1`

Observed traits from live scan output:

- each scan reports its own `schema` and `schema_version`
- scans report file counts, thresholds, and findings in file-local terms
- scans do not currently expose graph readiness, graph evidence, or graph
  enrichment state
- text mode remains the human-facing default even in `--json` command output

Observed live baseline in this repo:

- `god-files`: `0` findings, `1236` scanned files
- `duplicate-blocks`: `105` findings, `1` high, `104` warning, `1237`
  scanned files
- `attention-markers`: `0` findings, `1237` scanned files

## Graph Readiness Baseline

Current graph status already has a compact trust model and detailed
diagnostics.

Observed live `effigy --json graph status` shape:

- command payload schema: `effigy.graph.status.v1`
- `ready: true`
- `index_present: true`
- `freshness.state: "refresh-recommended"`
- `freshness.usable: true`
- `freshness.summary: "graph index is stale; run \`effigy graph index --json\`"`
- detailed `changed_paths`, `new_paths`, `deleted_paths`, `stale_paths`,
  `failed_paths`, and extractor inventory remain available

This is a strong starting point for `1030`. The graph side already knows how
to say "usable but stale" without hiding detail. The scan side has no contract
for reporting whether graph context was used, skipped, or unavailable.

## Integration Read

The current opportunity is additive rather than replacement-oriented.

Good fit for later graph-aware work:

- enrich existing file-level findings with graph context
- add graph-native scans where relationships are the core signal

Bad fit for the lane:

- making ordinary scans require an index
- auto-indexing from scan commands
- rewriting current scan schemas into one graph-shaped super-schema

## Fixture And Proof Surface

Existing repo-owned proof that can support this lane:

- `crates/effigy-codegraph/src/tests/*`
  - synthetic graph fixture repos already exist for indexing, query, and
    context quality
- `src/tests/runner_tests/runner_core_tests/graph_tests.rs`
  - built-in command coverage for graph status, search, affected, stale, and
    missing-index behavior
- `docs/guides/076-code-graph-and-agent-workflows.md`
  - existing graph benchmark references optional live repos:
    - `~/Dev/projects/underlay-reference`
    - decodelabs live repos when present
- `src/tests/runner_tests/builtin_command_tests/scan_tests/*`
  - scan behavior is already covered with file-level fixtures, but there is no
    dedicated graph-aware scan fixture family yet

Expected proof posture for this lane:

- fixture-first for command contracts and rule logic
- optional live-repo checks for Underlay and decodelabs-style repos
- no private local repo required for test pass

## Non-Goals Locked

- no hidden graph indexing from `scan`
- no breakage of existing `effigy.scan.*.v1` payloads
- no Effigy-only boundary rules
- no LLM-generated findings
- no MCP or daemon work

## Acceptance Read For 1030

`1030` should now define:

- how graph-required versus graph-enriched scan behavior is requested
- the minimal graph readiness payload scan commands need
- how no-index, stale, degraded, and ready states appear in scan output
- which parts of current scan JSON stay unchanged
