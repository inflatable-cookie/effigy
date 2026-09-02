---
title: Generated catalog baseline 1106 worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy generated catalog baseline
created: 2026-09-02
updated: 2026-09-02
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260902-141458-generated-catalog-baseline-1106.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, catalog-pack]
---

## What This Thread Was Doing

Effigy card `1105` published and proved the first official catalog pack.
Card `1106` is now the sole Ready dependency edge: replace Effigy's editable
embedded catalog source with an exact generated recovery snapshot and typed
provenance lock derived from that accepted artifact.

This dispatches one bounded implementation lane. No transcript or second prompt
is part of the authority chain.

## Why It Matters

The dedicated pack repository must become the only editable concrete-asset
authority without weakening Effigy's offline recovery floor. Exact generated
bytes, provenance identity, and drift checks make that ownership split
enforceable.

## Current State

Here is the state the worker is inheriting:

- **Repository:** `inflatable-cookie/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `2c547261d71c236d3237681557411a1b5bcf772b`
- **Pushed main verification:** local `HEAD` and `origin/main` both equal the
  planning base before this handoff commit
- **Planning checkout:** clean before this handoff
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** card `1105` Complete; card
  `1106` Ready with exact publication input; spec `115`, roadmap `g08.048`,
  contract `043`, and front doors reconciled
- **Worker branch:** `worker/g08-048-generated-catalog-baseline-1106`
- **Worker worktree:** `/Users/tom/Dev/worktrees/effigy-generated-catalog-baseline-1106`
- **Worktree creation command:** `git worktree add -b worker/g08-048-generated-catalog-baseline-1106 /Users/tom/Dev/worktrees/effigy-generated-catalog-baseline-1106 origin/main`
- **Worker worktree policy:** follow `Completion Protocol`; launcher worktree
  first, named/manual fallback only when required.
- **Required sibling worktree links:** link `effigy-catalog-pack`, source
  `/Users/tom/Dev/projects/effigy-catalog-pack`, destination beside this
  worktree as `../effigy-catalog-pack`. Create when absent; reuse only a
  symlink resolving to that source; stop on any other existing path.
- **Active spec lane:** `docs/specs/115-catalog-pack-publication-and-cutover-strict-lane.md`
- **Roadmap milestone:** `docs/roadmaps/g08/048-catalog-pack-publication-and-cutover.md`
- **Ready cards, in order:** `docs/roadmaps/g08/batch-cards/1106-cut-over-generated-catalog-baseline.md`
- **Allowed runway:** card `1106` only
- **Remaining card budget:** one card
- **Dispatch topology:** sole ready-frontier lane; cards `1107` and `1108`
  remain blocked on this merge
- **Parallel safety check:** serial dependency edge from accepted `1105`
  publication evidence to generated baseline; no sibling implementation lane
- **Surfaces this lane owns:** generated catalog snapshot and provenance lock;
  catalog embedding/validation code and focused tests; repository-owned import
  or drift-check tooling; directly related docs, changelog, card `1106`,
  milestone/spec closeout, evidence log, and next-task front doors
- **Integration ownership:** this worker owns card `1106` closeout. It must
  leave cards `1107` and `1108` blocked; the orchestrator refreshes their
  ready-frontier status after merge.
- **Merge ordering:** same-repository PRs merge one at a time; the orchestrator
  refreshes this head against current `main` and re-reviews it if a sibling lane
  merges first
- **Canonical refs:** `docs/architecture/026-feature-placement-and-command-surface.md`;
  `docs/contracts/043-feature-placement-and-surface-migration-contract.md`
- **Review oracle:** card `1106` Review Oracle and spec `115` Whole-Lane Review
  Oracle
- **Model capability profile:** ordinary bounded day-to-day implementation;
  use the cheapest adequate non-frontier route selected by the orchestrator
- **Frontier-worker justification:** none
- **Tool/runtime restrictions:** ordinary QA and normal Effigy use must remain
  offline. Explicit read-only online provenance proof may pull the named public
  digest and verify its attestation. Do not edit `.github/workflows/`, publish,
  tag, change package visibility, move registry pointers, or release Effigy.
- **Required validation:** focused `effigy-catalog` and CLI/integration tests;
  deterministic offline snapshot/lock drift counterexamples; explicit online
  digest, attestation, inventory, and byte comparison; `effigy qa`;
  `cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`;
  `git diff --check`
- **PR base/head:** current pushed `main` / worker branch above
- **PR URL:** pending
- **Review state:** awaiting implementation
- **Merge path:** orchestrator after accepted review of the current head and
  passing required checks

## Boundaries

Please keep this run inside the named runway:

- **In scope:** implement card `1106` completely from the exact publication
  input already recorded on the card.
- **Out of scope:** public `service pack update`; proposal automation; pack
  publication changes; workflows; Effigy release; S3 or extension transport;
  unrelated catalog behavior or command-surface changes.
- **Outcome shape:** smallest complete contract-valid generated-baseline cutover,
  validation, evidence, closeout, and PR. Do not stop at an import script or
  diagnostics-only result.
- Do not invent architecture, change contracts, widen the roadmap, or choose an
  unresolved product/API/persistence/security decision.
- This handoff represents one worker lane. Write only inside **Surfaces this
  lane owns**. Leave future-card readiness promotion to **Integration ownership**.
- Work only in the clean worker worktree selected by `Completion Protocol`.
  Never edit the planning checkout or an unrelated dirty checkout.
- Do not merge the PR. Merge belongs to the orchestrator after its accepted
  review/check gate.

## Important Context

- **Planning lineage:** architecture `026` -> contract `043` -> spec `115` ->
  roadmap `g08.048` -> card `1106`; accepted external evidence is catalog-pack
  PR `#4`, merge `7427421a3bebf207ce9979c47f60609d1b276713`.
