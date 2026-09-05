---
title: External skill task runner worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / Effigy skill-runner orchestrator
created: 2026-08-31
updated: 2026-08-31
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260831-151737-external-skill-task-runner.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, skill-runner]
---

## What This Thread Was Doing

The orchestrator planned an explicit Effigy runner for tasks shipped inside an
installed skill. Today Northstar must point `--repo` at its installed skill and
pass the real consumer through task arguments. The new surface separates the
task-definition source from the consumer runtime target.

This handoff dispatches one complete implementation lane: card `1092`. The
documentation-context lane is paused at card `1089` and resumes only after this
PR closes the skill-runner lane.

## Why It Matters

Agents should be able to run a skill-owned task from a consumer repository
without changing CWD, copying tasks, registering the skill, or teaching the
source/target inversion in every mode file. The boundary must stay explicit:
one operator-selected source supplies code; one independently resolved consumer
owns runtime effects.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `19e74018e22202ac4c7938c9f16657d1fea6496d`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  `19e74018e22202ac4c7938c9f16657d1fea6496d` before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** architecture `025`, contract
  `042`, strict spec `110`, roadmap `g08.037`, ready card `1092`, and planning
  log `2026-08/31-151155-external-skill-task-runner-planning.md`.
- **Worker branch:** `worker/g08-037-skill-runner-1092`.
- **Worker worktree:**
  `/Users/tom/Dev/worktrees/effigy-skill-runner-1092`. This orchestration adapter
  does not supply an isolated launch worktree; the orchestrator creates this
  named worktree from the committed handoff before dispatch.
- **Worktree creation command:**
  `git worktree add -b worker/g08-037-skill-runner-1092 /Users/tom/Dev/worktrees/effigy-skill-runner-1092 origin/main`.
- **Worker worktree policy:** inspect current context first, then reuse the
  named registered worktree. Do not create another worktree.
- **Required sibling worktree links:** none. The Northstar source checkout is a
  read-only optional smoke target, not a catalog member or required link.
- **Active spec lane:**
  `/Users/tom/Dev/projects/effigy/docs/specs/110-external-skill-task-runner-strict-lane.md`
- **Roadmap milestone:**
  `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/037-external-skill-task-runner.md`
- **Ready cards, in order:** only
  `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/batch-cards/1092-add-external-skill-task-runner.md`
- **Allowed runway:** all work in card `1092`: CLI, typed source/target context,
  isolated routing/execution, path semantics, nested task/Rhai preservation,
  JSON/help/docs, tests, Northstar smoke, evidence, and lane closeout.
- **Remaining card budget:** one card. Close it completely, then restore card
  `1089` as ready. Do not implement `1089`.
- **Dispatch topology:** serial lane.
- **Parallel safety check:** skill-runner work touches CLI, context, routing,
  execution, JSON, docs, and front doors. The paused docs-query lane overlaps
  CLI/docs/JSON and remains serial.
- **Canonical refs:** architecture `025`; contracts `011`, `013`, `037`, and
  `042`; strict spec `110`; card `1092`; working rules `001`.
- **Review oracle:** contract `042`, `## Review Oracle`; falsify all six named
  counterexamples before PR creation.
- **Model capability profile:** frontier coding model with high reasoning. This
  lane changes a public CLI and a trust-sensitive source/target path boundary.
- **Tool/runtime restrictions:** use the project-local Effigy skill and
  Northstar Rust everyday-authoring route. Do not edit `.github/workflows/`, run
  release mutations, implement `effigy docs context`, edit the Northstar
  checkout, add automatic skill discovery, or add consumer runtime inheritance.
- **Required validation:** focused changed-crate and runner/CLI-output tests;
  JSON schema/example/selection checks; documentation coverage/generated-help
  checks; read-only Northstar skill smoke when present; `effigy qa`;
  `cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`;
  `git diff --check`.
- **PR base/head:** `main` to `worker/g08-037-skill-runner-1092`.
- **PR URL:** pending.
- **Review state:** awaiting orchestrator review.
- **Merge authorisation:** absent. Do not merge.

## Boundaries

- **In scope:** the complete contract-042 surface and card-1092 closeout.
- **Out of scope:** global skill registry/name resolution, install/update/signing,
  implicit task merging, catalog members, container-bound skill execution,
  consumer runtime inheritance, Northstar repository edits, card `1089`, release
  work, and workflow edits.
- **Outcome shape:** implementation, tests, public/agent docs, changelog,
  evidence log, honest front-door closeout, pushed branch, and reviewable PR.
- Do not invent or widen architecture. Contract `042` settles the public grammar,
  source/target identities, path classes, isolation rules, failures, output,
  compatibility, and review oracle.
- Preserve ordinary task routing and existing `--repo` behavior exactly. Do not
  implement the new surface by reinterpreting `--repo`.
- Reject unsupported source members, consumer/container inheritance, or
  escaping sources before task side effects.
- Keep Northstar proof read-only. Fixture proof is authoritative if
  `/Users/tom/Dev/projects/northstar/skills/northstar/effigy.toml` is absent.
- Follow the repository Rust profile and `PAPERCUTS.md` loop. Record incidental
  solvable friction; do not widen this card to fix it.
- Work only in the named clean worker worktree. Never edit or clean the
  orchestrator's main checkout.
- Do not merge the PR.

## Important Context

- **Planning lineage:** operator-reported Northstar friction was captured and
  promoted through architecture `025`, contract `042`, strict spec `110`,
  roadmap `g08.037`, card `1092`, and the dated planning log.
