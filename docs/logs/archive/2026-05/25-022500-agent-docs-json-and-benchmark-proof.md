# Agent Docs, JSON, And Benchmark Proof

Date: 2026-05-25
Roadmap: `g08.007`
Batch: `1035`

## What Changed

- updated the Effigy skill and repo-local skill copy to route graph-aware scan
  work by job:
  - navigation stays on `graph explore`
  - risk review uses `scan boundary-violations`, `scan dead-code`, or
    `scan validation-gaps`
- updated the command matrix and agent adoption guide to include the new
  graph-aware scan lane
- added JSON examples for:
  - graph-enriched `scan god-files`
  - `scan boundary-violations`
  - `scan dead-code`
  - `scan validation-gaps`
- extended `perf:graph-agent-benchmark` with a deterministic fixture-backed
  `validation-gaps` proof case

## Validation

- `cargo fmt --all -- --check`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1034b cargo build --bin effigy`
- `/tmp/effigy-g08-1034b/debug/effigy perf:graph-agent-benchmark /tmp/effigy-g08-1034b/debug/effigy`
- `effigy docs check links docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/guides/047-agent-and-cross-repo-adoption.md docs/guides/076-code-graph-and-agent-workflows.md skills/effigy/SKILL.md .agents/skills/effigy/SKILL.md skills/effigy/references/agent-operating-loop.md .agents/skills/effigy/references/agent-operating-loop.md skills/effigy/references/graph-assist.md .agents/skills/effigy/references/graph-assist.md skills/effigy/references/workflow-shortcuts.md .agents/skills/effigy/references/workflow-shortcuts.md CHANGELOG.md`
- `effigy docs check json-examples`
- `effigy docs check index`
- `effigy docs check next-action --policy vision`

## Vision Target Delta

- primary tags: `ROUTE`, `CONTRACT`, `OPERATE`
- moved: agent-facing guidance, JSON examples, and the reusable benchmark task now cover graph-aware scans in addition to navigation queries
- remains open: `1036` closeout and residual-limit summary
