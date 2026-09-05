# 1091 - Audit And Refresh Documentation, Instructions, And Help

Roadmap: [`../036-documentation-instruction-and-help-parity-refresh.md`](../036-documentation-instruction-and-help-parity-refresh.md)
Spec: [`../../../specs/archive/109-documentation-instruction-and-help-parity-refresh.md`](../../../specs/archive/109-documentation-instruction-and-help-parity-refresh.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md)
Guides: [`035`](../../../guides/035-guide-ownership-and-update-triggers.md),
[`037`](../../../guides/037-documentation-contribution-playbook.md)
Prior evidence: [`g08.034`](../034-documentation-coverage-parity.md)

Status: Complete
Owner: documentation, instruction, help, and documentation-validation surfaces
Created: 2026-08-30
Completed: 2026-08-30

## Purpose

Audit the current public feature surface against every active documentation,
instruction, built-in help, and generated reference entry point; repair the
verified gaps; then leave evidence and guards strong enough to prevent silent
drift.

## Work

- run `effigy tasks`, `effigy doctor`, and every general `effigy scan` family:
  `god-files`, `boundary-violations`, `dead-code`, `duplicate-blocks`,
  `comment-ratio`, `generated-assets`, `generated-in-src`,
  `attention-markers`, and `stale-suppressions`; run `validation-gaps` over the
  final changed-file set
- record counts and dispositions for every finding; repair documentation,
  instruction, help, or validation-infrastructure findings in this lane and
  explicitly defer unrelated code-only findings without disguising them as
  completion
- perform the canonical Northstar AGENTS instruction-surface review against
  root `AGENTS.md` and `CLAUDE.md`; the operator authorizes bounded repairs to
  those files, but not inspection or modification of Northstar's own source
- inventory current public behavior from command/parser descriptors, built-in
  registries, global flags, selector affordances, JSON entry points,
  manifest/config types, generated config, behavior tests, and `CHANGELOG.md`
- rebuild the whole behavior-family evidence matrix from current `main`; use
  the g08.034 matrix as a checklist, not as current proof
- compare the inventory with `README.md`, `docs/README.md`,
  `docs/guides/README.md`, every relevant active guide, command reference,
  troubleshooting, contracts that advertise live behavior, both Effigy skill
  trees, root agent instructions, built-in general/scoped help, and generated
  config/reference output
- use `effigy graph` for ownership and behavior-flow discovery, then verify
  final claims against exact source and rendered output
- repair every verified in-scope gap with concise routing plus sufficient deep
  guidance; keep project-local and distributed skill trees semantically aligned
- add or extend deterministic coverage tests for stable registry, help,
  generated reference, instruction, and skill relationships without asserting
  arbitrary prose or creating a second feature registry
- update `CHANGELOG.md` under `[Unreleased]` for user-facing discovery changes
- publish one dated evidence log and close the lane; return strict spec `108`,
  roadmap `g08.035`, and card `1089` to the ready state

## Acceptance

- [x] the feature matrix covers every current public command and manifest
      behavior family through an explicit source owner
- [x] every behavior family has a truthful route through active docs and the
      relevant general or scoped CLI help
- [x] generated config/reference output agrees with live manifest types and
      routes deeper explanation correctly
- [x] Northstar AGENTS review evidence includes size/shape metrics, link and
      command checks, bridge status, findings, and repair/retention decisions
- [x] all scan families and final changed-file validation-gap analysis have
      recorded results and honest dispositions
- [x] all verified in-scope gaps are fixed; blocked items name the exact
      authority, product, or scope reason
- [x] recurrence guards cover every stable relationship discovered during the
      audit that is valuable enough to enforce
- [x] no production behavior, workflow, release, or historical evidence changes
      enter the patch
- [x] focused checks and full validation pass
- [x] closeout surfaces agree that card `1091` is complete and `1089` is ready

## Evidence

Closeout log: [`30-174452-documentation-instruction-help-parity-closeout.md`](../../../logs/archive/2026-08/30-174452-documentation-instruction-help-parity-closeout.md)

## Validation

- focused help/parser/render and generated-config tests affected by changes
- `effigy test --test documentation_coverage_tests`
- `effigy qa:docs`
- `effigy docs check workflow-paths`
- `effigy qa:docs:agent-defaults`
- rerun the general scan family and changed-file `validation-gaps`
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Close with one dated log containing the feature matrix, help/config proof,
Northstar AGENTS review, scan before/after counts and dispositions, changed
surfaces, focused/full validation, residuals, and the readiness transition back
to card `1089`.

## Stop Conditions

Stop on production behavior or public contract changes, a new product decision,
workflow/release mutation, historical rewrite, unbounded prose churn, or a
code-only scan repair outside this lane. Stop if a feature family cannot be
classified from current authority or if full validation exposes a behavior
defect that needs its own plan.

## Next Task

After evidence-backed closeout, return to
[`1089`](./1089-add-bounded-documentation-context-query.md).
