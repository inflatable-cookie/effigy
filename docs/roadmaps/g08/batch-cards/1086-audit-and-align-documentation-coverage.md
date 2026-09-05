# 1086 - Audit And Align Documentation Coverage

Roadmap: [`../034-documentation-coverage-parity.md`](../034-documentation-coverage-parity.md)
Spec: [`../../../specs/archive/107-documentation-coverage-parity.md`](../../../specs/archive/107-documentation-coverage-parity.md)
Guides: [`037`](../../../guides/037-documentation-contribution-playbook.md),
[`035`](../../../guides/035-guide-ownership-and-update-triggers.md)

Status: Complete
Owner: documentation and public discovery surfaces
Created: 2026-08-21
Ready after: operator-selected repository-wide docs coverage audit

## Purpose

Build the implementation-to-documentation inventory, use it to find real
coverage gaps across the repository, and repair those gaps in one coherent
documentation batch.

## Work

- inventory current command/parser descriptors, global flags, selector
  affordances, JSON entry points, manifest/config fields, runtime behavior
  tests, built-in help/config docs, and the unreleased changelog
- compare them with root/docs front doors, active guides, the command matrix,
  troubleshooting, `.agents/skills/effigy/SKILL.md`,
  `skills/effigy/SKILL.md`, built-in help, and generated config reference
- use `effigy graph` for ownership/flow discovery and exact source inspection
  for final claims
- write a compact evidence matrix organized by behavior family with source
  owner, required docs surfaces, gap, and disposition
- repair every in-scope gap the matrix verifies; keep skill copies aligned
- explicitly prove coverage for the August managed-runtime seed case named in
  strict spec `107`
- update `CHANGELOG.md` under `[Unreleased]` for user-facing documentation and
  discoverability changes

## Acceptance

- [x] the inventory covers every current public command family and manifest
      behavior family through an explicit source owner
- [x] active user docs, agent guidance, built-in help, and generated config
      docs contain no unresolved in-scope gaps
- [x] the recent managed-runtime seed case is fully discoverable without
      relying on historical ledger logs
- [x] changes remain documentation-only except for rendering/check tests or
      other documentation infrastructure
- [x] the evidence matrix distinguishes fixed, already-covered, and blocked
      findings

## Validation

- focused help/config/skill rendering tests affected by the changes
- `effigy qa:docs`
- `effigy docs check workflow-paths`
- `effigy qa:docs:agent-defaults`
- `git diff --check`

## Evidence Requirement

Carry the matrix and exact changed-surface list into the card `1087` closeout
log. Do not claim whole-repo coverage from a keyword search alone.

Evidence:
[`2026-08/21-230738-documentation-coverage-parity-closeout.md`](../../../logs/archive/2026-08/21-230738-documentation-coverage-parity-closeout.md)

## Stop Conditions

Stop on production behavior changes, a new contract, workflow/release edits,
historical rewrites, or an unresolved product choice.

## Next Task

Card `1087` is complete. Return the reviewable worker PR to the orchestrator;
do not merge.
