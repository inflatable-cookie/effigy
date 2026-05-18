# Source Section Packets

Date: 2026-05-18  
Roadmap: [`g07.041`](../../roadmaps/g07/041-source-section-packets-and-no-reread-workflow.md)  
Batch card: [`990`](../../roadmaps/g07/batch-cards/990-harden-source-section-no-reread-packets.md)  
Strict lane: [`091`](../../specs/091-codegraph-parity-strict-lane.md)

## What Changed

- added explicit excerpt metadata to `graph explore`:
  - `section_kind`
  - `completeness`
- deduplicated same-path excerpts inside one explore packet so byte budget is
  not wasted repeating the same file
- added language-aware section extraction for:
  - Python function/class blocks
  - Python decorator-backed route handlers
  - Markdown heading sections
- kept bounded context-window fallback for the rest of the language surface
- added guidance text so agents can treat incomplete packets as a signal to
  open the file deliberately rather than guessing

## Completeness Contract

Current `completeness` values:

- `complete-section`
- `truncated-section`
- `surrounding-context`

Current `section_kind` examples:

- `python-block`
- `heading-section`
- `context-window`

This stays additive under the existing `effigy.graph.explore.v1` payload.

## Validation

- `cargo test -p effigy-codegraph`
- `cargo clippy -p effigy-codegraph -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo fmt --all -- --check`

New regression:

- `graph_explore_labels_python_sections_and_deduplicates_same_path_excerpts`

## Interpretation

- the explore packet is now clearer about what an agent can safely reason from
  without immediately reopening the same file
- Python route/handler flows now return a fuller local block instead of a thin
  window around one token match
- duplicate same-file excerpts from ranked file plus ranked symbol paths are no
  longer burning packet budget

## Residual Limits

- section extraction is still selective, not universal
- Rust, PHP, JS/TS, and manifest task sections still rely mostly on bounded
  surrounding context in this slice
- no benchmark harness was added here for explicit reread counts; that belongs
  with the affected-test and parity closeout cards

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: explore packets now declare section completeness explicitly and
  provide fuller supported sections with less duplicate noise
- remains open: changed-file test impact workflow, broader framework coverage,
  scale hardening, and final parity proof

## Next Task

Execute `991`.
