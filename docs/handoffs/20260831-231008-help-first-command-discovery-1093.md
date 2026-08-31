---
title: Help-first command discovery 1093 worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy CLI command inventory, help parser/rendering, and public documentation
created: 2026-08-31
updated: 2026-08-31
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260831-231008-help-first-command-discovery-1093.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr]
---

## What This Thread Was Doing

The operator asked for an ownership-first audit of Effigy's growing command and
dependency surface. The audit promoted architecture `026` and contract `043`.
The operator then chose the first bounded migration: group discovery under
`effigy help`, keep execution grammar unchanged, and use exact topics `work`,
`local`, `repo`, `deliver`, `extend`, and `admin`.

This dispatches card `1093`, the complete help-first implementation and
closeout lane. No transcript or second prompt is part of the authority chain.

## Why It Matters

Effigy's useful built-in surface is hard to scan as one flat list. Help-first
grouping makes the surface navigable without reserving new top-level command
names, lengthening executable commands, or stealing repository task selectors.
It is the first concrete boundary improvement from the feature-placement audit.

## Current State

Here is the state the worker is inheriting:

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `00bd0870f151693429264907b756bb289937c6bf`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  `00bd0870f151693429264907b756bb289937c6bf` before this handoff commit
- **Planning checkout:** clean before this handoff was created
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** architecture `026`, contract
  `043`, roadmap `g08.038`, strict spec `111`, ready card `1093`, updated front
  doors, and the reduced open-design note
- **Worker branch:** intended
  `worker/g08-038-help-first-command-discovery-1093`; accept the launcher branch
  under the Completion Protocol
- **Worker worktree:** launcher-provided Paseo worktree; named manual fallback
  `/Users/tom/Dev/worktrees/effigy-help-first-command-discovery-1093`
- **Worktree creation command:** Paseo branch-off workspace from `origin/main`;
  only use `git worktree add -b worker/g08-038-help-first-command-discovery-1093 /Users/tom/Dev/worktrees/effigy-help-first-command-discovery-1093 origin/main`
  when the Completion Protocol requires the named fallback
- **Worker worktree policy:** follow `Completion Protocol`; launcher worktree
  first, named/manual fallback only when required.
- **Required sibling worktree links:** none
- **Active spec lane:**
  `docs/specs/111-help-first-command-discovery-strict-lane.md`
- **Roadmap milestone:**
  `docs/roadmaps/g08/038-help-first-command-discovery.md`
- **Ready cards, in order:**
  `docs/roadmaps/g08/batch-cards/1093-add-help-first-command-discovery.md`
- **Allowed runway:** card `1093` only: typed help-group ownership, grouped
  general help, group and command topics, collision/deferral proof, docs,
  validation, evidence, and closeout
- **Remaining card budget:** one card
- **Dispatch topology:** serial single-worker lane
- **Parallel safety check:** no sibling lane or shared mutable implementation
  scope; this worker owns the complete card and PR
- **Canonical refs:** `docs/architecture/026-feature-placement-and-command-surface.md`;
  `docs/contracts/001-working-rules.md` and
  `docs/contracts/043-feature-placement-and-surface-migration-contract.md`
- **Review oracle:** card `1093` `## Review Oracle`, all seven cases
- **Model capability profile:** Opus Worker — complex worker handoff, high
  reasoning, full implementation ownership
- **Tool/runtime restrictions:** use project-local Effigy skill and repo-owned
  selectors; do not edit `.github/workflows/`, mutate releases, or modify
  unrelated user work
- **Required validation:** focused `effigy-cli`, parser, help, output,
  inventory, deferral, generated-reference, and docs-coverage tests;
  `effigy qa`; `cargo fmt --all -- --check`;
  `cargo clippy --all-targets -- -D warnings`; `git diff --check`
- **PR base/head:** `main` to the actual launcher branch, intended
  `worker/g08-038-help-first-command-discovery-1093`
- **PR URL:** pending
- **Review state:** awaiting implementation and PR, then independent
  orchestrator review of the exact head
- **Merge path:** orchestrator after accepted review of the current head and
  passing required checks

## Boundaries

Please keep this run inside the named runway:

- **In scope:** implement and close card `1093` exactly as bounded by strict
  spec `111` and contract `043`.
- **Out of scope:** executable `effigy <group> <command>` aliases; new
  top-level built-in names; direct-command or selector-routing changes; alias
  warning, hiding, deprecation, or removal; catalog-pack acquisition;
  release/distribution separation; S3/Rhai provider extraction; plugin
  transport; workflow or release mutation.
- **Outcome shape:** implementation. Own the smallest complete contract-valid
  change, cleanup, validation, evidence, front-door closeout, and PR creation.
  Diagnostics alone do not complete this lane.
- Do not invent architecture, change contracts, widen the roadmap, or choose an
  unresolved product/API/persistence/security decision.
- This handoff represents one worker lane. Do not edit another lane's assigned
  scope; if shared mutable scope or a hidden dependency appears, stop and report
  it through the active control plane or the operator.
- Work only in the clean worker worktree selected by `Completion Protocol`.
  Never edit the planning checkout or an unrelated dirty checkout.
- Do not merge the PR. Merge belongs to the orchestrator after its accepted
  review/check gate.

## Important Context

- **Planning lineage:** the feature-boundary audit found about thirty public
  top-level families. Architecture `026` classifies ownership; contract `043`
  now fixes the help-first grammar and exact primary taxonomy. This lane is the
  first implementation slice from that audit.
- **Why these cards are ready:** the operator approved discovery-only scope and
  all six topic names. The contract fixes every primary home, negative routing
  boundary, unknown-topic behavior, and deferral invariant. Card `1093` adds an
  adversarial seven-case review oracle.
