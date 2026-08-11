# 1077 - Enforce Pre-Release CI Proof

Roadmap: [`../030-pre-release-ci-proof.md`](../030-pre-release-ci-proof.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/039-pre-release-ci-proof-contract.md`](../../../contracts/039-pre-release-ci-proof-contract.md)
Spec: [`../../../specs/archive/103-pre-release-ci-proof.md`](../../../specs/archive/103-pre-release-ci-proof.md)

Status: Complete
Owner: Platform
Created: 2026-08-11

## Purpose

Close the gap that allowed a release candidate to reach tag creation without a
normal hosted CI run.

## Work

- add an exact-HEAD successful manual CI checker
- require it through Effigy's self-hosted `[release.gates]`
- put CI dispatch, exact run selection, and watch before release commands
- align AGENTS, bundled skill mirrors, guides, checklist, and changelog
- promote the durable invariant and close the strict lane

## Acceptance

- [x] matching successful SHA passes
- [x] missing or different SHA exits non-zero with remediation
- [x] release config test proves the gate remains installed
- [x] active protocol surfaces reject merely recent green CI
- [x] workflow YAML remains unchanged because `ci.yml` already supports manual
      dispatch

## Validation

- focused release-command test for the checker and self-host config
- shell syntax check
- formatting, docs policy, and changed-file hygiene

## Stop Conditions

Stop if enforcement requires automatic release mutation, accepting non-exact
evidence, or coupling the generic release crate to GitHub.

## Next Task

Lane complete. Evidence:
[`11-182709-pre-release-ci-proof-closeout.md`](../../../logs/2026-08/11-182709-pre-release-ci-proof-closeout.md).
