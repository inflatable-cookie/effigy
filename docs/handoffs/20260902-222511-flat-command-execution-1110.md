---
title: Flat command execution 1110 worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-02
updated: 2026-09-02
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260902-222511-flat-command-execution-1110.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, cli]
---

## What This Thread Was Doing

The operator dogfooded card `1109`'s executable command namespaces and found
that the extra execution keywords made the CLI more overbearing, not clearer.
The useful part is the grouped help view. Active spec `117`, roadmap `g09.002`,
and card `1110` now make direct built-in invocation canonical again.

This dispatches card `1110` only. No transcript or second prompt is part of the
authority chain.

## Why It Matters

Effigy should use nesting for real command-owned domains, not turn help
taxonomy into mandatory ceremony. This lane restores the shorter operator
surface without losing structured discovery.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `ab37e4550fe81b2641932993844bfafb4b0078c4`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  the planning base before this handoff was created
- **Planning checkout:** clean before this handoff edit
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight
- **Planning artifacts included at the base:** architecture `026`, contract
  `043`, strict spec `117`, roadmap `g09.002`, card `1110`, and planning log
  `02-222056`
- **Worker branch:** `worker/g09-002-flat-command-execution-1110`
- **Worker worktree:** Paseo-managed worktree created at dispatch; its actual
  clean registered path is authoritative
- **Worktree creation command:** Paseo `branch-off` from `origin/main`; manual
  fallback only under the repository-configured worktree container
- **Worker worktree policy:** follow Completion Protocol; launcher worktree
  first, named/manual fallback only when required
- **Required sibling worktree links:** none
- **Active spec lane:**
  `docs/specs/archive/117-flat-command-execution-strict-lane.md`
- **Roadmap milestone:** `docs/roadmaps/g09/002-flat-command-execution.md`
- **Ready cards, in order:**
  `docs/roadmaps/g09/batch-cards/1110-remove-executable-command-namespaces.md`
- **Allowed runway:** card `1110` only; one implementation PR
- **Remaining card budget:** one card
- **Dispatch topology:** one serial lane; no independent sibling card exists
- **Parallel safety check:** parser, dispatch, warning-envelope, help,
  completion, docs, skill parity, and closeout share one command-route authority
- **Surfaces this lane owns:** affected CLI/parser/runner/output tests and code;
  current help/completion/guides/examples/config; `.agents/skills/effigy` and
  `skills/effigy`; `CHANGELOG.md`; card `1110`; roadmap `g09.002`; spec `117`;
  one evidence log; current roadmap/spec/log/vision/contract front doors
- **Integration ownership:** worker reconciles the single-lane closeout; the
  orchestrator owns exact-head review, merge, and post-merge currentness audit
- **Merge ordering:** same-repository PRs merge one at a time; the orchestrator
  refreshes this head against current `main` and re-reviews it if another lane
  merges first
- **Canonical refs:**
  `docs/architecture/026-feature-placement-and-command-surface.md`;
  `docs/contracts/001-working-rules.md` and
  `docs/contracts/043-feature-placement-and-surface-migration-contract.md`
- **Review oracle:** card `1110` `## Review Oracle` and spec `117`
  `## Whole-Lane Review Oracle`
- **Model capability profile:** day-to-day non-frontier implementation; use an
  economical configured profile whose notes cover bounded implementation
- **Frontier-worker justification:** none
- **Tool/runtime restrictions:** no `.github/workflows/` edits, release
  mutation, consumer-repository mutation, new built-in escape or precedence
  policy, subcommand flattening, S3 work, or extension-transport design
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

- **In scope:** remove the five executable namespace aliases and their
  migration surface; restore direct canonical help, completion, docs,
  generated references, config examples, and managed-skill parity; keep help
  grouping and command-owned subcommands; close card `1110` honestly
- **Out of scope:** renaming help groups; new shadowed-builtin escape;
  selector-precedence changes; flattening genuine subcommands; release or
  workflow mutation; consumer edits; S3 or extension transport
- **Outcome shape:** one complete rollback implementation and reviewable PR;
  diagnosis of preview-owned route/warning surfaces is part of implementation
- Do not invent architecture, widen the roadmap, or choose a new public API.
- This lane has no sibling worker. Stop if another live change creates shared
  command-route scope instead of resolving it silently.
- Work only in the clean worker worktree selected by Completion Protocol.
- Do not merge the PR. Merge belongs to the orchestrator.