- **Why this card is ready:** public grammar, v1 isolation, source/target path
  classes, compatibility, failures, validation, stop conditions, and six
  adversarial cases are explicit. Planning `effigy qa:docs` passed.
- **Existing implementation pressure:** command context currently couples
  resolved root, discovered catalogs, task process CWD, path placeholders,
  cache/env paths, and nested dispatch. Start with the typed seam; avoid adding
  scattered command-local exceptions.
- **Northstar source example:**
  `/Users/tom/Dev/projects/northstar/skills/northstar/effigy.toml` declares one
  `northstar` catalog with Rhai-backed setup/check tasks. Current mode docs use
  `effigy --repo <installed-northstar> northstar/<task> <target>`.
- **Product preference:** `effigy skill` is the public domain, backed by a
  generic internal task-source/target split. V1 stays isolated host/Rhai only.
- **Report after:** typed context/source loading is proven; execution/path/nested
  semantics are proven; final JSON/docs/full-QA closeout; or immediately on a
  stop condition.
- **Report to:** the orchestrator through the active collaboration thread.

## Suggested Next Move

Read this handoff top to bottom, run the four-command worktree probe, then move
to `/Users/tom/Dev/worktrees/effigy-skill-runner-1092` and verify the committed
handoff. Load `AGENTS.md`, `PAPERCUTS.md`, card `1092`, architecture `025`,
contract `042`, and strict spec `110` from that worktree. Only then run
`effigy tasks`, `effigy doctor`, and targeted `effigy graph explore` queries.

Start by tracing the smallest typed change that lets preflight carry explicit
task-source identity without changing ordinary requests. Implement in coherent
chunks: context/source loading; execution and path semantics; public rendering,
JSON, docs, and closeout.

## Completion Protocol

### Before you start

1. This handoff's `worker_mode: implementation` and
   `dispatch_authority: orchestrator` metadata activate worker mode. Before
   broad reads, run only `git rev-parse --show-toplevel`,
   `git branch --show-current`, `git status --porcelain`, and
   `git worktree list --porcelain`.
2. The shared orchestration checkout is expected to be `main`; it is not the
   worker target. Verify the named registered worktree
   `/Users/tom/Dev/worktrees/effigy-skill-runner-1092` is clean, on
   `worker/g08-037-skill-runner-1092`, and registered. Use it. Do not create a
   second worktree or mutate the main checkout.
3. From the worker worktree run
   `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Confirm `HEAD == origin/main`, confirm planning base
   `19e74018e22202ac4c7938c9f16657d1fea6496d` is an ancestor of `HEAD`, and
   confirm `docs/handoffs/20260831-151737-external-skill-task-runner.md` exists
   in `HEAD`.
4. Load the canonical blob with
   `git show HEAD:docs/handoffs/20260831-151737-external-skill-task-runner.md`.
   If it differs from the absolute dispatch file, stop.
5. Required sibling worktree links are `none`.
6. Read the active card/spec/roadmap, architecture/contracts, `AGENTS.md`, and
   `PAPERCUTS.md` from the worker worktree.
7. Run `effigy tasks` and `effigy doctor`, then targeted graph queries. Separate
   known warning-only graph/god-file findings from new errors.

### While you work

- Execute only card `1092`. Keep commits aligned with meaningful chunks.
- Use bounded causal and code-level judgment inside contract `042`. Remove
  temporary diagnostics before review.
- Preserve unrelated work. Do not use destructive Git commands.
- Append qualifying incidental execution friction to `PAPERCUTS.md` before
  continuing; do not fix it unless already in scope.
- Report meaningful chunks with changed files, validation actually run,
  remaining acceptance, risks, and blockers.
- Stop if implementation requires implicit discovery, consumer config/runtime
  inheritance, container-bound execution, source members, ordinary routing
  breakage, workflow edits, release work, or a new API decision.

### When card 1092 is complete

1. Falsify all six contract-042 review-oracle cases and run every validation
   named in the card and `Current State`.
2. Write one dated execution log under `docs/logs/archive/2026-08/` with the
   source/target matrix, oracle proofs, no-side-effect failures, JSON proof,
   Northstar smoke or absence, exact test counts, and full-QA results.
3. Mark card `1092`, roadmap `g08.037`, and strict spec `110` complete. Archive
   spec `110` only after acceptance is evidenced. Restore card `1089`, roadmap
   `g08.035`, and strict spec `108` to active/ready state. Update every roadmap,
   spec, contract, log, and vision front door so `1089` is the single next task.
4. Update `CHANGELOG.md` under `[Unreleased]` and all public/agent/generated
   command-reference surfaces required by the card.
5. Push `worker/g08-037-skill-runner-1092` and open a PR against current `main`.
   Link architecture `025`, contract `042`, spec `110`, roadmap `g08.037`, card
   `1092`, the execution log, validation, and residuals.
6. Report the PR URL and evidence to the orchestrator. Do not merge.

### Review and merge path

The orchestrator reviews the PR independently against the canonical refs,
diff, checks, and six oracle cases. If formal self-approval is unavailable, the
orchestrator posts its verdict as a PR comment. Requested changes return to this
same worker. Current requested changes: none. Merge requires explicit operator
authorisation.

- **Closeout refs:** card `1092`; roadmap `g08.037`; strict spec `110` and its
  archive; architecture `025`; contract `042`; dated execution log;
  `CHANGELOG.md`; roadmap/spec/contract/log/vision front doors; PR.

### Handoff closeout

Leave planning state honest. If blocked, record the blocker and stop. Card
`1089` is the next task after this runway; it is not part of this worker PR.
