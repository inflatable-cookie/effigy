---
title: Acowtancy migration and deployment continuation handoff
status: active
owner: Platform
updated: 2026-05-11
tags: [coordination, handoff, acowtancy, deployment, state, oci]
---

## What This Thread Was Doing

This thread took the original Acowtancy migration problem and pushed the
generic parts into Effigy.

The starting problem was not a one-shot data import. Acowtancy has a live
legacy MySQL site, a new Underlay/Farmyard/Postgres site, media that must stay
referenced, UAT content created in the new system before go-live, and repeated
legacy refreshes before cutover. The old migration code in Acowtancy is
sprawling because it mixes app-specific transforms with orchestration,
artifact transport, state replay, capture, and deployment concerns.

Effigy now has the app-agnostic outer frame:

- OCI artifact transport and staging.
- State stacks for ordered schema/seed/import/overlay/capture layers.
- Named state capture profiles.
- Rhai state API for capture tasks.
- Remote bundle source support for git and OCI delivery.
- Deployment transaction commands for named environments.
- Deployment status, history, and evidence-backed redeploy reports.

The next thread should turn those generic features into a practical Acowtancy
operator plan: concrete Acowtancy `effigy.toml`/import config, Farmyard tasks,
artifact naming, capture/rebase workflow, UAT deployment flow, and final
readiness checklist.

## Why It Matters

Acowtancy needs a repeatable pre-go-live loop:

1. Snapshot the old site.
2. Transform legacy database/media into replayable OCI-backed state artifacts.
3. Build a fresh new-site baseline.
4. Deploy UAT.
5. Let admins create new-system-only content.
6. Freeze UAT.
7. Capture UAT-authored content as a replayable overlay.
8. Refresh legacy-derived artifacts from a newer old-site snapshot.
9. Reconcile offline.
10. Rebuild and redeploy a clean UAT or production candidate.

Effigy should own the orchestration and evidence trail. Acowtancy should still
own transforms, conflict decisions, and media semantics.

The final solution needs to make that split operational, not theoretical.

## Current State

- Done so far: Effigy has the state-stack framework, OCI artifact substrate,
  remote bundle source support, deployment transaction surface, report history,
  and canonical documentation for their boundaries.
- Still open: Acowtancy has not yet been rebased onto the new Effigy surfaces
  end-to-end. The next work is concrete cross-repo integration planning and
  then implementation inside `/Users/tom/Dev/projects/acowtancy`.
- Active spec lane: none. Effigy `g04` is closed for this feature group.
- Canonical refs:
  - `/Users/tom/Dev/projects/effigy/docs/contracts/014-artifact-substrate-contract.md`
  - `/Users/tom/Dev/projects/effigy/docs/contracts/019-deployment-transaction-system-contract.md`
  - `/Users/tom/Dev/projects/effigy/docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md`
  - `/Users/tom/Dev/projects/effigy/docs/roadmaps/g04/027-deployment-transaction-system.md`
  - `/Users/tom/Dev/projects/effigy/docs/roadmaps/g04/028-deployment-config-plan-and-reporting.md`
  - `/Users/tom/Dev/projects/effigy/docs/roadmaps/g04/029-railway-deployment-adapter.md`
  - `/Users/tom/Dev/projects/effigy/docs/roadmaps/g04/030-render-deployment-adapter.md`
  - `/Users/tom/Dev/projects/effigy/docs/roadmaps/g04/031-deployment-status-history-and-redeploy.md`
  - `/Users/tom/Dev/projects/effigy/docs/roadmaps/g04/032-acowtancy-deployment-proof-and-closeout.md`
  - `/Users/tom/Dev/projects/acowtancy/ledger/planning/migration-execution/legacy-migration-problem-space-and-rebase-loop.md`
- Relevant Effigy commits:
  - `936f7e8d Document deploy transaction API`
  - `69c62c7e Add deployment transaction surface`
- Remaining continuation envelope: plan Acowtancy integration first; do not
  open another Effigy implementation lane until Acowtancy exposes a concrete
  framework gap.
- Lane budget / pause signal: fresh-thread boundary requested by the user.

Key local files:

- `/Users/tom/Dev/projects/effigy/AGENTS.md`
- `/Users/tom/Dev/projects/effigy/docs/guides/025-command-reference-matrix.md`
- `/Users/tom/Dev/projects/effigy/docs/guides/026-json-payload-examples.md`
- `/Users/tom/Dev/projects/effigy/src/runner/deploy_command/transaction.rs`
- `/Users/tom/Dev/projects/acowtancy/ledger/planning/migration-execution/legacy-migration-problem-space-and-rebase-loop.md`

