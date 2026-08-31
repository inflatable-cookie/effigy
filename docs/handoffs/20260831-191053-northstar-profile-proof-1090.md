---
title: Generic and Northstar documentation profile proof worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / documentation graph orchestrator
created: 2026-08-31
updated: 2026-08-31
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260831-191053-northstar-profile-proof-1090.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, documentation-graph]
---

## What This Thread Was Doing

The repository-defined documentation graph lane has shipped profile-aware
indexing and the bounded `effigy docs context` query. PR 62 closed card `1089`
and made card `1090` the sole ready continuation.

This dispatches card `1090`: prove the runtime remains repository-neutral,
publish Northstar as committed consumer configuration, measure retrieval
quality, and close the lane. No transcript or second prompt is part of the
authority chain.

## Why It Matters

The query is useful only if repositories own their semantics and an installed
skill never becomes hidden runtime authority. This final proof must show that a
generic consumer works with arbitrary vocabulary, that Northstar adoption is a
copied configuration asset, and that live authority wins representative
retrieval without suppressing directly requested history.

## Current State

Here is the state the worker is inheriting:

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `f9748c5170cafbbe5e0b85686b1ddf9db245e55b`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  `f9748c5170cafbbe5e0b85686b1ddf9db245e55b` before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** card `1089` closeout and PR 62
  merge, including `effigy.docs.context.v1` and its evidence log.
- **Worker branch:** intended `worker/g08-035-profile-proof-1090`; accept the
  launcher's clean non-`main` branch when one is supplied.
- **Worker worktree:** intended
  `/Users/tom/Dev/worktrees/effigy-profile-proof-1090`; accept the launcher's
  clean registered worktree when one is supplied.
- **Worktree creation command:** orchestrator-owned. Do not create a second
  worktree merely because its path differs from the intended name.
- **Worker worktree policy:** follow `Completion Protocol`; launcher worktree
  first, named/manual fallback only when required.
- **Required sibling worktree links:** none.
- **Active spec lane:**
  `/Users/tom/Dev/projects/effigy/docs/specs/108-documentation-graph-profiles-strict-lane.md`
- **Roadmap milestone:**
  `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/035-repository-defined-documentation-graph.md`
- **Ready cards, in order:** only
  `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/batch-cards/1090-prove-generic-and-northstar-profiles.md`
- **Allowed runway:** card `1090` only: generic proof, Northstar starter and
  consumer adoption, predeclared benchmark, documentation, validation, and
  lane closeout.
- **Remaining card budget:** one card. Stop after evidence-backed lane closeout;
  do not infer release work or a new roadmap.
- **Dispatch topology:** this implementation lane may run beside the isolated
  feature-boundary planning delegate because that delegate owns only its named
  triage/research packet and cannot promote or implement.
- **Parallel safety check:** do not edit
  `docs/triage/20260831-181909-command-surface-and-runtime-boundary-audit.md`,
  its planning-delegate handoff, or any feature-boundary audit packet. Stop if
  the audit PR unexpectedly touches card `1090` surfaces.
- **Canonical refs:** architecture `024`; contracts `001` and `041`; strict
  spec `108`; roadmap `g08.035`; card `1090`; card `1089` evidence.
- **Review oracle:** repository-neutral runtime, consumer-owned configuration,
  installed-skill independence, and predeclared retrieval quality as detailed
  below.
- **Model capability profile:** frontier coding model with high reasoning; this
  batch crosses public starter configuration, runtime authority, and exact
  benchmark claims.
- **Tool/runtime restrictions:** use the project-local Effigy skill and the
  repository's Northstar Rust everyday-authoring route. Do not edit
  `.github/workflows/`, run release mutations, add runtime skill lookup, or add
  Northstar-specific branches to generic runtime code.
- **Required validation:** focused manifest/codegraph/CLI/built-in/starter
  tests; documentation links, generated reference, command matrix, and JSON
  examples; benchmark replay over Effigy and the generic fixture; `effigy qa`;
  `cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`;
  `git diff --check`; and changed-file affected analysis through `effigy graph`.
- **PR base/head:** `main` to `worker/g08-035-profile-proof-1090`, or the actual
  launcher-provided worker branch.
