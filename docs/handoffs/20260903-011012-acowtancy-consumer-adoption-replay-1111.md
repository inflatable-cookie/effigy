---
title: Acowtancy consumer adoption replay worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy consumer-contract evidence worker
created: 2026-09-03
updated: 2026-09-03
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260903-011012-acowtancy-consumer-adoption-replay-1111.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, consumer-evidence]
---

## What This Thread Was Doing

The Effigy orchestrator completed the second vision governance review. The
operator kept maturity at Stage 2, selected Theme 3, and chose Acowtancy as the
first current consumer replay. Planning compiled one ready card that collects
evidence without taking ownership of Acowtancy work.

This dispatches one bounded implementation lane. No transcript or second prompt
is part of the authority chain.

## Why It Matters

Effigy's consumer contract is mature on paper but needs a current non-fixture
replay and the first populated comparison scorecard. Acowtancy exercises a real
workspace/docs-authority shape and recent nested-catalog behavior. The replay
must show whether Effigy's portable guidance still matches that repository
without turning consumer-local differences into product changes.

## Current State

Here is the state the worker is inheriting:

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `adbad9924282a2b515bf34463559bc580e689e5f`
- **Pushed main verification:** local `HEAD` and `origin/main` both equal
  `adbad9924282a2b515bf34463559bc580e689e5f`
- **Planning checkout:** clean after the planning commit and push
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** governance cycle-two log,
  D-2026-05, strict spec `118`, `g09.003`, and ready card `1111`
- **Worker branch:** `worker/g09-003-acowtancy-consumer-replay-1111`
- **Worker worktree:** `/Users/tom/.paseo/worktrees/310mya31/g09-003-acowtancy-consumer-replay-1111`
- **Worktree creation command:** Paseo-managed `branch-off` worktree from
  `origin/main`; no manual command is authorized unless launcher fallback is
  required by the Completion Protocol
- **Worker worktree policy:** follow `Completion Protocol`; launcher worktree
  first, named/manual fallback only when required.
- **Required sibling worktree links:**
  - `acowtancy`: source `/Users/tom/Dev/projects/acowtancy`; path beside this
    worktree `/Users/tom/.paseo/worktrees/310mya31/acowtancy`
  - `underlay`: source `/Users/tom/Dev/projects/underlay`; path beside this
    worktree `/Users/tom/.paseo/worktrees/310mya31/underlay`
  - `poodle`: source `/Users/tom/Dev/projects/poodle`; path beside this
    worktree `/Users/tom/.paseo/worktrees/310mya31/poodle`
- **Active spec lane:** `docs/specs/118-acowtancy-consumer-adoption-replay-strict-lane.md`
- **Roadmap milestone:** `docs/roadmaps/g09/003-acowtancy-consumer-adoption-replay.md`
- **Ready cards, in order:**
  `docs/roadmaps/g09/batch-cards/1111-acowtancy-consumer-adoption-replay.md`
- **Allowed runway:** frozen Acowtancy replay, ownership classification,
  Effigy/Acowtancy scorecard, and only directly proved generic Effigy
  starter/guide reconciliation
- **Remaining card budget:** one card
- **Dispatch topology:** sole ready-frontier lane
- **Parallel safety check:** Acowtancy and its active workers remain read-only
  dependencies; no sibling Effigy implementation lane is active
- **Surfaces this lane owns:** a new scorecard under
  `docs/vision/governance/`, one evidence log under `docs/logs/2026-09/`, card
  `1111`, `g09.003`, spec `118`, necessary Effigy front-door closeout, and only
  frozen-replay-proved edits to guide `056` or the Northstar starter
- **Integration ownership:** this sole lane owns its bounded Effigy closeout;
  it owns no Acowtancy surface
- **Merge ordering:** same-repository PRs merge one at a time; the orchestrator
  refreshes this head against current `main` and re-reviews it if a sibling lane
  merges first