## Important Context

- **Planning lineage:** help-first discovery (`1093`) → executable preview
  (`1109`) → operator dogfood rejection → spec `117` / g09.002 / card `1110`
- **Why the runway is ready:** the operator settled execution grammar, retained
  help behavior, alias/warning removal, selector boundary, genuine-subcommand
  boundary, validation, evidence, and adversarial review oracle
- **Decisions and preferences:** flat execution, grouped discovery; direct
  commands are canonical and warning-free; former namespace words return to
  manifest routing; historical preview artifacts stay historically accurate
- **Open tensions:** deferred built-ins can again be shadowed by repository
  tasks. Preserve that existing precedence and report it; do not invent an
  escape in this lane.
- **Report after:** route/warning removal plus adversarial parser fixtures form
  one coherent passing chunk, then again at PR completion
- **Report to:** the operator through the active Paseo control plane; finish
  notification returns the PR to the orchestrator

## Suggested Next Move

Run the Completion Protocol preflight before broad reads. Then read `AGENTS.md`,
spec `117`, roadmap `g09.002`, card `1110`, architecture `026`, and contracts
`001`/`043`. Identify all preview-owned route and warning state, prove selector
restoration first, then update discovery and current guidance as one coherent
surface.

## Completion Protocol

### Before you start

1. This handoff's `worker_mode: implementation` and
   `dispatch_authority: orchestrator` activate worker mode. Before broad reads,
   run `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, its status is empty, and its
   branch is not `main`, accept it as the launcher-provided worktree. Record its
   actual root/branch and do not create another worktree because its generated
   path or branch differs from this handoff.
3. If current context is `main`, dirty, unregistered, or unusable, inspect the
   named worktree. If unusable, read `.agents.local.env`, require
   `AGENTS_WORKTREE_CONTAINER_DIR`, and ask the operator when absent. Create a
   unique worktree/branch there from pushed `origin/main`. Never use `/tmp`,
   `TMPDIR`, or a guessed path; never clean, reset, stash over, or discard dirty
   state. Report a launcher-supplied dirty or `main` worktree instead of
   creating another.
4. From the selected worktree, record this handoff's repository-relative path.
   Run
   `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Confirm `HEAD == origin/main`, confirm
   `ab37e4550fe81b2641932993844bfafb4b0078c4` is an ancestor, and confirm the
   tracked handoff exists in `HEAD`. Compare the tracked blob with the absolute
   dispatch file; stop if they differ.
5. Required sibling links are `none`.
6. Read the active milestone, card, `AGENTS.md`, spec, architecture, and
   contracts named above.
7. Run the repo's cheap orientation checks and record what you actually ran.

### While you work

- Execute card `1110` and keep commits aligned with meaningful chunks.
- Own reproduction, route/warning removal, selector restoration, current
  guidance, validation, evidence, and PR creation as one lane.
- Stop on missing authority, ambiguity, scope expansion, or validation that
  changes the plan. Do not turn the shadowing limitation into a new API.

### When the assigned runway is complete

1. Run every validation named in Current State.
2. Falsify every card/spec oracle row and map it to named proof. Reconcile card,
   roadmap, spec, handoff, and front-door state.
3. Mark card `1110` and `g09.002` complete, archive spec `117`, and leave one
   honest next planning checkpoint. Preserve g09.001, card 1109, archived spec
   116, and their evidence as historical records.
4. Push the worker branch. If `main` moved, integrate current `main`, revalidate,
   and report the changed head for fresh review.
5. Open one PR against current pushed `main`. Link the spec, milestone, card,
   changed surfaces, evidence, validation, and unresolved shadowing limitation.
6. Report the PR URL and exact head. Do not merge.

### Review and merge path

The orchestrator reviews the exact PR head against the canonical refs, diff,
checks, and review oracle. Shared GitHub identity means the accepted verdict is
normally a PR comment. Requested changes return to this same worker and use
`execution-miss`, `oracle-gap`, `planning-change`, `validation-gap`, or
`integration-drift`; a planning change returns to the orchestrator first.
Current requested changes: none.

- **Closeout refs:** card `1110`, roadmap `g09.002`, spec `117`, one dated
  evidence log, roadmap/spec/log/vision/contract front doors, `CHANGELOG.md`,
  current guides/config/generated references, and managed Effigy skill parity

### Handoff closeout

Leave all closeout refs honest. If blocked, record and report the blocker rather
than making the lane look complete.