- **PR URL:** pending.
- **Review state:** awaiting independent orchestrator review.
- **Merge path:** orchestrator after accepted review of the current head and
  passing required checks.

## Boundaries

Please keep this run inside card `1090`:

- **In scope:** preserve one arbitrary-vocabulary non-Northstar fixture; add the
  Northstar graph profile to the shipped consumer starter and the committed
  Effigy consumer configuration needed for replay; document explicit copying
  and explicit later updates; add representative architecture, contract,
  current-roadmap, next-task, and historical-decision queries; predeclare and
  run the benchmark; prove installed-skill independence; update ownership and
  adoption docs, changelog, evidence, card/spec/roadmap/front doors; archive
  spec `108` only after every acceptance item passes.
- **Out of scope:** command-surface/runtime-boundary audit promotion; new graph
  ranking semantics unless the declared benchmark exposes a contract defect;
  implicit profile inheritance; skill lookup at runtime; embeddings or model
  inference; external crawling; workflow edits; release preparation or
  execution; unrelated papercut fixes.
- **Outcome shape:** a reviewable implementation PR with reproducible proof and
  an honest closed lane. Do not stop at a benchmark report while contracted
  starter, adoption, validation, and closeout work remains possible.
- Consumer `effigy.toml` content is the only runtime authority. A starter or
  skill may originate bytes, but installation and future updates are explicit
  materialization operations.
- Keep generic runtime vocabulary generic. No Northstar path, field, status,
  kind, or relation token may enter generic manifest/codegraph/query logic.
- Freeze the benchmark corpus, expected live authorities, historical rivals,
  and pass criteria before reading its results. Do not tune queries or weights
  after the fact merely to make the table green; record and diagnose misses.
- Review-oracle counterexamples:
  1. A generic fixture renames every Northstar-looking token yet still indexes
     and queries correctly without runtime edits.
  2. The copied Northstar profile produces byte-equivalent results when skill
     directories are unavailable to the process.
  3. For every predeclared live-authority query, the expected live source is in
     the top three and a related historical-only source does not rank above it.
  4. A query that directly names a historical decision still retrieves that
     historical evidence.
  5. Changing the installed template after copying does not silently reinterpret
     the consumer profile.
- Preserve unrelated work and use only the selected clean worker worktree. Do
  not clean, reset, stash over, or edit another checkout.
- Follow `PAPERCUTS.md`. Record incidental solvable friction without widening
  this card to fix it.
- Do not merge the PR. Merge belongs to the orchestrator after its accepted
  review/check gate.

## Important Context

- **Planning lineage:** architecture `024` and contract `041` place all runtime
  semantics in the selected repository manifest. Spec `108` sequences cards
  `1088` through `1090`; `1088` proved generic structural records and freshness,
  and `1089` proved bounded retrieval and exact provenance.
- **Why this card is ready:** PR 62 merged at `f9748c517` with accepted exact-head
  review and all required checks green. Every active `Next Task` now points to
  ready card `1090`.
- **Decisions and preferences:** Northstar is one profile, not a built-in
  ontology. Starter installation copies configuration; it does not create an
  ongoing runtime dependency. Benchmark evidence must be reproducible and must
  preserve direct historical discovery.
- **Open tensions:** the repository may not yet have a canonical benchmark
  harness or one obvious starter asset. Use existing docs-policy/init ownership
  and the smallest durable test/data surface. Stop for planning only if two
  competing authorities cannot be resolved from architecture `024`, contract
  `041`, and current init/adoption docs.
- **Report after:** starter/profile and independence proof are green; then after
  the predeclared benchmark; then after full closeout validation, or immediately
  on a stop condition.
- **Report to:** the operator/orchestrator through this worker thread.

## Suggested Next Move

Run the `Completion Protocol` preflight before broad reads. Then read
`AGENTS.md`, `PAPERCUTS.md`, card `1090`, spec `108`, roadmap `g08.035`,
architecture `024`, contracts `001` and `041`, and card `1089` evidence from the
selected worktree. Use `effigy graph` to locate current starter/init,
docs-policy, query-fixture, and generated-doc ownership. Write down the
benchmark matrix and expected sources before executing it.

