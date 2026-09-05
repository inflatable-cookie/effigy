---
title: Documentation coverage parity worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: documentation implementation worker
created: 2026-08-21
updated: 2026-08-21
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260821-225240-documentation-coverage-worker.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr]
---

## What This Thread Was Doing

The operator asked whether Effigy's recent managed-runtime and workspace-user
changes were well documented, then widened the request: scan the whole current
repository for the same class of documentation coverage drift and fix every
in-scope gap.

The orchestrator promoted that intent into one strict serial lane. This is the
handoff from planning to the implementation worker. You do not need the source
conversation; the spec, roadmap, cards, and this file carry the authority.

## Why It Matters

Correct behavior is not enough when an operator or agent cannot discover it
from the surface they naturally use. Effigy currently has several public
documentation layers—skills, CLI help, generated config docs, front doors,
reference guides, deep guides, and troubleshooting—and feature-local updates
can leave those layers uneven. This lane makes that drift visible, repairs it,
and adds proportional recurrence protection.

## Current State

Here is the state the worker is inheriting:

- **Repository:** `inflatable-cookie/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `0e7ca695dc76d0853339d3184060fda4578a1192`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  `0e7ca695dc76d0853339d3184060fda4578a1192` before this handoff commit.
- **Planning checkout:** clean; `.agents.local.env` is a local excluded file
  explicitly authorised by the operator.
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** strict spec `107`, roadmap
  `g08.034`, cards `1086` and `1087`, and the planning log.
- **Worker branch:** `worker/g08-034-docs-coverage`
- **Worker worktree:** `/Users/tom/Dev/worktrees/effigy-g08-034-docs-coverage`
- **Worktree creation command:** `git worktree add -b worker/g08-034-docs-coverage /Users/tom/Dev/worktrees/effigy-g08-034-docs-coverage origin/main`
- **Worker worktree policy:** first use the clean, dedicated, non-`main`
  registered worktree supplied by the launcher, even if its generated path or
  branch differs from these handoff placeholders. Record the actual path/branch
  and never create a second worktree for that reason. If the current context is
  unusable, use the named worktree when it matches; only then read
  `.agents.local.env`, require `AGENTS_WORKTREE_CONTAINER_DIR`, and create a
  unique manual worktree/branch under that container from `origin/main`. Ask the
  operator first if the file or key is absent; never use `/tmp`, `TMPDIR`, or a
  guessed path.
- **Active spec lane:** `/Users/tom/Dev/projects/effigy/docs/specs/107-documentation-coverage-parity.md`
- **Roadmap milestone:** `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/034-documentation-coverage-parity.md`
- **Ready cards, in order:** `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/batch-cards/1086-audit-and-align-documentation-coverage.md`, then `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/batch-cards/1087-guard-and-close-documentation-coverage.md`
- **Allowed runway:** inventory current public behavior across the whole repo;
  repair verified gaps in active user, agent, built-in, and generated docs;
  add proportional recurrence checks; validate and close the lane.
- **Remaining card budget:** two ordered cards in one worker PR.
- **Dispatch topology:** serial single-worker lane.
- **Parallel safety check:** the cards share documentation, test, changelog,
  evidence, and planning front-door files; parallel writers would conflict.
- **Canonical refs:** `docs/architecture/000-overview.md`,
  `docs/architecture/010-package-map.md`;
  `docs/contracts/001-working-rules.md`, active behavior contracts selected by
  each audited surface, `docs/guides/035-guide-ownership-and-update-triggers.md`,
  and `docs/guides/037-documentation-contribution-playbook.md`.
- **Model capability profile:** implementation-capable agent with strong
  repository navigation, Rust, documentation, and regression-test judgment.
- **Tool/runtime restrictions:** follow `AGENTS.md`; use Effigy routing; do not
  modify `.github/workflows/`, release state, or production behavior.
- **Required validation:** focused help/config/skill/docs-policy tests;
  `effigy qa:docs`; `effigy docs check workflow-paths`;
  `effigy qa:docs:agent-defaults`; `effigy qa`;
  `cargo fmt --all -- --check`;
  `cargo clippy --all-targets -- -D warnings`; `git diff --check`.
- **PR base/head:** `main` <- `worker/g08-034-docs-coverage`
- **PR URL:** pending worker completion.
- **Review state:** awaiting orchestrator review.
- **Merge authorisation:** not granted; do not merge.

## Boundaries

Please keep this run inside the named runway:

- **In scope:** all current public Effigy behavior families and their active
  user, agent, built-in, generated, reference, and troubleshooting docs; docs
  rendering/check infrastructure and focused tests needed to prevent drift.
- **Out of scope:** production behavior, new public APIs or contracts, release
  and dependency mutation, `.github/workflows/`, archives and historical logs,
  and unrelated prose cleanup.
- Do not invent architecture, change contracts, widen the roadmap, or choose an
  unresolved product/API/persistence/security decision.
- This handoff represents one worker lane. Do not edit another lane's assigned
  scope; if shared mutable scope or a hidden dependency appears, stop and report
  it through the operator.
- Work only in the selected clean worker worktree: prefer the current
  launcher-provided worktree and record its actual path/branch; otherwise use
  `/Users/tom/Dev/worktrees/effigy-g08-034-docs-coverage` /
  `worker/g08-034-docs-coverage`, or the recorded local-path fallback created by
  the startup preflight. Never edit the orchestrator's planning checkout or an
  unrelated dirty checkout.
- Do not merge the PR. Merge remains a separate operator-authorised action.

## Important Context

- **Planning lineage:** operator intent follows the Acowtancy g04.022
  boot-proof ledger and two post-merge addenda. Current Effigy behavior already
  contains headless managed mode, the four upstream fixes, workspace ownership
  diagnosis, and non-console exec. Strict spec `107` turns the documentation
  question into a whole-repository parity sweep inside active g08.
- **Why these cards are ready:** current `main` is clean and pushed; the audit
  sources, target docs surfaces, non-goals, acceptance, validation, and
  closeout requirements are explicit.
- **Decisions and preferences:** classify by current public behavior family,
  not keyword count. Prefer routing over prose duplication. Treat historical
  material as evidence. Keep `.agents/skills/effigy/SKILL.md` authoritative in
  this repo while preserving parity with the distributed
  `skills/effigy/SKILL.md` source.
- **Open tensions:** prose coverage cannot be proved fully by automation. Add
  deterministic guards only where the relationship is stable; do not create a
  second command/config registry or brittle full-paragraph snapshots.
- **Seed findings to verify, not assume:** the deep guides already appeared to
  cover most managed-runtime behavior, while the skill and parts of built-in
  discovery appeared thinner around headless companions, ownership diagnosis,
  and selector affordances. Recheck against the handoff tip and let the full
  inventory determine the final scope.
- **Report after:** card `1086` inventory and gap-repair batch is coherent and
  focused checks pass, then again after final validation and PR creation.
- **Report to:** the operator, who will relay progress to the orchestrator.

## Suggested Next Move

This handoff explicitly activates worker mode. Start by reading it from the top.
Before broad repository reads, run the quick startup worktree-safety preflight
in `## Completion Protocol`. The named worktree should already exist and be a
clean registered checkout on `worker/g08-034-docs-coverage`; use it if so and do
not create another.

