---
title: retire roadmap backlog
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: northstar-queue
created: 2026-09-09
updated: 2026-09-09
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260909-152106-retire-roadmap-backlog.md
base_required: pushed-main
queue_dispatch: northstar-queue
queue_approval: "The operator supplied the one-time roadmap-backlog retirement prompt on 2026-09-09 and previously directed this Chatterbox to use Northstar Queue for dispatch."
queue:
  capability: general
  skipPRReview: false
tags: [coordination, handoff, worker, pr, docs, cleanup, backlog, triage]
---

## What This Thread Was Doing

Chatterbox applied the installed Northstar cleanup mode to the operator's
one-time roadmap-backlog retirement prompt. It verified a clean synchronized
integration checkout, inspected queue and Paseo state, inventoried every
backlog item and inbound live reference, and froze the dispositions below.

Repository state at classification:

- repository: `/Users/tom/Dev/projects/effigy`
- integration branch: `main`
- exact integration head and `origin/main`: `339726f0bf9caf7fe68da97c7a1798037109266e`
- worktree: clean
- lifecycle state: paused
- active generation: none
- approved frontier: none
- active strict lane or task: none
- installed Northstar source: `/Users/tom/.agents/skills/northstar`
- installed Northstar source commit: `b8ce1b0a2b41ab85caff29d7aa1362a1e4d59e1f`

The only Effigy queue record was already done. No unfinished worker, reviewer,
PR, handoff, or workspace owned the mutation surfaces. The retained prunable
worktree `/Users/tom/Dev/worktrees/effigy-skill-runner-1092` is pre-existing
and must not be modified.

## Why It Matters

Effigy still teaches two intake layers: executable roadmap tasks and a roadmap
backlog. Current Northstar doctrine uses one clean boundary. Roadmaps hold only
promoted executable tasks; `docs/triage/` temporarily holds unresolved or
deferred candidates and never grants execution authority. The cutover removes
stale doctrine without converting migration into product approval.

## Current State

The disposition manifest is frozen:

| Former backlog file | Classification | Required destination |
| --- | --- | --- |
| `docs/roadmaps/backlog/README.md` | obsolete scaffolding | delete after all five item dispositions land |
| `breaking-command-surface-and-container-compaction.md` | implemented and superseded | delete; current command and container authority already lives in current architecture, contracts, guides, and archived g09 evidence |
| `distribution-channels.md` | implemented and promoted through `g03.020` | delete; retarget live references to guides `042`, `049`, `051`, and `062` as appropriate |
| `release-contract-v0.md` | accepted durable design, mostly promoted | merge the remaining rollback communication, early-v0 support-window, and v1-planning criteria into guide `049`, then delete |
| `g09-candidate-themes.md` | completed or superseded except consumer cohort expansion | create `docs/triage/20260909-152106-consumer-adoption-cohort-expansion.md`; preserve source, scope, constraints, open questions, promotion conditions, owner/next check, and links to vision `007` and `020`; delete the source |
| `vendored-effigy-skill-portfolio-status-and-sync.md` | unresolved deferred candidate | create `docs/triage/20260909-152107-vendored-effigy-skill-portfolio-sync.md`; preserve source, scope, constraints, questions, promotion conditions, owner/next check; keep the matching `PAPERCUTS.md` item open and link it to the triage note; delete the source |

Current triage notes remain open and unchanged except for index/front-door
coverage. Add `docs/triage/README.md` as the non-authoritative intake contract.
Do not promote either new note into a task. Theme 5 release execution is not a
candidate task: current release guides, contracts, and operator gates already
own it.

Canonical planning state after the cutover remains paused: no active
generation, no approved frontier, and no executable task. `docs/roadmaps/README.md`
and `docs/logs/README.md` must continue to direct the operator to Northstar
Atlas before any `g10` rollover.

## Boundaries

In scope:

- delete all six files under `docs/roadmaps/backlog/` and the directory itself;
- add the two reserved timestamped triage notes and `docs/triage/README.md`;
- repair live doctrine, front doors, current guides, vision surfaces, archive
  roll-up links, the one broken archived-spec link, starter assets, manifest
  fixture, local docs checks, changelog, and closeout evidence;
- add an absence check to Effigy's repo-owned docs QA and the bundled
  Northstar starter so `docs/roadmaps/backlog/` is rejected as deprecated
  input; use the smallest existing task/check vocabulary and do not add a new
  public Effigy command;
- update starter tests when the emitted file set or task bundle changes.

Owned mutable paths:

