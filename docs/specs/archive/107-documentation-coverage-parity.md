# 107 - Documentation Coverage Parity

Status: Complete
Owner: documentation and public discovery surfaces
Roadmap: [`g08.034`](../../roadmaps/g08/034-documentation-coverage-parity.md)
Contracts: [`001`](../../contracts/001-working-rules.md)
Guides: [`037`](../../guides/037-documentation-contribution-playbook.md),
[`035`](../../guides/035-guide-ownership-and-update-triggers.md)

## Problem

Effigy's current behavior can be correct while its agent skill, built-in help,
generated configuration reference, command matrix, and long-form guides expose
different subsets of that behavior. The August managed-runtime work made this
visible: the deep guides covered headless supervision and workspace identity,
but the project-local skill and some built-in discovery surfaces did not.

A one-feature patch is not enough evidence. The repository needs a bounded
whole-surface audit that compares current public behavior with every active
user- and agent-facing documentation layer, fixes the gaps it finds, and leaves
a regression check for mechanically detectable drift.

## Decision

- Treat live command descriptors/parsers, manifest/config types and examples,
  built-in help/config docs, runtime behavior tests, and the current changelog
  as the implementation-side inventory.
- Compare that inventory with the root/docs front doors, active guides,
  command matrix, troubleshooting guidance, both Effigy skill copies, built-in
  help, and generated config reference.
- Record an evidence matrix that names each audited behavior family, its source
  owner, its active documentation surfaces, the gap found, and the resolution.
- Fix every in-scope gap found during the sweep. Prefer concise routing links
  over duplicating full guides into every surface.
- Add or extend deterministic tests/checks for coverage relationships that can
  be enforced without freezing prose or inventing a second command registry.

## Audit Boundary

The sweep covers current public Effigy behavior across:

- built-in command families, global flags, selector affordances, and JSON
  entry points;
- manifest and generated configuration fields;
- managed development, secrets, container/workspace identity, doctor, and
  execution behavior;
- operator and agent routing needed to discover those surfaces.

Historical logs, archived guides/specs, imported planning text, internal-only
helpers, and private implementation details are evidence, not rewrite targets.

The recent managed-runtime changes are a required seed case, not the audit's
limit. The matrix must explicitly cover headless managed mode and companions,
readiness scoping and timeout, concurrent start order, optional container
secrets under forced unlock, workspace ownership diagnosis, and non-console
`effigy exec` identity.

## Non-Goals

- no production behavior or public API change
- no new architecture or product contract
- no release, tag, dependency, or workflow mutation
- no historical rewrite merely to make old evidence sound current
- no broad prose restyling unrelated to a verified coverage gap

## Acceptance

- [x] a repository-wide evidence matrix maps current public behavior families
      to implementation owners and active documentation surfaces
- [x] every in-scope gap found by the matrix is fixed or named as a blocked
      item with a concrete reason
- [x] the project-local and distributed Effigy skills remain synchronized and
      route agents to all current operational surfaces they need
- [x] built-in help and generated configuration documentation expose the
      relevant commands, flags, environment overrides, findings, and fields
- [x] active front doors, command/reference guides, deep guides, and
      troubleshooting agree without unnecessary duplication
- [x] deterministic regression coverage protects mechanically checkable
      relationships
- [x] docs checks, focused tests, formatting, Clippy, and full Effigy QA pass

## Evidence

- [`2026-08/21-230738-documentation-coverage-parity-closeout.md`](../../logs/archive/2026-08/21-230738-documentation-coverage-parity-closeout.md)

## Stop Conditions

Stop and return to the orchestrator if the audit requires production behavior,
a new public contract, a workflow edit, release mutation, or a product decision
that cannot be resolved from current code and canonical docs.

## Next Task

Run the second governance review by 2026-09-17. Await operator intent for the
next Horizon theme; do not infer release work or generation rollover.
