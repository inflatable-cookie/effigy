# Markdown Frontmatter Extraction 1097 Closeout

Status: complete
Created: 2026-09-01
Roadmap: g08.042
Batch: 1097-fix-markdown-frontmatter-heading-extraction
Handoff: `20260901-134829-markdown-frontmatter-1097.md`
Papercut: YAML frontmatter is indexed as one setext heading

## Summary

- Leading YAML frontmatter (`---` on line 1 through the next standalone `---`)
  was parsed as one setext heading, so `docs context` could spend section budget
  on a useless multi-line display name.
- Extraction now recognizes only a complete leading fence block and suppresses
  headings that start inside that block. Field facts and labelled relations in
  the block keep their original spans. Incomplete and non-leading `---` shapes
  keep ordinary Markdown heading behavior.
- No ranking, budgeting, traversal, profile grammar, graph-store, refresh, CLI,
  or JSON contract changes.

## Review oracle → proof

1. `---\ntitle: Example\n---\n# Real` still emits frontmatter as a heading —
   falsified by
   `docs_profile::leading_yaml_frontmatter_is_metadata_not_a_heading` and
   `docs_context_omits_leading_yaml_frontmatter_from_heading_results`.
2. Removing the synthetic heading shifts real ATX/setext spans or the document
   node — falsified by exact line/byte asserts in the baseline unit test
   (`# Real` line 5 / setext line 7 / document `0..len`).
3. Configured `State: live` fact or labelled relation inside frontmatter
   disappears — falsified by
   `leading_yaml_frontmatter_keeps_profile_fields_and_relations` and the CLI
   JSON fields assert.
4. Incomplete opening or later `---` discards content — falsified by
   `incomplete_and_nonleading_yaml_delimiters_keep_document_content`.
5. Implementation recognizes Northstar handoff keys/paths — falsified by
   generic fence-only detection in `extract.rs` and the existing
   `documentation_graph_runtime_logic_carries_no_northstar_vocabulary` oracle.
6. Repair changes ranking/budgets/traversal, profile grammar, storage, refresh,
   CLI, or JSON — falsified by diff scope (`extract.rs` + tests/docs/changelog
   closeout only) and full `effigy qa`.

## Changes

- `crates/effigy-codegraph/src/language/markdown/extract.rs`: leading YAML
  frontmatter range detection; skip headings that start inside that range.
- Focused `effigy-codegraph` docs_profile tests and CLI text/JSON recurrence.
- Guide `079`, changelog, papercut, roadmap `g08.042`, card `1097`, and Next
  Task pointers closed back to publication planning.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`, `CONTRACT`
- Movement: leading frontmatter competed as a synthetic heading → metadata-only
  with preserved facts, relations, and exact spans
- Remaining gap: None for this papercut; official catalog-pack publication
  planning under contract `043` remains the Next Task. Ranking/timeout papercuts
  stay separate.

## Validation Performed

- `cargo test -p effigy-codegraph --lib yaml_frontmatter` — 2 passed
- `cargo test -p effigy-codegraph --lib incomplete_and_nonleading` — 1 passed
- `cargo test --test cli_output_tests docs_context_omits_leading_yaml_frontmatter`
  — 1 passed
- `effigy perf:docs-context-benchmark` — all predeclared expectations held
- `effigy qa` — 3636 passed, 1 skipped; docs and JSON-contract checks passed
- `cargo fmt --all -- --check` — passed
- `cargo clippy --all-targets -- -D warnings` — passed (existing
  `proc-macro-error2` future-incompat notice only)
- `git diff --check` — passed

## Risks

- Non-leading complete `---` … `---` pairs still become setext headings under
  ordinary Markdown rules. That is intentional and out of card scope.

## Next Task

- Return to planning for official catalog-pack publication and concrete-asset
  cutover under contract `043`. That lane needs a real OCI coordinate and
  explicit workflow-edit authority; it is not ready.