## Boundaries

- Keep Effigy app-agnostic. Do not put Acowtancy transforms, row merge policy,
  paper-specific conflict logic, or media rewrite semantics into Effigy.
- Do not treat `deploy export` as live deployment. It remains file export.
  `deploy apply` is the transaction surface.
- Do not make Effigy create provider projects, services, resources, domains,
  variables, or secrets.
- Do not promise database/media rollback. `deploy redeploy` is replay of
  recorded immutable inputs, not rollback.
- Do not run release prepare/execute unless the user explicitly asks. Release
  execution remains human-owned.
- Do not edit `.github/workflows/` without explicit approval.
- Follow repo constraints from `/Users/tom/Dev/projects/effigy/AGENTS.md` and
  the Acowtancy repo instructions before touching Acowtancy files.

## Important Context

Effigy state config is normal manifest config.

The user explicitly pushed back on a separate discovery file for state config.
The accepted shape is `[state.<name>]` in normal composed Effigy config, with
normal `manifest.import`/include doing the splitting if needed. If there is
only one declared state stack, no `default = "uat"` is needed.

Named capture profiles matter.

The user also pushed for concise repeated commands. The intended Acowtancy
capture command is:

```sh
effigy state capture uat new-content --yes --push
```

not a long command full of env vars. Repo-owned Rhai capture tasks should use
the Effigy state API:

```rhai
let context = state::capture_context();
let output = state::capture_source();
```

Deployment config is also normal manifest config.

Expected shape:

```toml
[deploy.uat]
provider = "railway"
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "acowtancy-uat"
artifact_policy = "digest-preferred"

[deploy.production]
provider = "railway"
state = "production"
code_ref = "release-tag"
release_policy = "required"
provider_project = "acowtancy-production"
artifact_policy = "digest-pinned"
```

The current deployment implementation provides the report-backed transaction
surface and provider-neutral boundary. Treat provider live mutation as a point
to verify against the current code before relying on it operationally. The
contract records the intended full adapter behavior; the implementation landed
the first transaction/report surface.

The Acowtancy canonical problem doc already explains the rebase loop and must
remain the source of truth for the business/migration problem:

- `/Users/tom/Dev/projects/acowtancy/ledger/planning/migration-execution/legacy-migration-problem-space-and-rebase-loop.md`

## Suggested Next Move

Start in the Acowtancy repo and produce an actionable integration plan before
editing broad app code.

Recommended first prompt for the new thread:

```text
Read `/Users/tom/Dev/projects/effigy/docs/handoffs/acowtancy-migration-deployment-continuation.md`
and the Acowtancy migration problem doc it references.

Then inspect `/Users/tom/Dev/projects/acowtancy` for:
- existing Effigy config and manifest imports
- current state config, if any
- Farmyard migration/reset tasks
- OCI artifact build/push scripts
- Rhai capture task surface
- deployment/export config
- docs that mention the migration/rebase loop

Produce a concrete Acowtancy implementation plan that ties Effigy state,
OCI artifacts, remote bundle sources, deployment transactions, UAT capture,
and production release policy into one operator workflow. Do not implement
until the plan identifies exact files, command flow, missing tasks, and any
Effigy framework gaps.
```

The first concrete deliverable should be a short Acowtancy plan with:

- target `effigy.toml`/import layout
- `[state.uat]` and `[state.production]` stack shape
- capture profile names and artifact refs
- OCI bundle naming and digest policy
- `deploy.uat` and `deploy.production` config
- operator command sequence for initial UAT, UAT freeze/capture, rebase, and
  production candidate
- list of app-owned tasks/scripts that must exist
- list of Effigy gaps, if any, with proof from the current implementation

## Completion Protocol

1. Keep Effigy changes out of scope unless Acowtancy integration proves a real
   framework gap.
2. If Effigy needs changes, open a new roadmap/spec lane before implementation.
3. Update the Acowtancy canonical problem doc when the operator workflow
   becomes concrete.
4. Keep app-owned migration details in Acowtancy docs/code, not Effigy.
5. Validate Acowtancy commands with focused dry-run/plan commands first.
6. Record any live-provider assumptions separately before UAT deployment.
7. End the next thread with either an implemented Acowtancy workflow or a
   precise blocked list with file paths and commands.

Validation note for this handoff:

- `effigy tasks` succeeded.
- `effigy doctor` reported one existing health task failure and four god-file
  warnings. The failure was not investigated as part of this handoff.
