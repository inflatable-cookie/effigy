---
title: Documentation, instruction, and help parity worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / documentation parity orchestrator
created: 2026-08-30
updated: 2026-08-30
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260830-165236-documentation-instruction-help-parity.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, docs, agents, help, scan]
---

## What This Thread Was Doing

The orchestrator turned the operator's whole-project documentation request into
one bounded maintenance lane. The worker will inspect every general Effigy scan,
perform the Northstar AGENTS instruction-surface review, rebuild the public
feature-to-documentation evidence matrix from current code, and repair active
documentation, agent guidance, generated reference output, and shipped CLI help.

This lane temporarily pauses documentation-graph card `1089` because both jobs
can modify help, generated reference, command-matrix, guide, and coverage-test
surfaces. Card `1091` owns the complete audit, repair, recurrence, evidence, and
closeout loop; its successful closeout returns `1089` to ready.

This is the only handoff from the orchestrator to one bounded implementation
thread. The worker does not need a copied transcript or a second prompt.

## Why It Matters

Effigy is useful only when users and agents can discover the behavior that
actually ships. The repository already has a strong August 21 parity baseline,
but subsequent features mean that baseline is no longer sufficient proof. This
run makes active prose, agent instructions, generated reference material, and
the CLI's own help agree with current behavior, while using scan evidence and
stable recurrence checks to prevent quiet drift.

## Current State

Here is the state the worker is inheriting:

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `0902cdf7bdce85e7d783b2fc56a89adbfe4c2e15`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  `0902cdf7bdce85e7d783b2fc56a89adbfe4c2e15` before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** strict spec `109`, roadmap
  `g08.036`, ready card `1091`, planning log
  `2026-08/30-164636-documentation-instruction-help-refresh-planning.md`, and
  the paused/resume state for strict spec `108` and card `1089`.
- **Worker branch:** intended fallback `worker/g08-036-docs-help-parity`; use the
  launcher's clean non-`main` branch when it supplies one.
- **Worker worktree:** launcher-provided first. Manual fallback name:
  `$AGENTS_WORKTREE_CONTAINER_DIR/effigy-docs-help-parity` after the worker
  resolves the required local path contract.
- **Worktree creation command:** launcher-owned by default. Manual fallback only
  after the handoff preflight and a valid `AGENTS_WORKTREE_CONTAINER_DIR`:
  `git worktree add -b worker/g08-036-docs-help-parity "$AGENTS_WORKTREE_CONTAINER_DIR/effigy-docs-help-parity" origin/main`.
- **Worker worktree policy:** use a clean, dedicated, non-`main` registered
  worktree supplied by the launcher even when its generated path or branch
  differs from the fallback above. Record the actual values and do not create a
  second worktree because of a name mismatch. If the current context is
  unusable, inspect the named fallback; only then read `.agents.local.env`,
  require `AGENTS_WORKTREE_CONTAINER_DIR`, and ask the operator when it is
  absent. Never guess `/tmp`, `TMPDIR`, or a repository-adjacent location.
- **Required sibling worktree links:** none.
- **Active spec lane:**
  `/Users/tom/Dev/projects/effigy/docs/specs/109-documentation-instruction-and-help-parity-refresh.md`
- **Roadmap milestone:**
  `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/036-documentation-instruction-and-help-parity-refresh.md`
- **Ready cards, in order:** only
  `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/batch-cards/1091-audit-and-refresh-documentation-instructions-and-help.md`
- **Allowed runway:** card `1091` only: scan and AGENTS review, current public
  feature inventory, active documentation/help/generated-reference repairs,
  proportional recurrence checks, full validation, evidence, and closeout.
- **Remaining card budget:** one card. Stop after `1091` is complete and card
  `1089` has honestly returned to ready. Do not implement `1089` in this PR.
- **Dispatch topology:** one serial worker lane.
- **Parallel safety check:** this lane shares built-in help, generated reference,
  guide, command-matrix, and coverage-test surfaces with card `1089`; keep it
  serial and leave the documentation-graph lane paused until closeout.
