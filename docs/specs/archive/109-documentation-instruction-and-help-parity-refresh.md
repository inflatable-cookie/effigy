# 109 - Documentation, Instruction, And Help Parity Refresh

Status: Complete
Owner: active documentation and public discovery surfaces
Roadmap: [`g08.036`](../../roadmaps/g08/036-documentation-instruction-and-help-parity-refresh.md)
Contracts: [`001`](../../contracts/001-working-rules.md)
Guides: [`035`](../../guides/035-guide-ownership-and-update-triggers.md),
[`037`](../../guides/037-documentation-contribution-playbook.md)
Prior evidence: [`g08.034`](../../roadmaps/g08/034-documentation-coverage-parity.md)
Completed card: [`1091`](../../roadmaps/g08/batch-cards/1091-audit-and-refresh-documentation-instructions-and-help.md)

## Problem

Effigy has changed materially since the repository-wide documentation parity
audit closed. The repository now needs a fresh evidence-led sweep across scan
health, agent instructions, active user documentation, generated reference
output, and shipped CLI help. A keyword-only audit would miss relationships
between live descriptors, manifest types, tests, and the surfaces users or
agents actually enter.

## Decision

- Reuse the behavior-family evidence matrix and recurrence approach from strict
  spec `107`, but rebuild the matrix from current `main` rather than trusting
  the August 21 snapshot.
- Treat live command descriptors and parsers, built-in registries, manifest
  config types, JSON schemas/examples, behavior tests, and the unreleased
  changelog as implementation-side evidence.
- Audit every active user and agent surface: root and docs front doors, guide
  index and active guides, command matrix, troubleshooting, both Effigy skill
  trees, `AGENTS.md` with its `CLAUDE.md` bridge, built-in help, and generated
  config/reference output.
- Run the full repository scan family. Repair findings inside this lane only
  when they concern documentation, instruction, help, or their verification
  infrastructure. Record a clear disposition for code-only findings instead
  of widening into an unrelated refactor.
- Perform the canonical Northstar AGENTS instruction-surface review. The
  operator explicitly authorizes bounded evidence-backed repairs to
  `AGENTS.md` and `CLAUDE.md`.
- Fix every verified in-scope gap and add proportional deterministic guards for
  stable relationships. Do not freeze arbitrary prose or create a second
  feature registry.

## Audit Boundary

The sweep covers current public behavior across command families, global flags,
selector affordances, JSON entry points, manifest/config families, environment
and machine overrides, built-in diagnostics, agent workflows, and generated
reference material. It proves that each behavior family has a discoverable
route through active documentation and the relevant CLI help.

Historical logs, archived specs, closed roadmap prose, vendored material,
generated build output, private helpers, and release artifacts are evidence,
not rewrite targets.

## Non-Goals

- no production behavior or public API change
- no code-quality refactor solely to clear a scan finding
- no release, dependency, or CI workflow mutation
- no rewrite of logs, archived specs, or closed planning evidence
- no broad prose restyling without a verified coverage or currentness gap

## Acceptance

- [x] one current evidence matrix covers every public command and manifest
      behavior family with source owner, active docs/help routes, finding, and
      disposition
- [x] every shipped top-level and scoped help family is checked against current
      behavior and routed to sufficient active documentation
- [x] generated config/reference output covers every current public manifest
      family without conflicting with long-form guidance
- [x] the Northstar AGENTS review records metrics, findings, bridge status, and
      every bounded repair or retained decision
- [x] all general scan families run; every finding is fixed, already accepted,
      or explicitly deferred with its owner and reason
- [x] active front doors, guides, skills, help, generated reference, contracts,
      and changelog contain no unresolved in-scope coverage gaps
- [x] deterministic recurrence checks protect stable coverage relationships
      without duplicating runtime authority
- [x] focused checks, docs QA, formatting, Clippy, and full Effigy QA pass
- [x] one dated closeout log records the matrix, scan results, changed surfaces,
      validation, residuals, and the return to card `1089`

## Evidence

- Planning: [`2026-08/30-164636-documentation-instruction-help-refresh-planning.md`](../../logs/archive/2026-08/30-164636-documentation-instruction-help-refresh-planning.md)
- Closeout: [`2026-08/30-174452-documentation-instruction-help-parity-closeout.md`](../../logs/archive/2026-08/30-174452-documentation-instruction-help-parity-closeout.md)

## Stop Conditions

Stop and return to the orchestrator if the audit requires a production behavior
change, a new product or public API decision, a workflow edit, release
mutation, historical rewrite, or code-only refactor outside
documentation/help ownership. Stop if claimed whole-surface coverage cannot be
backed by an explicit matrix.

## Next Task

Run the active documentation-graph lane at ready card
[`1089`](../../roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md).
