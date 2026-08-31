---
title: Bounded documentation context query worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / documentation graph orchestrator
created: 2026-08-31
updated: 2026-08-31
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260831-174019-documentation-context-1089.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, documentation-graph]
---

## What This Thread Was Doing

The documentation-graph lane already shipped its repository-defined profile,
exact Markdown sections, typed facts and relations, and profile-aware freshness
in card `1088`. A separate external-skill task-runner lane interrupted the
sequence and is now merged. This handoff restores the strict lane at its sole
ready task: card `1089`.

Implement the bounded public `effigy docs context` query over the shared graph.
This worker owns the query, CLI, JSON, generated documentation, tests, and
evidence closeout. Northstar profile adoption and proof remain card `1090`.

## Why It Matters

Effigy now stores enough exact documentation structure to retrieve useful
context, but agents still have to assemble it manually. This card turns those
records into a deterministic, budgeted command without adding a second store,
remote inference, or repository-specific runtime logic.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `fd14c5cad5d2ceac02f120e5c912e95a706ac635`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  `fd14c5cad5d2ceac02f120e5c912e95a706ac635` before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Foundation:** card `1088` is complete. Its evidence is
  `docs/logs/2026-08/30-004016-documentation-graph-1088.md`.
- **Worker branch:** intended `worker/g08-035-docs-context-1089`; accept the
  launcher's clean non-`main` branch when one is supplied.
- **Worker worktree:** intended
  `/Users/tom/Dev/worktrees/effigy-docs-context-1089`; accept the launcher's
  clean registered worktree when one is supplied.
- **Worktree creation command:** orchestrator-owned. Do not create a second
  worktree merely because its path differs from the intended name.
- **Required sibling worktree links:** none.
- **Active spec lane:**
  `/Users/tom/Dev/projects/effigy/docs/specs/108-documentation-graph-profiles-strict-lane.md`
- **Roadmap milestone:**
  `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/035-repository-defined-documentation-graph.md`
- **Ready cards, in order:** only
  `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md`
- **Allowed runway:** card `1089` only: bounded query/retrieval, text and JSON
  output, public/help/generated documentation, tests, and closeout evidence.
- **Remaining card budget:** one card. Stop after evidence-backed closeout makes
  `1090` ready; do not implement `1090` in this PR.
- **Dispatch topology:** serial lane. Card `1090` consumes this command and its
  output contract, so it is not parallel work.
- **Canonical refs:** architecture `024`, contract `041`, strict spec `108`,
  roadmap `g08.035`, card `1089`, and working rules contract `001`. Read their
  tracked counterparts from the worker worktree.
- **Model capability profile:** capable coding model with high reasoning; this
  batch introduces a public command and versioned JSON contract.
- **Tool/runtime restrictions:** use the project-local Effigy skill and the
  repository's Northstar Rust everyday-authoring route. Do not edit
  `.github/workflows/`, run release mutations, add package-manager wrappers,
  add remote/model retrieval, or implement Northstar profile adoption.
- **Required validation:** focused codegraph, CLI, built-in help, generated-doc,
  and JSON contract tests named by card `1089`; `cargo fmt --all -- --check`;
  focused `cargo clippy --all-targets -- -D warnings`; `git diff --check`; and
  changed-file affected analysis through `effigy graph`. Use the card's bounded
  validation rather than widening into release work.
- **PR base/head:** `main` to `worker/g08-035-docs-context-1089`, or the actual
  launcher-provided worker branch.
- **PR URL:** pending.
- **Review state:** awaiting independent orchestrator review.
- **Merge path:** do not merge. The orchestrator owns exact-head review, check
  verification, revision routing, and merge after an accepted verdict.

## Boundaries

- **In scope:** `effigy docs context <QUERY>`; positive bounded
  `--max-sections`, `--max-bytes`, and `--max-hops`; baseline and profile-aware
  scope; lexical seed search; bounded typed-relation traversal; relevance-first
  ranking; exact deduplicated section/fact/relation/provenance/match/freshness
  data; deterministic truncation; text and `effigy.docs.context.v1` JSON;
  schema/example/help/generated config/reference/matrix updates; focused tests;
  closeout evidence.
- **Out of scope:** card `1090` implementation, Northstar starter/profile
  content, portfolio status/sync for vendored skills, skill-runner changes,
  embeddings, model-generated summaries, external crawling, daemon/MCP work,
  workflow edits, and release work.