- **Canonical refs:**
  `/Users/tom/Dev/projects/effigy/docs/contracts/001-working-rules.md`;
  `/Users/tom/Dev/projects/effigy/docs/guides/035-guide-ownership-and-update-triggers.md`;
  `/Users/tom/Dev/projects/effigy/docs/guides/037-documentation-contribution-playbook.md`;
  `/Users/tom/Dev/projects/effigy/docs/roadmaps/g08/034-documentation-coverage-parity.md`;
  `/Users/tom/Dev/projects/effigy/docs/specs/archive/107-documentation-coverage-parity.md`;
  `/Users/tom/Dev/projects/effigy/docs/logs/archive/2026-08/21-230738-documentation-coverage-parity-closeout.md`.
  Read their tracked counterparts from the selected worker worktree.
- **Model capability profile:** capable coding model with high reasoning; the
  run is broad in evidence but bounded to public documentation and discovery.
- **Tool/runtime restrictions:** use the project-local Effigy skill. After the
  worktree preflight, use the installed `northstar-agents-review` skill as the
  AGENTS audit procedure; this handoff already supplies worker authority and the
  operator explicitly authorizes bounded repairs to root `AGENTS.md` and
  `CLAUDE.md`. Do not start a nested worker or orchestrator. Do not edit
  `.github/workflows/`, run release mutations, alter dependencies, add
  package-manager wrappers, rewrite historical evidence, or change production
  behavior merely to simplify documentation.
- **Required validation:** focused help/parser/render and generated-config tests;
  `effigy test --test documentation_coverage_tests`; `effigy qa:docs`;
  `effigy docs check workflow-paths`; `effigy qa:docs:agent-defaults`; all
  general scans plus final changed-file `validation-gaps`; `effigy qa`;
  `cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`;
  `git diff --check`.
- **PR base/head:** current pushed `main` to the launcher-selected worker branch,
  or fallback `worker/g08-036-docs-help-parity`.
- **PR URL:** pending.
- **Review state:** awaiting orchestrator review after the operator relays the
  PR URL.
- **Merge authorisation:** absent. Do not merge.

## Boundaries

Keep this run inside card `1091`:

- **In scope:** all general `effigy scan` results and their dispositions; the
  root `AGENTS.md` and exact `CLAUDE.md` bridge; current public command, flag,
  selector, JSON, manifest/config, environment, diagnostics, and agent-workflow
  behavior families; `README.md`; active docs front doors and guides; live
  contracts that advertise public behavior; command reference and
  troubleshooting; both Effigy skill trees; general and scoped CLI help;
  generated config/reference output; documentation/help renderers and tests;
  coverage guards; `[Unreleased]` changelog entry; evidence and planning
  closeout.
- **Out of scope:** production behavior changes, new public APIs or contracts,
  code-quality refactors whose only motivation is a code-only scan finding,
  workflow/release/dependency changes, historical logs and archived planning
  rewrites, vendored material, generated build output, broad style churn, and
  card `1089` implementation.
- **Outcome shape:** an evidence-backed audit and the smallest complete set of
  in-scope repairs, deterministic guards, closeout log, pushed branch, and
  reviewable PR. Do not stop at a findings report while verified repairs remain
  inside the card.
- “In-app help” means Effigy's shipped general and scoped CLI help plus generated
  config/reference output. “All documentation” means active current surfaces;
  history is evidence, not a rewrite target.
- Rebuild the feature matrix from live source. The g08.034 matrix is a useful
  checklist and comparison point, not current proof.
- Scan findings are not an instruction to refactor unrelated Rust. Fix findings
  inside documentation, instruction, help, renderer, or validation ownership;
  record code-only findings as accepted or deferred with an exact owner/reason.
- A coverage claim needs an explicit behavior-family row with implementation
  owner, required active surfaces, rendered help/reference proof, finding, and
  disposition. Keyword search alone is not evidence.
- Prefer concise routing to one authoritative deep guide over repeating the same
  prose everywhere. Add tests for stable relationships, not arbitrary wording.
- Preserve unrelated work and follow the repository's `PAPERCUTS.md` loop.
- Do not invent architecture, widen the roadmap, choose unresolved product
  behavior, or turn an unclear behavior into documentation by guesswork.
- Work only in the selected clean worker worktree. Never edit, clean, reset, or
  stash over the orchestrator's planning checkout or another dirty checkout.
- Do not merge the PR. Merge remains a separate operator-authorised action.

## Important Context