## Completion Protocol

### Before you start

1. This handoff's `worker_mode: implementation` and
   `dispatch_authority: orchestrator` activate worker mode. Before broad reads,
   run `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, its status is empty, and its
   branch is not `main`, accept it as the launcher-provided worktree. Record its
   actual root/branch; do not compare its generated path or branch with the
   intended values or create another worktree merely because they differ.
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
   `git merge-base --is-ancestor f9748c5170cafbbe5e0b85686b1ddf9db245e55b HEAD`,
   and confirm this relative handoff path exists in selected `HEAD`. Load it with
   `git show HEAD:docs/handoffs/20260831-191053-northstar-profile-proof-1090.md`.
   If the absolute dispatch file differs from that tracked blob, stop. The
   committed `HEAD` copy is canonical.
5. Required sibling worktree links are `none`; skip link creation.
6. Read the active milestone, card, `AGENTS.md`, and canonical refs.
7. Run only the cheap Effigy orientation checks relevant to locating the named
   starter/profile and validation surfaces; record what you actually ran.

### While you work

- Execute only card `1090`. Keep commits aligned with coherent proof,
  adoption, and closeout chunks rather than model turns.
- Preserve the predeclared benchmark before seeing results. If results miss,
  diagnose within contract boundaries; stop if success would require a new
  ranking rule, hidden skill dependency, or Northstar hard-coding.
- After each meaningful chunk, report changed files, validation actually run,
  remaining acceptance items, risks, and blockers.
- Stop if a contract is missing, authority is ambiguous, scope expands, the
  generic fixture requires Northstar runtime tokens, skill absence changes
  results, or validation changes the plan.

### When the assigned runway is complete

1. Run the required final validation listed in `Current State` and card `1090`.
2. Try to falsify the diff against each review-oracle counterexample. Map every
   universal, exact, and negative claim to concrete proof. Reconcile card,
   roadmap, spec/archive, log, front doors, and the single next-task state.
3. Write one dated evidence log with the frozen benchmark corpus and expected
   sources, actual ranks and context bytes, current-versus-historical results,
   arbitrary-vocabulary generic proof, copied-profile proof, installed-skill
   independence proof, validation output, and residuals.
4. Mark card `1090` complete and close roadmap `g08.035`. Archive spec `108`
   only after acceptance is complete. Remove stale ready-card pointers and leave
   one honest post-lane planning checkpoint; do not infer release work.
5. Update `CHANGELOG.md`, architecture/package ownership and adoption docs, and
   all required generated/reference surfaces.
6. Push the selected worker branch and open a reviewable PR against current
   `main`. The planning base predates this handoff commit and is an ancestor
   check, not the PR base SHA.
7. In the PR body, link spec `108`, roadmap `g08.035`, card `1090`, architecture
   `024`, contract `041`, card `1089` and new evidence logs, changed surfaces,
   validation, benchmark results, and unresolved items.
8. Report the PR URL, exact head SHA, and evidence. Do not merge.

### Review and merge path

The orchestrator will review the PR against the canonical refs, full diff,
benchmark proof, and checks. Current review state: awaiting independent
orchestrator review.

The orchestrator records its verdict on the PR. Because worker and orchestrator
share the GitHub identity, use a PR comment rather than formal self-approval. If
changes are requested, make only those changes on this branch, push, and report
the new exact head. Blocking findings use `execution-miss`, `oracle-gap`,
`planning-change`, `validation-gap`, or `integration-drift`; a planning change
returns to planning before revision. Requested changes: none. Merge occurs only
after an accepted verdict for the unchanged head, passing required checks,
mergeability into `main`, and no operator pause.

- **Closeout refs:** card `1090`; roadmap `g08.035`; spec `108`; architecture
  `024`; contract `041`; new dated log; roadmap/spec/log/front-door currentness;
  `CHANGELOG.md`.

### Handoff closeout

Before calling the runway complete, leave the card, roadmap, spec/archive, log,
front doors, and next-task state honest. If blocked, record the blocker and stop
instead of making the lane look complete.
