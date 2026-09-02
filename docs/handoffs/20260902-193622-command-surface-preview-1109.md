---
title: Command-surface preview 1109 worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-02
updated: 2026-09-02
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260902-193622-command-surface-preview-1109.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, cli, migration]
---

## What This Thread Was Doing

The operator and orchestrator audited Effigy's broad command surface, selected
five executable job namespaces, and promoted an additive pre-`v1.0` migration
lane after a 30-repository impact inventory.

This dispatches card `1109` only. No transcript or second prompt is part of the
authority chain.

## Why It Matters

Effigy has useful capabilities but a flat operator surface. The preview makes
the existing job taxonomy executable while keeping one implementation per
command and giving current automation a visible, non-destructive migration
window.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `ae5301f48002f1b8b32d9caa1fe9c43de0d13582`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  the planning base before this handoff was created
- **Planning checkout:** clean before this handoff edit
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight
- **Planning artifacts included at the base:** architecture `026`, contract
  `043`, strict spec `116`, roadmap `g09.001`, card `1109`, and planning log
  `02-192316`
- **Worker branch:** `worker/g09-001-command-surface-preview-1109`
- **Worker worktree:** Paseo-managed worktree created at dispatch; its actual
  clean registered path is authoritative
- **Worktree creation command:** Paseo `branch-off` from `origin/main`; manual
  fallback only under the repository-configured worktree container
- **Worker worktree policy:** follow Completion Protocol; launcher worktree
  first, named/manual fallback only when required
- **Required sibling worktree links:** none
- **Active spec lane:**
  `docs/specs/116-command-surface-compaction-preview-strict-lane.md`
- **Roadmap milestone:**
  `docs/roadmaps/g09/001-command-surface-compaction-preview.md`
- **Ready cards, in order:**
  `docs/roadmaps/g09/batch-cards/1109-add-executable-command-namespaces.md`
- **Allowed runway:** card `1109` only; one implementation PR
- **Remaining card budget:** one card
- **Dispatch topology:** one serial lane; no independent sibling card exists
- **Parallel safety check:** parser, dispatch, warning envelope, help,
  completion, docs, skill parity, and closeout share one command-route authority
- **Surfaces this lane owns:** affected CLI/parser/runner/output tests and code;
  current command/help/completion guides and examples; `.agents/skills/effigy`
  and `skills/effigy`; `CHANGELOG.md`; card `1109`; roadmap `g09.001`; spec
  `116`; one closeout log; current roadmap/spec/log/vision/contract front doors
- **Integration ownership:** worker reconciles the single-lane closeout; the
  orchestrator owns exact-head review, merge, and post-merge currentness audit
- **Merge ordering:** same-repository PRs merge one at a time; the orchestrator
  refreshes this head against current `main` and re-reviews it if another lane
  merges first
- **Canonical refs:**
  `docs/architecture/026-feature-placement-and-command-surface.md`;
  `docs/contracts/001-working-rules.md` and
  `docs/contracts/043-feature-placement-and-surface-migration-contract.md`
- **Review oracle:** card `1109` `## Review Oracle` and spec `116`
  `## Whole-Lane Review Oracle`
- **Model capability profile:** day-to-day non-frontier implementation; select
  the cheapest adequate current profile at dispatch
- **Frontier-worker justification:** none
- **Tool/runtime restrictions:** no `.github/workflows/` edits, release
  mutation, direct-route removal, consumer-repository mutation, S3 work, or
  extension-transport design
- **Required validation:** focused parser/routing/CLI/JSON/help/completion and
  skill-parity tests; `effigy qa`; `cargo fmt --all -- --check`;
  `cargo clippy --all-targets -- -D warnings`; `git diff --check`;
  `effigy doctor --json`
- **PR base/head:** current pushed `main` / worker branch exact head
- **PR URL:** pending
- **Review state:** awaiting implementation and exact-head review
- **Merge path:** orchestrator after accepted review of the current head and
  passing required checks

## Boundaries

- **In scope:** the complete additive grouped-command preview in card `1109`,
  including migration diagnostics, help/completion/current-doc/skill parity,
  evidence, and honest single-lane closeout
- **Out of scope:** direct-route removal; `v1.0` or any release action;
  portfolio consumer edits; workflow edits; S3 or extension transport; unrelated
  feature-boundary work
- **Outcome shape:** one contract-valid additive implementation and reviewable
  PR; diagnosis of existing parser ownership is part of implementation
- Do not invent architecture, change contracts, widen the roadmap, or choose an
  unresolved product/API/persistence/security decision.
- This lane has no sibling worker. Stop if another live change creates shared
  command-route scope rather than resolving it silently.