- **Outcome shape:** a reviewable implementation PR with tests and honest
  planning closeout. Do not stop at an exploratory report while the contracted
  implementation remains possible.
- Preserve lazy shared-graph freshness. Do not add a second refresh path or
  query store.
- Retrieval is evidence selection, not synthesis. Return source text and exact
  provenance; do not invent summaries.
- Relevance gates inclusion. Currentness and authority may improve relevant
  ties but must never inject unrelated high-authority material.
- No-match is a successful empty result. An empty query is an error. Defaults
  are 8 sections, 24,000 bytes, and one hop; hard maxima are 32 sections,
  100,000 bytes, and three hops.
- Preserve profile-less baseline behavior and generic runtime vocabulary. No
  Northstar path, status, kind, or relation may enter the generic query logic.
- Keep ordering and budget accounting deterministic. Do not emit misleading
  partial sections when a byte boundary is hit; expose truncation diagnostics.
- Do not invent architecture or change contract `041`. If shared graph records
  cannot satisfy exact provenance or deterministic budgets, pause and report
  the planning gap before weakening the contract.
- Follow the repo's `PAPERCUTS.md` loop. Record incidental solvable friction;
  do not widen this card to fix it.
- Work only in the selected clean worker worktree. Preserve unrelated work and
  do not clean, reset, stash over, or edit another checkout.

## Important Context

- **Planning lineage:** architecture `024` establishes a repository-defined
  semantic layer over the existing graph; contract `041` defines grammar,
  extraction, retrieval, output, and acceptance; strict spec `108` sequences
  cards `1088` through `1090`; roadmap `g08.035` owns the milestone.
- **Why this card is ready:** card `1088` proved exact hierarchical Markdown
  spans, generic profile compilation, typed facts/relations, and profile-aware
  freshness. The shared graph now holds the records required by this query.
- **Likely seams:** use `effigy graph` to find current query/FTS, built-in CLI,
  JSON envelope/schema/example, help snapshot, and generated documentation
  ownership. Do not assume file locations from the predecessor handoff.
- **Review oracle:** the invariant is that lexical or bounded relation relevance
  gates every included result; currentness and authority only break relevant
  ties; every returned item has exact provenance; repeated unchanged queries
  produce the same bounded order and diagnostics.
- **Adversarial cases for implementation and review:**
  1. An unrelated authority-100 section must not outrank or appear beside a
     lexical authority-0 match merely because it is authoritative.
  2. A directly named historical section remains retrievable.
  3. A no-match query succeeds with an empty result and stable metadata.
  4. Section, byte, and hop boundaries truncate deterministically and report
     why without a misleading partial section.
  5. A repository without `[docs_policy.graph]` returns the same public report
     shape from baseline Markdown records.
  6. Repeating an unchanged query returns identical ordering.
- **Public contract:** command form is
  `effigy docs context <QUERY> [--max-sections N] [--max-bytes N] [--max-hops N]`;
  JSON schema identifier is `effigy.docs.context.v1`.
- **Report after:** query/retrieval core is green, then after public output and
  generated-doc surfaces are aligned, or immediately on a stop condition.
- **Report to:** the operator/orchestrator through this worker thread.

## Suggested Next Move

This handoff explicitly activates worker mode. Start by reading it from the top.
Before broad repository reads, run the startup worktree-safety preflight below.
Use the clean launcher-provided non-`main` worktree even if its generated path
or branch differs from the intended names.

After the committed handoff check succeeds, read `AGENTS.md`, `PAPERCUTS.md`,
card `1089`, architecture `024`, contract `041`, strict spec `108`, roadmap
`g08.035`, and the card `1088` evidence log from the selected worktree. Run
`effigy tasks`, `effigy doctor`, and targeted `effigy graph` queries only after
the worktree decision. Implement retrieval and budgets as one coherent chunk,
then align public output, schemas, generated docs, and tests as the second.

At each natural pause, report what changed, validation actually run, remaining
acceptance items, risks, and blockers.

## Completion Protocol

### Before you start

1. Read this handoff path. Its `worker_mode: implementation` and
   `dispatch_authority: orchestrator` metadata activate worker mode. Before any
   broad repo read, run only `git rev-parse --show-toplevel`,
   `git branch --show-current`, `git status --porcelain`, and
   `git worktree list --porcelain`.
2. If the current root is a registered worktree, status is empty, and the branch
   is not `main`, accept it as the launcher-provided worktree. Record the actual
   root and branch. Do not create another worktree for a name mismatch.