- **Canonical refs:** `docs/vision/013-cross-repo-vision-adoption-playbook-v1.md`,
  `docs/vision/016-cross-repo-rollout-comparison-scorecard-template-v1.md`,
  `docs/vision/decisions/D-2026-05-consumer-adoption-cohort-replay.md`;
  `docs/guides/056-northstar-effigy-consumer-repo-contract.md`,
  `docs/contracts/001-working-rules.md`
- **Review oracle:** card `1111` `## Review Oracle` and spec `118`
  `## Whole-Lane Review Oracle`
- **Model capability profile:** economical non-frontier day-to-day worker for a
  bounded evidence and documentation lane
- **Frontier-worker justification:** none
- **Tool/runtime restrictions:** never modify, fetch, switch, reset, clean,
  hydrate, start, or apply anything in Acowtancy; do not start containers,
  managed sessions, secrets, state, installs, workflows, or releases
- **Required validation:** frozen identities and pre/post Acowtancy status;
  `effigy tasks`; `effigy doctor`; `effigy test --plan`;
  `effigy docs/qa:docs`; `effigy docs/qa:northstar`; scorecard evidence review;
  Effigy `effigy qa:docs`; `git diff --check`; `effigy doctor --json`; focused
  recurrence proof only if a machine-owned starter changes
- **PR base/head:** `main` / `worker/g09-003-acowtancy-consumer-replay-1111`
- **PR URL:** pending
- **Review state:** awaiting worker implementation and PR
- **Merge path:** orchestrator after accepted review of the current head and
  passing required checks

## Boundaries

Please keep this run inside the named runway:

- **In scope:** frozen Acowtancy replay, ownership classification,
  Effigy/Acowtancy scorecard, and only directly proved generic Effigy
  starter/guide reconciliation
- **Out of scope:** every Acowtancy edit or active card; runtime/container,
  dependency, secret, state, generated-artifact, or workaround mutation; Effigy
  product code; a second consumer; release/workflow work; S3/provider extraction;
  universal compatibility claims
- **Outcome shape:** evidence-first completion with the smallest directly proved
  Effigy documentation/starter repair, if any. A green replay with no product
  edit is a valid outcome; a consumer-owned failure is evidence, not repair
  authority.
- Do not invent architecture, change contracts, widen the roadmap, or choose an
  unresolved product/API/persistence/security decision.
- This handoff represents one worker lane. Write only inside **Surfaces this
  lane owns**. If shared mutable scope, a hidden dependency, or another lane's
  write appears, stop and report it through the active control plane or the
  operator instead of resolving it yourself.
- Work only in the clean worker worktree selected by `Completion Protocol`.
  Never edit the planning checkout or an unrelated dirty checkout.
- Do not merge the PR. Merge belongs to the orchestrator after its accepted
  review/check gate.

## Important Context

- **Planning lineage:** governance cycle two kept Effigy at Stage 2 and opened
  D-2026-05 -> strict spec `118` -> `g09.003` -> ready card `1111`.
- **Why these cards are ready:** the pilot, exact consumer SHA, command matrix,
  ownership taxonomy, allowed write surfaces, validation, evidence, stop
  conditions, and adversarial review oracle are all fixed.
- **Decisions and preferences:** Acowtancy was operator-selected. Its current
  `main` was clean at
  `e42b64b17cae15ed419872ccb9bdfc48861d5214`; it already exposes
  `docs/qa:docs` and `docs/qa:northstar`. Keep unknown scorecard dimensions
  unknown. One pilot is not portfolio proof.
- **Open tensions:** the consumer checkout may move or become dirty before
  replay; stop rather than changing it. The retained child-catalog workaround
  remains until Acowtancy-owned downstream revalidation. Current tasks may
  reveal environmental prerequisites; record and stop instead of starting them.
- **Report after:** the frozen command matrix and ownership classification are
  complete, before making any optional Effigy starter/guide edit
- **Report to:** the active control plane; the orchestrator retains review and
  merge authority

## Suggested Next Move