- **Decisions and preferences:** usage must stay as simple or simpler than
  today. General help becomes grouped. `effigy help <group>` discovers a family.
  `effigy help <command>` exposes current detail. Existing direct commands are
  still the only built-in execution routes. Binary weight is not a goal;
  ownership and operator coherence are.
- **Open tensions:** current `effigy help docs` silently falls back to general
  help; replace that with typed command detail. General help and command
  descriptors have separate-looking owners; converge only enough to guarantee
  one primary group per entry. `bootstrap`, `demo`, and `secrets` are
  cross-cutting but their primary homes are already fixed by contract `043`.
- **Report after:** the implementation, focused proof, documentation/closeout,
  and full validation form one complete PR-ready chunk; report earlier only for
  a stop condition or material planning conflict.
- **Report to:** the operator, who will relay progress to the orchestrator.

## Suggested Next Move

Run the `Completion Protocol` preflight before broad reads. Then read
`AGENTS.md`, roadmap `g08.038`, card `1093`, strict spec `111`, architecture
`026`, and contracts `001` and `043` from the selected worker worktree. Inspect
the existing command descriptor and help owners, then implement the smallest
typed inventory seam that can prove the full oracle without execution-routing
changes.

## Completion Protocol

### Before you start

1. This handoff's `worker_mode: implementation` and
   `dispatch_authority: orchestrator` activate worker mode. Before broad reads,
   run: `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, its status is empty, and its
   branch is not `main`, accept it as the launcher-provided worktree. Record its
   actual root/branch; do not compare its generated path/branch with the named
   fallback or create another worktree merely because they differ.
3. If current context is `main`, dirty, unregistered, or unusable, inspect the
   named worktree. If unusable, read `.agents.local.env`, require
   `AGENTS_WORKTREE_CONTAINER_DIR`, and ask the operator when absent. Create a
   unique worktree/branch there from pushed `origin/main`. Never use `/tmp`,
   `TMPDIR`, or a guessed path; never clean, reset, stash-over, or discard dirty
   state. Report a launcher-supplied dirty or `main` worktree instead of
   creating another.
4. From the selected worktree, record this handoff's repository-relative path.
   Run `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Confirm `git rev-parse HEAD` equals `git rev-parse origin/main`, confirm
   `git merge-base --is-ancestor 00bd0870f151693429264907b756bb289937c6bf HEAD`
   succeeds, and confirm the relative handoff path exists in selected `HEAD`.
   Load it with `git show HEAD:docs/handoffs/20260831-231008-help-first-command-discovery-1093.md`.
   If the absolute dispatch file is readable and differs from that tracked blob,
   stop and report. The committed `HEAD` copy is canonical.
5. Required sibling links are `none`; create none.
6. Read the active milestone, assigned card, `AGENTS.md`, and canonical refs.
7. Run the repo's cheap orientation checks and record what you actually ran.

### While you work

- Execute card `1093` and keep commits aligned with meaningful chunks, not
  arbitrary model turns.
- Use bounded causal and code-level judgment. Remove temporary instrumentation
  before review unless the governing refs require durable observability.
- After each meaningful chunk, report through the active control plane or the
  operator with changed files, validation actually run, remaining work, new
  risks, and blockers.
- Stop if a contract is missing, intent is ambiguous, scope expands,
  authority/access is missing, or validation changes the plan.
- Do not quietly turn an open question into new architecture.

### When the assigned runway is complete

1. Run the required final validation listed in `Current State` and card `1093`.
2. Falsify the diff against all seven review-oracle counterexamples. Enumerate
   universal, exact, and negative claims; map each row to proof; reconcile card,
   roadmap, log, handoff, and front-door state. Return new product thresholds
   or acceptance rules to planning.
3. Mark card `1093`, roadmap `g08.038`, and strict spec `111` complete; archive
   the strict spec; write the required dated evidence log; refresh every active
   next-task pointer to planning for the catalog-pack acquisition prototype.
   Do not open that implementation lane in this PR.
4. Push the selected worker branch.
5. Open a reviewable PR against current pushed `main`. The planning base above
   predates this handoff commit and is intentionally not self-referential.
6. In the PR body, link spec `111`, roadmap `g08.038`, card `1093`, architecture
   `026`, contract `043`, changed surfaces, evidence, validation, and unresolved
   items.
7. Report the exact head, PR URL, evidence, and checks to the operator. Do not
   merge.

### Review and merge path

The orchestrator will review the PR against the canonical refs, diff, and
checks. Current review state: awaiting implementation and PR.

The orchestrator records an evidence-backed verdict in the provider's review
surface. When the orchestrator and worker share a GitHub identity, formal
self-approval is unavailable, so the orchestrator posts the verdict as a PR
comment; that comment is the canonical review record. If changes are requested,
make only those changes on this branch, push again, and report back through the
operator. Blocking findings use `execution-miss`, `oracle-gap`,
`planning-change`, `validation-gap`, or `integration-drift`; a
`planning-change` returns to planning before revision. Requested changes are:
none. The PR should link the card, milestone, spec, changed surfaces, evidence,
validation, and unresolved items. When the exact reviewed head is still
current, required checks pass, the PR is mergeable into the intended base, and
no stricter repository rule or explicit operator pause applies, the
orchestrator merges without another approval prompt.

- **Closeout refs:** card `1093`; evidence log; roadmap `g08.038`; strict spec
  `111`; roadmap, spec, contract, vision, and log front doors; feature-boundary
  open-design note

### Handoff closeout

Before calling the runway complete, leave the card, roadmap, log, and next-task
state honest. If the work is blocked, record the blocker and stop rather than
making the handoff look complete.
