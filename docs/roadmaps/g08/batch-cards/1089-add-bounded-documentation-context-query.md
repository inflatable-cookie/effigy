# 1089 - Add Bounded Documentation Context Query

Roadmap: [`../035-repository-defined-documentation-graph.md`](../035-repository-defined-documentation-graph.md)
Architecture: [`../../../architecture/024-repository-defined-documentation-graph.md`](../../../architecture/024-repository-defined-documentation-graph.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/041-documentation-graph-profile-contract.md`](../../../contracts/041-documentation-graph-profile-contract.md)
Spec: [`../../../specs/108-documentation-graph-profiles-strict-lane.md`](../../../specs/108-documentation-graph-profiles-strict-lane.md)
Predecessor: [`1088`](./1088-build-documentation-profile-and-structural-index.md)

Status: Ready
Owner: codegraph query and built-in docs command surfaces
Created: 2026-08-29
Ready after: card `1088` closeout proves stable structural records and card
`1091` closes the overlapping documentation/help maintenance lane
Resumed: external skill-runner card `1092` closed with evidence on 2026-08-31

## Purpose

Expose the structural graph as a small deterministic evidence packet for agents.

## Owner And Seam

`effigy-codegraph` owns retrieval, traversal, budgeting, and typed reports.
`effigy-cli` and the built-in docs shell own grammar, help, root selection,
rendering, envelope, and exit behavior. Do not duplicate ranking logic in the
runner.

## Work

- add `effigy docs context <QUERY>` with contract `041` budgets
- restrict candidates to profile roots or baseline Markdown scope
- reuse FTS lexical seeds and bounded graph traversal
- rank relevance before currentness and authority
- return exact, deduplicated sections with facts, relation paths, provenance,
  match reasons, freshness, and truncation state
- add concise text rendering and `effigy.docs.context.v1` JSON
- add schema, example, selection, help, generated config/reference, and command
  matrix coverage required by current documentation contracts
- prove no-match, direct historical match, current-authority tie, traversal,
  stable ordering, and each budget boundary

## Acceptance

- [ ] baseline and profiled repositories use the same command and report shape
- [ ] unrelated high-authority documents cannot outrank lexical candidates
- [ ] directly named historical sections remain retrievable
- [ ] hop, section, and byte limits are enforced and reported
- [ ] repeated queries over unchanged input return identical ordering
- [ ] text and JSON expose the same facts and evidence
- [ ] no command result contains model-generated summaries

## Validation

- focused codegraph query tests
- focused CLI/parser/help and built-in docs tests
- JSON schema and selection validation
- `cargo fmt --all -- --check`
- focused Clippy for changed crates
- `git diff --check`

## Evidence Requirement

Close with one dated log containing query fixtures, exact budget proofs, JSON
validation, test counts, and the explicit readiness transition for card `1090`.

## Stop Conditions

Stop if ranking requires remote inference, output cannot remain bounded, the
command needs a second refresh path, or authority can introduce unrelated
results.

## Next Task

Execute this card. Its evidence-backed closeout makes
[`1090`](./1090-prove-generic-and-northstar-profiles.md) ready.