- Work only in the clean worker worktree selected by Completion Protocol. Never
  edit the planning checkout or an unrelated dirty checkout.
- Do not merge the PR. Merge belongs to the orchestrator after its accepted
  review/check gate.

## Important Context

- **Planning lineage:** Atlas Theme 4 → feature-boundary architecture `026` →
  migration contract `043` → strict spec `116` → `g09.001` → card `1109`
- **Why the runway is ready:** namespace map, daily spine, selector precedence,
  human/JSON warnings, help visibility, completion posture, `v1.0` gate,
  validation, evidence, and adversarial review oracle are settled
- **Decisions and preferences:** `watch` stays direct; grouped routes are
  canonical; legacy direct commands remain executable but leave primary help
  and completion; JSON warnings are optional top-level envelope metadata;
  `admin/<task>` stays a slash selector; grouped commands bypass child-name
  task shadowing intentionally
- **Open tensions:** implementation may expose an unrecorded collision or an
  envelope limitation; either is a stop condition, not worker design authority
- **Report after:** route/parser foundation and adversarial fixtures form one
  coherent passing chunk, then again at PR completion
- **Report to:** the operator through the active Paseo control plane; finish
  notification returns the PR to the orchestrator

## Suggested Next Move

Run the Completion Protocol preflight before broad reads. Then read `AGENTS.md`,
spec `116`, roadmap `g09.001`, card `1109`, architecture `026`, and contracts
`001`/`043`. Implement the smallest shared route representation first and prove
the slash-selector, shadowing, and JSON-warning counterexamples before widening
documentation.

## Completion Protocol

### Before you start

1. This handoff's `worker_mode: implementation` and
   `dispatch_authority: orchestrator` activate worker mode. Before broad reads,
   run `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, its status is empty, and its
   branch is not `main`, accept it as the launcher-provided worktree. Record its
   actual root/branch and do not create another worktree because it differs from
   the planned branch or path.
3. If current context is `main`, dirty, unregistered, or unusable, inspect the
   named worktree. If unusable, read `.agents.local.env`, require
   `AGENTS_WORKTREE_CONTAINER_DIR`, and ask the operator when absent. Create a
   unique worktree/branch there from pushed `origin/main`. Never use `/tmp`,
   `TMPDIR`, or a guessed path; never clean, reset, stash over, or discard dirty
   state. Report a launcher-supplied dirty or `main` worktree instead of creating
   another.
4. From the selected worktree, record this handoff's repository-relative path.
   Run
   `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Confirm `HEAD == origin/main`, confirm
   `ae5301f48002f1b8b32d9caa1fe9c43de0d13582` is an ancestor, and confirm the
   tracked handoff exists in `HEAD`. Compare the tracked blob with the absolute
   dispatch file; stop if they differ.
5. Required sibling links are `none`.
6. Read the active milestone, card, `AGENTS.md`, spec, architecture, and
   contracts named above.
7. Run the repo's cheap orientation checks and record what you actually ran.

### While you work

- Execute card `1109` and keep commits aligned with meaningful chunks.
- Report the route/parser foundation once its adversarial tests pass, then
  finish the remaining discovery, docs, evidence, and closeout batch.
- Stop on missing authority, ambiguity, scope expansion, or validation that
  changes the plan. Do not turn a new choice into architecture.

### When the assigned runway is complete

1. Run all validation named in Current State.
2. Falsify every card/spec oracle row and map it to named proof in the evidence
   log. Reconcile card, roadmap, spec, handoff, and front-door state.
3. Mark card `1109` and `g09.001` complete, archive spec `116`, and leave the
   next task at the future `v1.0` consumer-evidence checkpoint. Do not create or
   ready a removal card.
4. Push the worker branch. If `main` moved, integrate current `main`, revalidate,
   and report the changed head for fresh review.
5. Open one PR against current pushed `main`. Link the spec, milestone, card,
   changed surfaces, evidence, validation, and unresolved `v1.0` gate.
6. Report the PR URL and exact head. Do not merge.

### Review and merge path

The orchestrator reviews the exact PR head against the canonical refs, diff,
checks, and review oracle. Shared GitHub identity means the accepted verdict is
normally a PR comment. Requested changes return to this same worker and use the
classes `execution-miss`, `oracle-gap`, `planning-change`, `validation-gap`, or
`integration-drift`; a planning change returns to the orchestrator first.
Current requested changes: none.

- **Closeout refs:** card `1109`, roadmap `g09.001`, spec `116`, its dated
  evidence log, roadmap/spec/log/vision/contract front doors, `CHANGELOG.md`,
  and managed Effigy skill parity

### Handoff closeout

Leave all closeout refs honest. If blocked, record and report the blocker rather
than making the lane look complete.
