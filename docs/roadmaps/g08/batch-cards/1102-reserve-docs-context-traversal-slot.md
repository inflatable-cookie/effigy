# 1102 - Reserve a Docs Context Traversal Slot

Roadmap: [`../047-docs-context-traversal-budget-papercut.md`](../047-docs-context-traversal-budget-papercut.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/041-documentation-graph-profile-contract.md`](../../../contracts/041-documentation-graph-profile-contract.md)
Papercut: [`PAPERCUTS.md`](../../../../PAPERCUTS.md)

Status: Ready
Owner: documentation-context result selection
Created: 2026-09-01
Ready since: 2026-09-01 operator-approved papercut routing

## Purpose

Ensure typed-relation traversal can contribute evidence when 0-hop lexical
candidates outnumber the section budget.

## Work

- add a deterministic saturated-corpus recurrence fixture
- reserve one eligible section slot for the best whole traversed candidate when
  at least two slots are available
- preserve the best lexical result and fill remaining capacity with existing rank
- prove one-slot, no-traversal, oversized-result, provenance, and truncation behavior
- close this card with one evidence log

## Acceptance

- [ ] with more lexical seeds than `max-sections`, `max-sections >= 2` returns
      the best lexical result first and at least one `hops > 0` result
- [ ] `max-sections = 1` returns only the best lexical result
- [ ] without a traversed candidate all slots retain existing direct rank order
- [ ] an oversized traversed section is omitted whole with a budget reason
- [ ] relevance gates, authority/currentness ordering, provenance, hop limits,
      and unrelated benchmark cases do not drift
- [ ] focused codegraph tests, docs-context benchmark, full QA, Rust checks, and
      diff checks pass

## Review Oracle

Falsify these counterexamples before PR creation:

1. Lexical saturation still consumes every section slot.
2. Traversal reservation displaces or ranks ahead of the best lexical result.
3. A one-section query returns traversal instead of the best direct match.
4. With no traversal, a reserved hole reduces the number of direct results.
5. An oversized traversed section is sliced, exceeds bytes, or hides truncation.
6. The fix adds a second query mode/ranker or weakens relevance and provenance.

## Validation

- focused `effigy-codegraph` docs-context tests
- focused CLI text/JSON tests only if public output changes
- `effigy perf:docs-context-benchmark`
- `effigy graph affected` for changed source, then direct targets
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Write one dated closeout log mapping every oracle row to exact proof. Record the
saturated fixture shape, selected ordering, byte behavior, unchanged benchmark
cases, and validation.

## Stop Conditions

Stop if the repair needs a new public query mode, embeddings/inference, a second
ranking implementation, a JSON schema break, graph refresh changes, or
rewriting unrelated frozen benchmark cases.

## Next Task

Return the exact-head PR to the Effigy orchestrator.
