# g09.004 Release Gate Diagnosability

Status: Complete
Created: 2026-09-05
Spec: [`119`](../../specs/archive/119-release-gate-diagnosability-strict-lane.md)
Card: [`1112`](./batch-cards/1112-release-gate-diagnosability.md)
Contracts: [`035`](../../contracts/035-release-tag-identity-contract.md),
[`039`](../../contracts/039-pre-release-ci-proof-contract.md)
Guide: [`051`](../../guides/051-release-orchestration.md)

## Purpose

Make a failed release gate diagnosable from what Effigy leaves behind, instead
of from a terminal someone happened to keep open.

## Origin

Swallowtail (Effigy `v0.12.1+local.aafbd93`, 11 configured gates) lost two
authorized `release prepare --yes --check-gates` attempts to a `floor` gate
failure that retained only `gate floor failed` and exit `101`. Reproducing by
hand took about two hours and did not name the cause; the same gate passed in
a fresh worktree, so the failure is environment- or workspace-specific.
Effigy's own releases have the same blind spot.

## Decision

- persist every executed gate's full stdout/stderr and the run environment
  under `.effigy/reports/release/gates/`, written by the shared gate runner
- show the failing gate's tail and log path in prepare/execute text output
- announce the configured gate inventory and progress on stderr regardless of
  terminal state; JSON stdout stays clean
- keep the rollback invariant; keep-on-failure is a separate decision

## Cards

- [x] [`1112`](./batch-cards/1112-release-gate-diagnosability.md) — complete; PR `90` merged at `f1732c87`

## Acceptance

- a failed gate under a captured-stderr harness leaves a complete log and a
  redacted environment record on disk
- the text rollback summary names the log path and shows the failing tail
- `release gates` names its inventory before the first gate runs
- JSON schema ids and existing fields are unchanged; new fields are additive
- gate order, fail-fast, shell invocation, and rollback are unchanged

## Non-Goals

- keep-on-failure or any non-rollback prepare mode
- new flags, environment variables, or gate kinds
- release execution, tag mutation, or workflow edits
- Swallowtail-side changes

## Dispatch Manifest

Published for the coordinator at the promoting commit on `main`.

- **Lane:** card `1112`, roadmap `g09.004`, strict spec `119`. State: ready.
- **Prerequisites:** clean `main` at or after the promoting commit; no
  active strict lane. **Completion:** PR merged with the evidence log, card,
  roadmap, spec, and changelog closed out.
- **Owned mutable paths:** `crates/effigy-release/src/**`,
  `src/runner/release_command/**`, `crates/effigy-cli/src/help/topics/release.rs`,
  release tests under `src/tests/**` and `crates/effigy-release/src/tests.rs`,
  `docs/guides/051-release-orchestration.md`,
  `docs/guides/017-json-output-contracts.md`.
  **Reserved shared closeout surfaces:** `CHANGELOG.md` `[Unreleased]`,
  `docs/logs/2026-09/`, `docs/logs/README.md`, this roadmap, card `1112`,
  spec `119`, `docs/specs/README.md`, `docs/roadmaps/README.md`,
  `docs/roadmaps/g09/README.md`.
- **Concurrency:** no approved siblings; no serial edges. Single lane.
- **Worker capability class:** economical non-frontier day-to-day
  implementation worker.
- **Acceptance evidence and review oracle:** card `1112` acceptance and spec
  `119` whole-lane oracle; fixture-based tests, `effigy qa`, fmt, clippy,
  `git diff --check`; one dated evidence log.
- **Stop conditions and escalation owner:** spec `119` stop conditions.
  Planning questions escalate to the coordinator, then Chatterbox. Anything
  touching release execution or workflows escalates to the operator.

## Next Task

Card `1112` is complete. PR `90` merged at `f1732c87`. Notify the Swallowtail
Chatterbox that the fix is on `main` for local-install adoption.