- **Planning lineage:** g08.034 established a whole-public-surface evidence
  matrix and proportional guards. Subsequent changes, including dependency
  linking, papercuts, unified test authority, and the documentation-profile
  foundation, require a fresh current-main audit. Strict spec `109`, roadmap
  `g08.036`, and card `1091` define the new maintenance boundary.
- **Why this card is ready:** the operator selected the outcome; active versus
  historical scope, scan dispositions, AGENTS repair authority, feature
  inventory sources, help/reference boundaries, acceptance, validation, and
  stop conditions are explicit. Planning-base `effigy qa:docs` and
  `git diff --check` passed.
- **Decisions and preferences:** fix every verified in-scope gap; make feature
  coverage useful to users and models; include the CLI's own help, not only
  Markdown; retain active docs as explanation rather than a second runtime
  registry; leave card `1089` untouched until this lane closes.
- **Known planning baseline:** before the lane was created, `effigy doctor`
  reported `err:0`, a stale graph warning, and five warning-level god-file
  findings. `effigy --json scan god-files` reported those same five warnings
  with no high or critical findings. Treat this as orientation, not a substitute
  for the worker's full scan run.
- **AGENTS review boundary:** review the target repository only. Measure and
  inspect root `AGENTS.md`, verify the exact `CLAUDE.md` bridge, test commands
  and links proportionally, distinguish always-loaded rules from linked detail,
  and record why each finding was repaired or retained. The operator authorized
  bounded repairs to these files; no Northstar source-repo edits are in scope.
- **Open tensions:** prose sufficiency remains partly judgment-based; avoid
  overstating mechanical completeness. Generated help/reference output may
  reveal behavior drift; if the truth requires a production change rather than
  a documentation repair, stop and return that defect to planning.
- **Report after:** baseline scan plus AGENTS review; then the current feature
  matrix and first coherent repair batch; then recurrence/final validation and
  PR creation, or immediately on any stop condition.
- **Report to:** the operator, who will relay progress and the PR URL to the
  orchestrator.

## Suggested Next Move

This handoff explicitly activates worker mode. Read it from the top, then run
only the worktree-safety preflight below before broad repository reads. Use the
launcher-provided clean non-`main` worktree even if its generated path or branch
differs from the fallback names. Do not create another worktree for that reason.

After the committed handoff check succeeds, read `AGENTS.md`, `PAPERCUTS.md`,
strict spec `109`, roadmap `g08.036`, card `1091`, guides `035` and `037`, and
the g08.034 evidence from the selected worktree. Then run `effigy tasks`,
`effigy doctor`, the full scan baseline, and the Northstar AGENTS review. Use
`effigy graph` for ownership/behavior discovery and exact source/rendered output
for final claims.

At each natural pause, tell the operator what changed, what validation actually
ran, which feature families remain, scan/AGENTS findings and dispositions, and
whether anything needs a planning decision.

## Completion Protocol

### Before you start

