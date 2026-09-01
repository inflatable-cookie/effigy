---
title: Catalog-pack support-floor worker
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-201505-catalog-pack-support-floor-1103.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, catalog-pack, support-policy]
---

## Objective

Execute card `1103`: establish Effigy's machine-readable catalog-pack update
support floor and its local typed validation.

## Launch State

- Repository: `/Users/tom/Dev/projects/effigy`
- Planning base: `ad26d03a97ee23b6d3060d6e0a4e8bb49bedb4e4`
- Worker branch: `worker/g08-048-catalog-pack-support-floor-1103`
- Roadmap: `docs/roadmaps/g08/048-catalog-pack-publication-and-cutover.md`
- Spec: `docs/specs/115-catalog-pack-publication-and-cutover-strict-lane.md`
- Card and review oracle:
  `docs/roadmaps/g08/batch-cards/1103-establish-catalog-pack-support-floor.md`
- Contracts: `001`, `043`
- Required sibling links: none
- Allowed runway: card `1103` only; one PR
- Worker class: day-to-day
- Worker-profile reason: bounded typed policy data, validator, failure matrix,
  and docs closeout; material compatibility semantics are explicit in the card
  and retained for orchestrator review
- Frontier implementation justification: none

## Ready-Frontier Shape

This is the sole Ready card. Card `1104` is serial because the pack repository
must consume an Effigy-owned file already landed on pushed `main`. Cards `1105`
through `1108` remain behind real publication, artifact, and generated-baseline
dependencies. Do not promote or begin them.

## Ownership

Own `support/catalog-pack-update.toml`, one typed Effigy parser/validator and its
focused tests, directly related support-policy docs, card `1103`, roadmap/spec
state needed to close only this card, and one unique evidence log. Reuse an
existing suitable crate owner; do not create a new crate or runtime selection
path for this policy file.

The orchestrator owns exact-head review, merge, downstream card promotion, and
all external repository/publication decisions.

## Required Initial Data

The current released Effigy version is `0.12.1`. Commit:

```toml
schema_version = 1
as_of_release = "0.12.1"
required_versions = ["0.12.1"]
```

Do not include `oldest_update_capable_release`: no released Effigy version yet
exposes public `service pack update`.

## Boundaries

- Validation is local and network-free. Remote release existence, latest-release
  freshness, resolved commit/blob, and candidate compatibility belong to pack
  publication card `1104`/`1105`.
- The required set is nonempty, duplicate-free semantic versions and must include
  the current Cargo release. `as_of_release` must equal that current release.
- In the current pre-update state, the oldest field is forbidden. Model and test
  the future invariant that, once update capability exists, it equals the
  minimum required version without claiming a release now exposes update.
- Reject unknown fields and unsupported schema versions.
- The file is Effigy support-policy authority only. It must not affect runtime
  pack selection, acquisition, or activation.

Do not edit `.github/workflows/`, create the pack repository, move catalog
assets, generate the snapshot, replace the official coordinate, expose public
update, tag, publish, change package visibility, move `stable`, release Effigy,
touch S3, or address unrelated papercuts.

## Review Oracle

Falsify every counterexample in card `1103`, including empty/duplicate/malformed
sets, current/as-of mismatch, premature or inconsistent oldest field, unknown
schema/data, network/runtime coupling, policy-owner inversion, and scope drift.

## Validation And Evidence

Run focused support-policy tests, `effigy qa:docs`, `effigy qa`,
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and
`git diff --check`. Write one dated evidence log mapping each oracle row to
exact tests and results. Record the final data, parser owner, network-free
boundary, and changed-file inventory.

## Completion Protocol

Before broad reads, verify a clean registered non-main worktree, fetch with
bounded noninteractive SSH, confirm `HEAD == origin/main`, confirm the planning
base is an ancestor, and confirm this absolute handoff is tracked at `HEAD`.
Read `AGENTS.md`, the roadmap, spec, card, and contracts. Implement only card
`1103`, commit, push, and open one PR to `main`. Report the PR URL, exact head,
validation, unresolved items, and docs QA classification. Do not merge. Review
revisions return to this same worker.