Run the `Completion Protocol` preflight before broad reads. Then read
`AGENTS.md`, spec `118`, `g09.003`, card `1111`, and the canonical refs from the
selected Effigy worktree. Verify all three required sibling links and the frozen
Acowtancy identity before running any consumer command. Report the matrix before
deciding whether the evidence permits an Effigy guidance repair.

## Completion Protocol

### Before you start

1. This handoff's `worker_mode: implementation` and
   `dispatch_authority: orchestrator` activate worker mode. Before broad reads,
   run: `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, its status is empty, and its
   branch is not `main`, accept it as the launcher-provided worktree. Record its
   actual root/branch; do not compare its generated path/branch with the planned
   values or create another worktree merely because they differ.
3. If current context is `main`, dirty, unregistered, or unusable, inspect the
   named worktree. If unusable, read `.agents.local.env`, require
   `AGENTS_WORKTREE_CONTAINER_DIR`, and ask the operator when absent. Create a
   unique worktree/branch there from pushed `origin/main`. Never use `/tmp`,
   `TMPDIR`, or a guessed path; never clean, reset, stash-over, or discard dirty
   state. Report a launcher-supplied dirty or `main` worktree instead of creating
   another.
4. From the selected worktree, record this handoff's repository-relative path.
   Run `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch
   origin`. Confirm `HEAD == origin/main`, confirm
   `git merge-base --is-ancestor adbad9924282a2b515bf34463559bc580e689e5f HEAD`,
   and confirm the relative handoff exists in `HEAD`. Load it with `git show`;
   if the absolute dispatch file differs, stop. The committed `HEAD` copy is
   canonical.
5. Verify the required `acowtancy`, `underlay`, and `poodle` sibling links.
   Canonicalize source and destination; require each source directory; create a
   destination only when absent; reuse only a symlink resolving to the declared
   source. Stop on a missing source, mismatch, directory, or file. Never delete,
   replace, overwrite, or skip a listed dependency.
6. Read the active milestone, assigned card, `AGENTS.md`, and canonical refs.
7. Run the repo's cheap orientation checks and record what you actually ran.

### While you work

- Execute card `1111` as one coherent lane. Keep commits aligned with meaningful
  evidence and reconciliation chunks, not model turns.
- Do not change Acowtancy. Verify its exact SHA and clean state again after every
  consumer command batch.
- After the command matrix, report through the active control plane with
  outcomes, ownership classifications, potential Effigy-owned drift, and any
  blocker before changing guide `056` or starter assets.
- Stop if a contract is missing, intent is ambiguous, scope expands,
  authority/access is missing, or validation changes the plan.

### When the assigned runway is complete

1. Run the required final validation listed in **Current State**.
2. Try to falsify the diff against card `1111`. Exercise every oracle
   counterexample and map each row to proof. Return any new contract choice or
   universal claim to planning.
3. Update card, roadmap, spec, evidence log, scorecard, and front doors with the
   honest state. Do not edit Acowtancy.
4. Push the selected worker branch. If another Effigy lane merged first, refresh
   against current `main`, re-run validation, and say so in the PR.
5. Open a reviewable PR against current pushed `main`.
6. In the PR body, link spec `118`, `g09.003`, card `1111`, the scorecard,
   evidence, validation, and unresolved items.
7. Report the PR URL and exact head. Do not merge.

### Review and merge path

The orchestrator will review the PR against the canonical refs, diff, and
checks. Current review state: awaiting worker implementation and PR.

The orchestrator records its verdict on the provider. If changes are requested,
make only those changes on this branch, push again, and notify the orchestrator.
Blocking findings use `execution-miss`, `oracle-gap`, `planning-change`,
`validation-gap`, or `integration-drift`; a `planning-change` returns to
planning before revision. Requested changes are: none. Merge remains with the
orchestrator after accepted exact-head review, passing checks, and mergeability.

- **Closeout refs:** card `1111`, `g09.003`, spec `118`, scorecard, one evidence
  log, and current contracts/roadmaps/specs/logs/vision front doors

### Handoff closeout

Before calling the runway complete, leave the card, roadmap, log, and next-task
state honest. If blocked, record the blocker and stop rather than making the
handoff look complete.
