# Agent Adoption And CLI Workflow Polish

Date: 2026-05-18  
Roadmap: [`g07.044`](../../roadmaps/g07/044-agent-adoption-and-cli-workflow-polish.md)  
Batch card: [`993`](../../roadmaps/g07/batch-cards/993-polish-agent-adoption-and-cli-workflow.md)  
Strict lane: [`091`](../../specs/091-codegraph-parity-strict-lane.md)

## What Changed

- updated [`skills/effigy/SKILL.md`](../../../skills/effigy/SKILL.md) so agents can
  discover the graph-first workflow without reading planning docs
- expanded graph command help to cover:
  - stale-index remediation
  - affected-test narrowing
  - exact-match fallback posture
  - local graph rebuild recovery
- updated graph-facing docs:
  - [`017-json-output-contracts.md`](../../guides/017-json-output-contracts.md)
  - [`021-quick-start-and-command-cookbook.md`](../../guides/021-quick-start-and-command-cookbook.md)
  - [`025-command-reference-matrix.md`](../../guides/025-command-reference-matrix.md)
  - [`026-json-payload-examples.md`](../../guides/026-json-payload-examples.md)
  - [`055-everyday-workflows.md`](../../guides/055-everyday-workflows.md)
  - [`076-code-graph-and-agent-workflows.md`](../../guides/076-code-graph-and-agent-workflows.md)
- updated codegraph rustdoc in [`crates/effigy-codegraph/src/lib.rs`](../../../crates/effigy-codegraph/src/lib.rs)

## Workflow Shape

The adoption surface now consistently teaches:

1. `effigy graph status --json`
2. if stale: `effigy graph index --json`
3. `effigy graph explore "<task>" --json`
4. `git diff --name-only | effigy graph affected --stdin --json` when narrowing validation
5. `graph context`, `graph search`, and `rg` only as lower-level follow-up tools

## Validation

- `cargo test -p effigy-cli`
- `cargo clippy -p effigy-cli -p effigy-codegraph -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo fmt --all -- --check`

## Interpretation

- the graph workflow is now easier to discover from the actual operator-facing
  surfaces instead of being trapped in roadmap lineage
- docs now teach stale recovery and exact-search fallback explicitly, which is
  the difference between useful graph adoption and brittle overtrust
- `graph affected` is now part of the normal agent loop rather than a hidden
  capability

## Residual Limits

- this slice did not add a new prompt-snippet command; the existing skill,
  help, and guides were enough for now
- final parity claims still belong to the dedicated closeout card

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: graph-first agent workflow is now discoverable from the skill, help,
  guides, JSON docs, and rustdoc
- remains open: final parity closeout and residual-gap decision

## Next Task

Execute `994`.