- `docs/roadmaps/backlog/**` (delete)
- `docs/triage/README.md`
- `docs/triage/20260909-152106-consumer-adoption-cohort-expansion.md`
- `docs/triage/20260909-152107-vendored-effigy-skill-portfolio-sync.md`
- `docs/README.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/archive/g02.md`
- `docs/roadmaps/archive/g03.md`
- `docs/roadmaps/archive/g08.md`
- `docs/contracts/001-working-rules.md`
- `docs/guides/014-release-checklist-template.md`
- `docs/guides/041-distribution-ci-pinning-and-wrapper-migration.md`
- `docs/guides/044-distribution-first-publish-execution-runbook.md`
- `docs/guides/049-ci-binary-distribution-and-release-protocol.md`
- `docs/guides/056-northstar-effigy-consumer-repo-contract.md`
- `docs/guides/078-papercuts-discovery-and-capture.md`
- `docs/vision/002-refocus-matrix-v1.md`
- `docs/vision/020-strategic-runway-atlas-v1.md`
- `docs/vision/decisions/D-2026-03-horizon-a-governance-theme.md`
- `docs/specs/archive/034-next-v0-x-readiness-and-roadmap-selection-strict-lane.md`
- `PAPERCUTS.md`
- `config/tasks.toml`
- `crates/effigy-catalog/starters/northstar/starter.toml`
- `crates/effigy-catalog/starters/northstar/effigy.toml`
- `crates/effigy-catalog/starters/northstar/docs/README.md`
- `crates/effigy-catalog/starters/northstar/docs/roadmaps/README.md`
- `crates/effigy-catalog/starters/northstar/docs/triage/README.md` (new)
- `crates/effigy-catalog/src/starter.rs`
- `crates/effigy-doctor/src/manifest_schema/tests.rs`
- `CHANGELOG.md`
- `docs/logs/README.md`
- `docs/logs/archive/2026-09/09-152106-roadmap-backlog-retirement.md` (new)

The worker may omit an owned file when no edit is needed. Any newly discovered
live reference may be changed only when required to prevent broken links or
conflicting live backlog doctrine; report it explicitly in the PR. Historical
logs, vision history, closed handoffs, immutable queue records, and plain-text
provenance stay unchanged. Historical references may name the former path but
must not be live links or authority.

Out of scope: product behavior, new public checker commands, release execution,
`.github/workflows/`, a new product lane, task execution, abandonment or
supersession, generation rollover, and any pre-existing Paseo workspace or
agent. Follow `/Users/tom/Dev/projects/effigy/AGENTS.md`.

## Important Context

- `docs/vision/020-strategic-runway-atlas-v1.md` is stale: it still frames g09
  as future, links the candidate-theme backlog, and names completed `g09.004`
  as next. Reconcile it to the closed g09 state, the two surviving candidate
  triage notes, and Atlas as the only next planning move.
- The cohort candidate belongs to the adoption posture in vision `007`. The
  portfolio-skill candidate also has a live `PAPERCUTS.md` entry.
- Guide `049` currently says to follow `release-contract-v0.md` for pinning and
  rollback. Make the guide self-contained instead of replacing the backlog
  with a compatibility stub.
- Guides `014` and `044`, archive roll-ups `g02`, `g03`, and `g08`, decision
  `D-2026-03-horizon-a-governance-theme`, and archived spec `034` contain live
  links or current directions that will break after deletion.
- The bundled starter currently teaches `## Backlog layout`, omits a triage
  anchor, and does not guard against a reintroduced roadmap backlog. Update
  `starter.toml`, emitted docs, its `effigy.toml`, contract guide `056`, and
  focused starter tests together.
- `crates/effigy-doctor/src/manifest_schema/tests.rs` uses the old distribution
  backlog path as ordinary valid fixture data. Retarget it to guide `049`; do
  not change schema behavior.
- Existing triage notes are non-authoritative. Do not edit their meaning or
  infer a frontier from them.

## Suggested Next Move

Create the disposition destinations first: the triage anchor, two notes, and
the missing durable release material in guide `049`. Then delete the backlog,
repair every live inbound reference, update starter/checker surfaces, and run a
final repository-wide search separating allowed historical provenance from
forbidden live doctrine.

Required proof:

- every row in the disposition manifest is accounted for in the closeout log;
- `find docs -type d -name backlog -print` returns nothing;
- no current executable surface, starter, instruction, fixture, or checker
  requires or teaches a roadmap backlog;
- triage is explicitly non-authoritative and top-level `gNN.NNN` tasks remain
  the only executable planning units;
- current front doors agree: paused, no active generation, no approved
  frontier, Atlas next;
- historical exceptions are listed rather than rewritten wholesale.

Run at least:

```bash
cargo run --bin effigy -- qa:docs
cargo test -p effigy-catalog northstar_starter -- --nocapture
cargo test -p effigy-doctor manifest_schema -- --nocapture
cargo fmt --all -- --check
git diff --check
```

Also execute the new local and starter backlog-absence checks directly. Add
more focused tests only if the touched code/config requires them. Do not run a
release mutation.

## Completion Protocol

### Worker and PR loop

Implement this one maintenance cutover, self-review the exact diff, commit,
push, and open one non-draft PR against `main`. The handoff itself is transport;
do not preserve it as historical project documentation. Report the PR URL,
exact head, disposition table, changed/deleted files, validation, and retained
historical exceptions through the queue callback.

### Independent review and closeout

The queue coordinator owns independent exact-head review, remediation if
needed, merge, synchronization of `/Users/tom/Dev/projects/effigy` to
`origin/main`, and normal closeout. Review must verify both content truth and
the exact worker head. After merge, delete this handoff in the closeout commit,
keep the regular Markdown closeout log, update its log-index count if the
worker did not already do so, run docs QA plus `git diff --check`, commit and
push closeout, and report final task, PR, merge, and closeout commits.

Do not open `g10` or dispatch product work. The continuation envelope is
exhausted after this cleanup. Chatterbox returns to the operator for Northstar
Atlas; the current approved frontier remains none.