Then read `AGENTS.md`, strict spec `107`, roadmap `g08.034`, both cards, the
docs contribution/ownership guides, and the current package map. Use
`effigy graph` to locate behavior and documentation owners, exact source reads
to substantiate the matrix, and `effigy tasks` for selector discovery. Build
the matrix before deciding that a surface is covered or missing.

Take card `1086` as the first coherent chunk. When it is stable, report changed
files, checks actually run, remaining gaps, and any issue that needs an
orchestrator decision. Continue into card `1087` only when the inventory is
honest and no stop condition has fired.

## Completion Protocol

### Before you start

1. Read this handoff path. Its `worker_mode: implementation` and
   `dispatch_authority: orchestrator` metadata activate worker mode. Then run
   one quick read-only safety probe before broad repository reads:
   `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, its status is empty, and its
   branch is not `main`, accept it as the launcher-provided worktree. Record its
   actual root/branch and do not compare them with the named path/branch or
   create another worktree merely because they differ.
3. Only if the current context is `main`, dirty, unregistered, or otherwise
   unusable should you inspect the named worktree. If that also cannot be used,
   read `.agents.local.env` and require `AGENTS_WORKTREE_CONTAINER_DIR`; if it
   is absent, ask the operator before creating the file or worktree. Then create
   a unique worktree and branch under that container from pushed `origin/main`,
   record the actual path and branch, and run all subsequent commands there.
   Never use `/tmp`, `TMPDIR`, or a guessed path; never clean, reset, stash-over,
   or discard the original checkout's dirty state. If the launcher supplied a
   dirty or `main` worktree, stop and report it instead of silently creating a
   second worktree.
4. From the selected worktree, confirm `git rev-parse HEAD` equals
   `git rev-parse origin/main`, confirm
   `git merge-base --is-ancestor 0e7ca695dc76d0853339d3184060fda4578a1192 HEAD`
   succeeds, and confirm this handoff file exists in the selected `HEAD`.
5. Read the active milestone, assigned cards, `AGENTS.md`, and canonical refs.
6. Run `effigy tasks`, `effigy doctor`, and the code-understanding queries
   needed to orient the audit. Record what you actually ran.

### While you work

- Execute cards `1086` and `1087` in order and keep commits aligned with
  meaningful chunks, not arbitrary model turns.
- After each meaningful chunk, report through the operator with changed files,
  validation actually run, remaining cards, new risks, and blockers.
- Stop and say so if a contract is missing, intent is ambiguous, scope expands,
  authority/access is missing, or validation changes the plan.
- Do not quietly turn an open question into a new architecture.

### When the assigned runway is complete

1. Run the required final validation: focused help/config/skill/docs-policy
   tests; `effigy qa:docs`; `effigy docs check workflow-paths`;
   `effigy qa:docs:agent-defaults`; `effigy qa`;
   `cargo fmt --all -- --check`;
   `cargo clippy --all-targets -- -D warnings`; `git diff --check`.
2. Update the card/log evidence required by the runway, including the actual
   worktree and branch if a temporary fallback was used.
3. Push the selected worker branch.
4. Open a reviewable PR against the current pushed `main` tip. The handoff's
   planning base is the planning commit before this handoff commit, not a
   self-referential hash for the commit containing this file.
5. In the PR body, link the spec, milestone, cards, changed surfaces, evidence
   matrix, validation, and unresolved items.
6. Report the PR URL and evidence to the operator. Do not merge.

### Review and merge path

The orchestrator will review the PR against the canonical refs, diff, and
checks. Current review state: awaiting implementation and orchestrator review.

The orchestrator and worker share a GitHub identity, so formal self-approval may
be unavailable. The orchestrator will post the evidence-backed verdict as a PR
comment when needed. If changes are requested, make only those changes on this
branch, push again, and report back. Requested changes are: none at dispatch.
The operator must explicitly authorise any merge.

- **Closeout refs:** cards `1086` and `1087`, roadmap `g08.034`, strict spec
  `107` and its archive destination, `docs/logs/archive/2026-08/`,
  `docs/logs/README.md`, `docs/roadmaps/README.md`,
  `docs/roadmaps/generation-index.md`, `docs/roadmaps/g08/README.md`, and
  `docs/specs/README.md`.

### Handoff closeout

Before calling the runway complete, leave the cards, roadmap, log, spec archive,
front doors, and next-task state honest. If work is blocked, record the blocker
and stop rather than making the handoff look more complete than it is.
