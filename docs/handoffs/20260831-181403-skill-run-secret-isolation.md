---
title: Skill-run consumer secret isolation worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / Northstar orchestrator
created: 2026-08-31
updated: 2026-08-31
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260831-181403-skill-run-secret-isolation.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, skill-run, secrets]
---

## What This Thread Was Doing

The Paseo settings rollout exercised the new `effigy skill run` surface from
Contact Patch and Acowtancy worktrees. Both consumer manifests declare required
Rhai-targeted vault secrets. A Northstar lifecycle task that does not read
secrets failed before its script ran because stdin was non-interactive.

This lane owns the smallest Effigy repair needed to make external skill tasks
honor their documented secret-isolation boundary.

## Why It Matters

Northstar's portable Paseo lifecycle is bundled once as a Rhai skill task.
Consumer projects with vault declarations must be able to run that unrelated
task non-interactively. Requiring a product vault passphrase during worktree
setup makes the new skill surface unusable for those projects.

## Current State

- **Repository:** `effigy`
- **Planning branch / base:** `main` at
  `827485808db8974f45a1e1136009d921204dd5e2`
- **Pushed main verification:** base matched `origin/main` before this handoff
- **Worker branch:** `worker/skill-run-secret-isolation`
- **Worker worktree:** Paseo-managed; record the generated path
- **Required sibling worktree links:** none
- **Allowed runway:** reproduce, repair the external-skill execution boundary,
  add focused regression tests, validate, push, and open one PR
- **Canonical refs:** `AGENTS.md`; `src/runner/skill_command.rs`;
  `crates/effigy-rhai/src/rhai_secrets.rs`; the skill command help and tests
- **Review state:** awaiting worker PR
- **Merge path:** orchestrator after accepted review of the exact current head

## Boundaries

- In scope: external `effigy skill run` execution and focused regression
  coverage for consumer secret isolation.
- Out of scope: changing ordinary consumer task secret semantics, vault formats,
  secret values, release machinery, Paseo config, or broad executor redesign.
- Preserve the documented rule that v1 external skill tasks do not inherit
  consumer secrets and source tasks requesting manifest-backed secrets are
  rejected.
- Preserve the consumer repository as runtime-effect/cwd authority.
- Never expose, print, unlock, or mutate a real vault. Use fixtures.
- Do not merge the PR.

## Important Context

Observed command:

`effigy skill run --path "$HOME/.agents/skills/northstar" paseo:worktree -- prepare ../underlay ../poodle`

Observed failure before the Northstar script ran:

`Rhai secrets require an unlocked vault passphrase and secret input requires an interactive TTY`

`run_skill_task` installs the isolated task source into the consumer runtime
context. Rhai secret-store resolution currently reads the consumer manifest and
eagerly unlocks required Rhai secrets even though the isolated source task is
forbidden from requesting consumer secrets.

Review oracle:

- A consumer fixture with required Rhai secrets and a locked/missing-passphrase
  vault can run an external Rhai skill task that never calls `secrets::*`.
- The task runs from the consumer root and retains allowed runtime effects.
- An external source task that declares secret inheritance remains rejected.
- Ordinary consumer Rhai tasks retain their existing required-secret behavior.
- The fix does not create a bypass that lets an external skill call consumer
  `secrets::*`.

## Suggested Next Move

Run the worker preflight, reproduce with a fixture, then trace the task-source
context into Rhai secret-store construction. Choose the narrowest explicit
boundary; do not weaken the global vault contract.

## Completion Protocol

### Before work

Accept only a clean registered non-`main` launcher worktree. Fetch origin,
confirm `HEAD == origin/main`, confirm the planning base is an ancestor, and
load this tracked handoff from `HEAD`. Read `AGENTS.md`, `effigy tasks`,
and the named implementation surfaces before editing.

### Implementation and proof

Add a regression that fails for the observed non-interactive external-skill
case. Implement the smallest fix that makes the review oracle true. Run focused
tests, `git diff --check`, and the repository's relevant QA selectors. Try to
falsify the boundary with an external script attempting secret access.

### PR and review

Commit and push `worker/skill-run-secret-isolation`. Open a PR to `main`
with the reproduction, causal explanation, exact test evidence, and any
remaining risk. Report the PR URL and exact head SHA. Do not merge.

If changes are requested, stay on the same branch. The orchestrator will post
the review and send a Paseo follow-up to wake this originating worker.

