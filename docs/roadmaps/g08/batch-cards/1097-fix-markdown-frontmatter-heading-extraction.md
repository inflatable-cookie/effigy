# 1097 - Fix Markdown Frontmatter Heading Extraction

Roadmap: [`../042-markdown-frontmatter-extraction-papercut.md`](../042-markdown-frontmatter-extraction-papercut.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/041-documentation-graph-profile-contract.md`](../../../contracts/041-documentation-graph-profile-contract.md)
Papercut: [`PAPERCUTS.md`](../../../../PAPERCUTS.md)

Status: Ready
Owner: `effigy-codegraph` Markdown structural extraction
Created: 2026-09-01
Ready since: 2026-09-01 papercut triage on current `main`

## Purpose

Keep leading YAML frontmatter out of the Markdown section inventory without
losing metadata semantics or exact provenance.

## Observed Failure

For `docs/handoffs/*.md`, the Markdown parser can read the closing frontmatter
`---` as a setext underline for the preceding metadata. The graph then emits
one heading whose display text contains most of the frontmatter block. That
unusable section competes for the bounded `docs context` result budget.

## Work

- recognize only a complete leading YAML frontmatter block
- prevent that block from producing a Markdown heading section
- preserve the full-file document node and original source coordinates for
  real headings and metadata
- preserve profile-configured field facts and labelled relations from
  frontmatter
- preserve ordinary ATX/setext headings and non-leading delimiter behavior
- add non-vacuous baseline, profiled, and command-output recurrence proof
- close the selected papercut and write one compact evidence log

## Acceptance

- [ ] no frontmatter key/value or closing fence becomes a heading section
- [ ] the first real ATX heading after frontmatter has its exact original line
      and byte span
- [ ] an ordinary setext heading after frontmatter remains indexed with exact
      provenance
- [ ] configured field facts and labelled relation links inside frontmatter
      remain available with exact spans
- [ ] incomplete leading and non-leading `---` shapes do not cause content to
      be silently skipped
- [ ] baseline extraction requires no docs profile or Northstar vocabulary
- [ ] no ranking, budgeting, traversal, graph-store, refresh, CLI, or JSON
      contract changes
- [ ] focused codegraph/CLI tests, `perf:docs-context-benchmark`, `effigy qa`,
      fmt, clippy, and diff checks pass
- [ ] papercut, contract, roadmap, card, evidence, and active next-task pointers
      close honestly and return to publication planning

## Review Oracle

Falsify these counterexamples before PR creation:

1. `---\ntitle: Example\n---\n# Real` still emits `title: Example` or the
   frontmatter block as a heading.
2. Removing the synthetic heading shifts the line/byte span of `# Real`, a
   later setext heading, or the full document node.
3. A configured `Status: ready` fact or labelled Markdown relation inside the
   frontmatter disappears, moves, or becomes prose-only.
4. An opening `---` without a closing fence, or a `---` after ordinary prose,
   causes the extractor to discard the rest of the document.
5. The implementation recognizes Northstar handoff keys or paths instead of a
   generic leading-frontmatter shape.
6. The repair changes docs-context ranking/budgets/traversal, profile grammar,
   graph storage, refresh timing, CLI grammar, JSON, or unrelated extraction.

## Validation

- focused `effigy-codegraph` Markdown/profile tests
- focused `docs context` text and JSON command-output tests
- `effigy perf:docs-context-benchmark`
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Write one dated closeout log mapping every oracle row to exact proof. Record
the real heading spans, preserved frontmatter metadata semantics, validation,
papercut closure, and return to official publication planning.

## Stop Conditions

Stop if the fix requires a docs-profile grammar or field/relation semantic
change, a parser dependency upgrade, a graph-store migration, a ranking or
query-budget decision, a CLI/JSON contract change, timeout work, or a
Northstar-specific runtime rule.

## Next Task

Execute this card, then return to official catalog-pack publication planning
under contract `043`.