1. Read this handoff path. Its `worker_mode: implementation` and
   `dispatch_authority: orchestrator` metadata activate worker mode. Before any
   broad repo read, run only:
   `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, status is empty, and the branch
   is not `main`, accept it as the launcher-provided worktree. Record the actual
   root/branch and do not compare them with the fallback names above.
3. If the launcher supplied a dirty or `main` worktree, stop and report it; do
   not silently create a second worktree. If no usable launcher worktree exists
   outside that case, inspect the named fallback, then read `.agents.local.env`
   and require `AGENTS_WORKTREE_CONTAINER_DIR`. Ask the operator if the file or
   key is absent. Create a unique worktree/branch under that container from
   `origin/main`. Never use `/tmp`, `TMPDIR`, or a guessed path, and never clean,
   reset, stash over, or discard the original checkout.
4. From the selected worktree, run `git fetch origin`. Record this handoff's
   repository-relative path as
   `docs/handoffs/20260830-165236-documentation-instruction-help-parity.md`.
   Confirm `git rev-parse HEAD` equals `git rev-parse origin/main`, confirm
   `git merge-base --is-ancestor 0902cdf7bdce85e7d783b2fc56a89adbfe4c2e15 HEAD`,
   and confirm the relative handoff exists in that `HEAD`. Load it with
   `git show HEAD:docs/handoffs/20260830-165236-documentation-instruction-help-parity.md`.
   If the absolute dispatch file differs from that tracked blob, stop. The
   committed `HEAD` copy is canonical.
5. Required sibling worktree links are `none`; skip link creation.
6. Read the active milestone, card `1091`, strict spec `109`, `AGENTS.md`,
   `PAPERCUTS.md`, and canonical refs from the selected worktree.
7. Run `effigy tasks` and `effigy doctor`, then the full general scan family.
   Record planning-baseline warnings separately from new errors. Invoke the
   installed `northstar-agents-review` skill for the target-repo instruction
   audit only after this worker preflight; do not start a nested lane.

### While you work

- Execute only card `1091`. Keep commits aligned with meaningful chunks:
  evidence inventory, documentation/help repairs, and guards/closeout rather
  than model turns.
- Run and record all general scans. Use `validation-gaps` against the final
  changed-file set. Do not claim code-only warnings were repaired when this lane
  only classified them.
- Build the behavior-family matrix from implementation owners first. Compare
  current behavior with active docs, both skills, AGENTS/CLAUDE, built-in help,
  and generated reference output. Inspect rendered help/config, not only source
  strings.
- Perform the Northstar AGENTS review read-only first, then make only bounded
  repairs supported by findings. Keep always-loaded instructions concise and
  move detail behind canonical links when that improves execution without
  weakening a real boundary.
- Fix each verified in-scope gap. Add or extend deterministic guards where a
  stable relationship can drift mechanically. Do not create a parallel command
  or feature registry merely to test prose coverage.
- Preserve unrelated work. Do not use destructive Git commands.
- Append qualifying incidental execution friction to `PAPERCUTS.md` before
  continuing, without widening this card to fix it.
- Report after each meaningful chunk with changed files, validation actually
  run, remaining feature families, scan/AGENTS findings, risks, and blockers.
- Stop if authority is missing, behavior is ambiguous, scope expands into
  production semantics, a new public decision is needed, a workflow/release or
  dependency mutation appears necessary, historical evidence would need
  rewriting, or validation changes the plan.

### When card 1091 is complete

1. Run the final validation named in `Current State` and card `1091`.
2. Write one dated execution log under `docs/logs/archive/2026-08/` containing the full
   feature matrix, general-scan before/after counts and dispositions,
   changed-file validation-gap analysis, Northstar AGENTS review metrics and
   findings, rendered help/config proof, changed surfaces, guard decisions,
   exact validation, blocked/residual items, and closeout transition.
3. Mark card `1091` and roadmap `g08.036` complete. Archive strict spec `109`
   only after every acceptance item is evidenced. Return strict spec `108` and
   roadmap `g08.035` to active, make card `1089` ready, and update contract,
   roadmap, spec, and log front doors so one `Next Task` points to `1089`.
   Do not implement `1089`.
4. Update `CHANGELOG.md` under `[Unreleased]` for user-facing documentation,
   help, generated-reference, or agent-discovery changes.
5. Push the selected worker branch and open a reviewable PR against the current
   pushed `main` tip. The planning base above predates this handoff commit; it
   is an ancestor check, not the PR base SHA.
6. In the PR body, link spec `109`, roadmap `g08.036`, card `1091`, the planning
   and execution logs, g08.034 prior evidence, changed surfaces, scan/AGENTS
   evidence, validation, and unresolved items.
7. Report the PR URL and evidence to the operator. Do not merge.

### Review and merge path

The orchestrator will review the PR independently against the canonical refs,
diff, rendered help/reference output, scan and AGENTS evidence, and hosted
checks. Current review state: awaiting review.

If the orchestrator and worker share a GitHub identity, the orchestrator will
post the verdict as a PR comment instead of formal self-approval. If changes are
requested, make only those changes on this branch, push, and report through the
operator. Requested changes are currently: none. The operator must explicitly
authorise any merge.

- **Closeout refs:** card `1091`; roadmap `g08.036`; strict spec `109`; the new
  dated execution log; `docs/logs/README.md`; roadmap/spec/contract front doors;
  resumed strict spec `108`, roadmap `g08.035`, and ready card `1089`;
  `CHANGELOG.md`; the PR.

### Handoff closeout

Before calling this runway complete, leave card, roadmap, spec, log, and
next-task state honest. If blocked, record the blocker and stop rather than
making the handoff look complete. Card `1089` is the next possible task, not
part of this worker's implementation runway.
