---
title: Release gate diagnosability worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: release gate runner, release text/JSON renders, release progress seam
created: 2026-09-05
updated: 2026-09-05
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260905-083700-release-gate-diagnosability-1112.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, g09.004, 1112]
---

## What This Thread Was Doing

The coordinator is dispatching the single approved implementation lane for
release-gate diagnosability. Card `1112` is the complete bounded repair; this
worker owns implementation, evidence, validation, and a reviewable PR.

This dispatches one bounded implementation lane. No transcript or second prompt
is part of the authority chain.

## Why It Matters

Release-gate failures currently retain too little diagnostic context. Persisting
the captured output and redacted environment, and surfacing a bounded failure
tail, lets operators diagnose a failed gate without rerunning it.

## Current State

Here is the state the worker is inheriting:

- **Repository:** `effigy` at `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `bc7a36f764ca435f4e93238d7d13dc5a17bd8765`
- **Pushed main verification:** clean `main`, `HEAD == origin/main` at the promoted commit
- **Planning checkout:** clean
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** manifest `g09.004`, card `1112`, spec `119`
- **Worker branch:** `worker/g09-004-release-gate-diagnosability-1112`
- **Worker worktree:** `/Users/tom/.paseo/worktrees/310mya31/g09-004-release-gate-diagnosability-1112`
- **Worktree creation command:** Paseo `create_workspace`; `isolation: worktree`, `mode: branch-off`, `baseBranch: origin/main`
- **Worker worktree policy:** follow `Completion Protocol`; launcher worktree first, named/manual fallback only when required.
- **Required sibling worktree links:** `none`
- **Active spec lane:** [`docs/specs/archive/119-release-gate-diagnosability-strict-lane.md`](../specs/archive/119-release-gate-diagnosability-strict-lane.md)
- **Roadmap milestone:** [`docs/roadmaps/g09/004-release-gate-diagnosability.md`](../roadmaps/g09/004-release-gate-diagnosability.md)
- **Ready cards, in order:** [`docs/roadmaps/g09/batch-cards/1112-release-gate-diagnosability.md`](../roadmaps/g09/batch-cards/1112-release-gate-diagnosability.md)
- **Allowed runway:** execute card `1112` only
- **Remaining card budget:** one card, one PR
- **Coordinator agent ID:** `0accca7b-4f0e-428a-b62c-b8755b32cc1c`
- **Delivery route:** coordinator-attached child with `notifyOnFinish: true`; the coordinator records scoped creation and returned child/workspace identity.
- **Dispatch topology:** sole approved ready-frontier lane
- **Parallel safety check:** no approved siblings; no serial edges; no shared mutable scope
- **Surfaces this lane owns:** `crates/effigy-release/src/**`, `src/runner/release_command/**`, `crates/effigy-cli/src/help/topics/release.rs`, release tests under `src/tests/**` and `crates/effigy-release/src/tests.rs`, `docs/guides/051-release-orchestration.md`, `docs/guides/017-json-output-contracts.md`
- **Integration ownership:** coordinator owns `CHANGELOG.md` `[Unreleased]`, `docs/logs/2026-09/`, `docs/logs/README.md`, the roadmap, card, spec, and planning front doors during closeout
- **Merge ordering:** same-repository PRs merge one at a time; the orchestrator refreshes this head against current `main` and re-reviews it if a sibling lane merges first
- **Canonical refs:** `docs/roadmaps/g09/004-release-gate-diagnosability.md`, `docs/roadmaps/g09/batch-cards/1112-release-gate-diagnosability.md`; `docs/contracts/001-working-rules.md`, `docs/contracts/039-pre-release-ci-proof-contract.md`
- **Review oracle:** card `1112` Review Oracle and spec `119` Whole-Lane Review Oracle
- **Model capability profile:** economical non-frontier day-to-day implementation worker (`Cursor Auto`)
- **Worker provider/model identity:** `cursor/default`
- **Frontier-worker justification:** `none`
- **Tool/runtime restrictions:** no new flags, environment variables, gate kinds, schema IDs, release execution, keep-on-failure path, workflow edits, or persistence outside `.effigy/reports/release/gates/`
- **Required validation:** focused release and JSON/help tests; `effigy graph affected` as applicable; `effigy qa`; `cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`; `git diff --check`
- **PR base/head:** current pushed `main` at `bc7a36f764ca435f4e93238d7d13dc5a17bd8765`; head pending
- **PR URL:** pending
- **Review state:** awaiting implementation and PR
- **Merge path:** orchestrator after accepted independent exact-head review and passing required checks

## Boundaries

Please keep this run inside the named runway:

- **In scope:** persist every executed gate's stdout/stderr and redacted run environment under `.effigy/reports/release/gates/`; add optional `log_path` and `environment_path`; show failed-gate tails and paths in prepare/execute text; always send progress to stderr; announce configured gate inventory; update the two named guides; add focused tests and implementation evidence.
- **Out of scope:** keep-on-failure; release execution; new flags, env vars, gate kinds, schema IDs, or changed rollback/invocation/order semantics; `.github/workflows/**`; persistence outside `.effigy/reports/release/gates/`; planning or consumer adoption.
- **Outcome shape:** smallest complete contract-valid repair with cleanup, validation, evidence, and a reviewable PR. Do not open a diagnostics-only PR.
- Do not invent architecture, change contracts, widen the roadmap, or choose an unresolved product/API/persistence/security decision.
- This handoff represents one worker lane. Write only inside **Surfaces this lane owns**. Leave closeout and front-door surfaces assigned to **Integration ownership** to the coordinator.
- Work only in the clean worker worktree selected by `Completion Protocol`.
- Do not merge the PR. Merge belongs to the orchestrator after its accepted review/check gate.

## Important Context

- **Planning lineage:** Swallowtail consumer intake dated 2026-09-05; operator-confirmed direction promoted at `bc7a36f764ca435f4e93238d7d13dc5a17bd8765`; roadmap `g09.004` -> spec `119` -> card `1112`.
- **Why this card is ready:** the operator approved this bounded single lane; the manifest names all owned surfaces, acceptance evidence, review oracle, validation, and stop conditions.
- **Decisions and preferences:** latest run wins; full gate output is retained; environment values matching `TOKEN`, `SECRET`, `KEY`, `PASSWORD`, or `CREDENTIAL` are `<redacted>`; JSON changes are additive and schema IDs stay unchanged; progress/inventory stay off JSON stdout.
- **Open tensions:** none within the approved envelope. Anything touching release execution or workflows escalates to the operator.
- **Report after:** each coherent implementation/test chunk, then the pushed PR and exact evidence.
- **Report to:** the owning coordinator through the linked child result. Do not require operator relay or message Chatterbox during automatic dispatch.

## Suggested Next Move

Run the `Completion Protocol` preflight before broad reads. Then read the active
milestone, assigned card, `AGENTS.md`, and canonical refs from the selected
worker worktree. Start the first coherent chunk. At a natural pause, report
what changed, validation run, what remains, and any blocker.

## Completion Protocol

Use the standard worker completion protocol in the Northstar orchestrator
handoff template. In particular: verify the clean registered worktree and
tracked handoff at the exact pushed base; execute only card `1112`; falsify all
seven review-oracle counterexamples; keep closeout surfaces for the coordinator;
push the branch; and open a reviewable PR without merging it.

The orchestrator will launch an independent reviewer in this same worker
workspace under a serial clean exact-head lease. The reviewer must use a
provider/model identity distinct from `cursor/default`, post a durable verdict
naming the exact head SHA, and make no tracked changes.

- **Closeout refs:** card `1112`, roadmap `g09.004`, spec `119`, one dated evidence log under `docs/logs/2026-09/`, `CHANGELOG.md`, and the named front doors

Before calling the runway complete, leave the card, roadmap, log, and next-task
state honest. The coordinator reconciles and closes those surfaces after merge.