- **Why this card is ready:** public `v1.0.1` and `stable` resolve to OCI digest
  `sha256:91de584e77487765c24f53abb63413783a99c0a7926c25aee1289a3cf370d9f3`;
  source and unpacked identities, attestation, anonymous exact-byte pull, and
  package linkage have accepted evidence.
- **Decisions and preferences:** canonical editable bytes remain in the pack
  repository. Effigy keeps only a generated, pinned, offline recovery snapshot.
  Content identity and OCI manifest identity stay distinct.
- **Open tensions:** choose the smallest typed lock and generation/check seam
  that makes manual edits fail without adding network access to ordinary QA.
  Stop if exact reproduction or permanent offline embedding cannot hold.
- **Report after:** generated snapshot/lock and both offline/online proofs are
  coherent, or at the first stop condition.
- **Report to:** the operator, who will relay progress to the orchestrator.

## Suggested Next Move

Run the `Completion Protocol` preflight before broad reads. Then inspect the
existing `rust-embed` catalog boundary and the pack repository's import and
identity proofs. Design the typed lock and deterministic generation/check path,
then implement one coherent batch.

## Completion Protocol

### Before you start

1. This handoff's `worker_mode: implementation` and
   `dispatch_authority: orchestrator` activate worker mode. Before broad reads,
   run `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, its status is empty, and its
   branch is not `main`, accept it as the launcher-provided worktree. Record its
   actual root/branch; do not compare its generated path/branch with the planned
   path or create another worktree merely because they differ.
3. If current context is `main`, dirty, unregistered, or unusable, inspect the
   named worktree. If unusable, read `.agents.local.env`, require
   `AGENTS_WORKTREE_CONTAINER_DIR`, and ask the operator when absent. Create a
   unique worktree/branch there from pushed `origin/main`. Never use `/tmp`,
   `TMPDIR`, or a guessed path; never clean, reset, stash-over, or discard dirty
   state. Report a launcher-supplied dirty or `main` worktree instead of creating
   another.
4. From the selected worktree, record this handoff's repository-relative path.
   Run `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Confirm `HEAD == origin/main`, confirm
   `git merge-base --is-ancestor 2c547261d71c236d3237681557411a1b5bcf772b HEAD`,
   and confirm the relative handoff exists in `HEAD`. Load it with `git show`.
   If the absolute dispatch file differs from that tracked blob, stop. The
   committed `HEAD` copy is canonical.
5. Verify the required sibling link after the launcher lifecycle has run. It
   must be a symlink named `effigy-catalog-pack` in the worktree container
   directory resolving to `/Users/tom/Dev/projects/effigy-catalog-pack`. Stop
   on absence or mismatch; never replace an existing path.
6. Read the active milestone, assigned card, `AGENTS.md`, and canonical refs.
7. Run the repository's cheap orientation checks and record what you ran.

### While you work

- Execute only card `1106`; keep commits aligned with meaningful chunks.
- Preserve the imported artifact bytes and exact identities. Remove temporary
  diagnostics before review unless the governing refs require durable evidence.
- Report at the named meaningful chunk with changed files, validation actually
  run, remaining work, new risks, and blockers.
- Stop if a contract is missing, intent is ambiguous, scope expands,
  authority/access is missing, or validation changes the plan.

### When the assigned runway is complete

1. Run all required validation named in **Current State**.
2. Falsify every card/oracle claim: hand-edit snapshot bytes, lock fields,
   manifest version, and content identity in isolated fixtures; prove ordinary
   QA stays offline; prove the public digest's attestation, inventory, and bytes
   match; prove baseline/bootstrap/layering behavior without installed state.
3. Add one dated evidence log mapping each row to exact proof. Close card `1106`
   and its milestone/spec state honestly, but leave cards `1107` and `1108`
   blocked for orchestrator post-merge readiness refresh.
4. Push the selected worker branch. If another lane merged first, incorporate
   current `main`, rerun validation, and report the changed head.
5. Open a reviewable PR against current pushed `main`.
6. In the PR body, link the handoff, card, milestone, spec, changed surfaces,
   evidence, validation, and unresolved items.
7. Report the PR URL and exact head. Do not merge.

### Review and merge path

The orchestrator independently reviews the exact head and posts its verdict on
the PR. Requested changes return to this worker. Accepted current head plus
passing checks and mergeability permits orchestrator merge without another
operator prompt.

- **Closeout refs:** card `1106`, roadmap `g08.048`, spec `115`, dated evidence
  log, changelog, and every active `Next Task` front door.

### Handoff closeout

Before calling the runway complete, leave the card, roadmap, log, and next-task
state honest. If blocked, record the blocker and stop rather than making the
handoff look complete.