3. If the launcher supplied a dirty or `main` worktree, stop and report it. Do
   not silently create a second worktree. If no usable launcher worktree exists,
   inspect the named fallback, then read `.agents.local.env` and require
   `AGENTS_WORKTREE_CONTAINER_DIR`. Ask the operator if the file or key is
   absent. Never use `/tmp`, `TMPDIR`, or a guessed path.
4. From the selected worktree, run
   `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Record this handoff's repository-relative path as
   `docs/handoffs/20260831-174019-documentation-context-1089.md`. Confirm
   `git rev-parse HEAD` equals `git rev-parse origin/main`, confirm
   `git merge-base --is-ancestor fd14c5cad5d2ceac02f120e5c912e95a706ac635 HEAD`,
   and confirm the relative handoff exists in that `HEAD`. Load it with
   `git show HEAD:docs/handoffs/20260831-174019-documentation-context-1089.md`.
   If the absolute dispatch file differs from that tracked blob, stop. The
   committed `HEAD` copy is canonical.
5. Required sibling worktree links are `none`; skip link creation.
6. Read the active milestone, ready card, `AGENTS.md`, `PAPERCUTS.md`, canonical
   architecture/contracts, strict spec, and predecessor evidence from the
   selected worktree.
7. Run `effigy tasks` and `effigy doctor`, then use `effigy graph` for targeted
   code understanding. Record known warnings separately from new errors. The
   planning checkout showed a stale graph-index warning and warning-only
   god-files, with `err:0`.

### While you work

- Execute only card `1089`. Keep commits aligned with retrieval/budgets and
  public output/contracts rather than model turns.
- Use bounded causal and code-level judgment inside the card. Remove temporary
  instrumentation before review unless a governing ref requires it.
- Preserve unrelated work. Do not use destructive Git commands.
- Append qualifying incidental execution friction to `PAPERCUTS.md` before
  continuing, without widening this card to fix it.
- Report after each meaningful chunk with changed files, validation actually
  run, remaining acceptance items, risks, and blockers.
- Stop if a contract is missing, intent is ambiguous, scope expands, retrieval
  needs remote/model inference, a second graph store or refresh path appears
  necessary, generic logic needs Northstar branches, or validation changes the
  plan.

### When card 1089 is complete

1. Run the final validation named in `Current State` and card `1089`.
2. Write one dated execution log under `docs/logs/2026-08/` covering retrieval
   cases, exact ordering and provenance, no-match behavior, direct historical
   retrieval, authority/currentness ties, relation traversal, each budget,
   baseline mode, freshness, and exact test/check results.
3. Mark card `1089` complete. Make card `1090` ready only when every `1089`
   acceptance item is evidenced. Update strict spec `108`, roadmap `g08.035`,
   log/roadmap/spec front doors, and every active `Next Task` pointer so they
   agree. Do not implement `1090`.
4. Append the user-facing command and JSON surface to `CHANGELOG.md` under
   `[Unreleased]`. Keep schema/example/help/generated config/reference/matrix
   surfaces aligned with the shipped contract.
5. Push the worker branch and open a reviewable PR against current `main`. The
   planning base above predates this handoff commit; it is an ancestor check,
   not the PR base SHA.
6. In the PR body, link spec `108`, roadmap `g08.035`, card `1089`, architecture
   `024`, contract `041`, predecessor and new evidence logs, changed surfaces,
   validation, and unresolved items.
7. Report the PR URL, exact head SHA, and evidence to the orchestrator. Do not
   merge.

### Review and merge path

The orchestrator will inspect the exact PR head independently against the
canonical refs, review oracle, diff, and required checks. If the orchestrator
and worker share a GitHub identity, the accepted or changes-requested verdict
will be a PR comment rather than formal self-approval.

If changes are requested, make only those changes on this branch, push, and
report the new exact head. Merge is orchestrator-owned and occurs only after an
accepted exact-head verdict, all required checks pass, the PR is mergeable, and
the operator has not paused the lane.

- **Closeout refs:** card `1089`; roadmap `g08.035`; strict spec `108`; the new
  dated execution log; `docs/logs/README.md`; roadmap/spec front doors;
  `CHANGELOG.md`; public help/generated docs; JSON schema/example/matrix; PR.

### Handoff closeout

Before calling this runway complete, leave card, roadmap, spec, log, and
next-task state honest. If blocked, record the blocker and stop rather than
making the handoff look complete. Card `1090` is the next possible task, not
part of this worker's implementation runway.
