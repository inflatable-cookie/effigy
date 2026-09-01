# g08.042 Markdown Frontmatter Extraction Papercut

Status: Ready
Created: 2026-09-01
Card: [`1097`](./batch-cards/1097-fix-markdown-frontmatter-heading-extraction.md)
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)
Papercut: [`PAPERCUTS.md`](../../../PAPERCUTS.md)

## Purpose

Stop a complete leading YAML frontmatter block from becoming a synthetic
setext heading while preserving the document's metadata facts, relations, and
exact source spans.

## Decision

- A complete YAML frontmatter block begins with `---` on the first line and
  ends at the next standalone `---` line.
- That block is metadata, not a Markdown section or heading.
- Profile-configured field facts and labelled relations inside the block remain
  extractable metadata.
- The document node still spans the complete file, and every real heading,
  relation, and fact keeps its original line and byte coordinates.
- An incomplete opening fence or a later `---` does not silently discard
  document content or redefine ordinary Markdown heading behavior.

## Scope

- `effigy-codegraph` Markdown extraction
- focused baseline/profile and command-output recurrence tests
- contract, papercut, evidence, and user-facing documentation closeout

## Boundary

- no docs-context ranking, budgeting, traversal, or output-schema change
- no docs-profile grammar, field-cardinality, relation, or currentness change
- no graph-store schema or refresh/timeout change
- no Northstar-specific runtime token or handoff-path special case
- no unrelated papercut, catalog-pack publication, release, workflow, or S3 work

## Cards

- [ ] [`1097`](./batch-cards/1097-fix-markdown-frontmatter-heading-extraction.md) — ready

## Acceptance

- a complete leading frontmatter block emits no heading section from its keys,
  values, or closing fence
- real ATX and setext headings after frontmatter retain exact original spans
- configured frontmatter field facts and labelled relations remain available
- incomplete or non-leading delimiter shapes do not suppress document content
- baseline extraction stays provider-neutral and deterministic
- focused, benchmark, and full repository validation pass

## Next Task

Execute ready card
[`1097`](./batch-cards/1097-fix-markdown-frontmatter-heading-extraction.md), then
return to official catalog-pack publication planning under contract `043`.
