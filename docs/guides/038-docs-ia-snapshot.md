# 038 - Docs IA Review

Use this page when the question is not "where is one guide?" but "is the guide
set itself shaped sensibly?"

This is the current portfolio review for the active guide set.

## Current Judgment

The main problem is not missing docs. It is too many similarly weighted docs,
too many long guides, and too much internal or governance material living in
the same public guide layer as user-facing product docs.

The current front doors are now better:
- [`../../README.md`](../../README.md)
- [`../README.md`](../README.md)
- [`README.md`](./README.md)

The next problem is the portfolio underneath them.

## Keep As Primary

These guides are part of the real product learning path and should stay in the
main user-facing set:

- `021` quick start
- `022` manifest cookbook
- `023` troubleshooting
- `025` command reference
- `055` everyday workflows
- `058` demo system guide
- `059` manifest composition
- `061` Rhai script steps
- `062` distribution system guide
- `063` container system guide
- `064` system, workspace, and dev contract
- `065` underlay starter
- `067` catalog services reference
- `069` workspace host integration
- `070` per-machine overlays and external mounts

## Keep As Secondary Deep Dives

These are valid docs, but they should not compete with onboarding pages:

- `016` task routing precedence
- `017` JSON output contracts
- `018` doctor explain mode
- `019` watch, init, and migrate
- `024` CI and automation recipes
- `026` JSON payload examples
- `028` migration quick paths
- `048` built-in test suite lifecycle and env
- `050` env schema integration
- `051` release orchestration
- `052` changelog workflows
- `056` Northstar + Effigy consumer repo contract

## Merge Candidates

These guides look real, but the portfolio is paying too much fragmentation cost:

| Guides | Problem | Likely End State |
| --- | --- | --- |
| `041`, `042`, `044`, `049` | release/distribution policy is split across too many pages | one public operator path plus one maintainer policy/reference layer |
| `029`, `035`, `037`, `039`, `040` | docs operations are spread across overlapping maintenance/process pages | one docs maintenance playbook plus one archive/deprecation policy |
| `027` and parts of `022` | snippet content competes with the cookbook | fold high-value snippets into `022`, demote the rest to reference |
| `024` and parts of `026` | CI recipes and JSON examples overlap heavily | keep both only if they stay clearly recipe vs sample-reference |

## Rename Candidates

These names are technically accurate but not strong enough for new readers:

| Current | Problem | Better Direction |
| --- | --- | --- |
| `064-system-workspace-and-dev-contract` | sounds internal and abstract | now partially improved in-page, but the filename still wants a future rename toward local-dev framing |
| `062-distribution-system-guide` | "distribution" is not obvious to many readers | consider a release-and-distribution operator framing |
| `048-built-in-test-suite-lifecycle-and-env` | too wide and too abstract | narrow to testing and env behavior |
| `056-northstar-effigy-consumer-repo-contract` | correct but intimidating | likely keep as advanced/adoption reference, not a front-door guide |

## Archive Candidates

These should not stay in the active guide conversation:

| Guide | Why |
| --- | --- |
| `045` vision next-task allowlist maintenance | narrow internal policy, not a public product guide |
| `046` vision next-task allowlist PR checklist snippet | PR support artifact, not a user-facing guide |
| older historical cleanup records already under `archive/` | correct where they are; do not promote them again |

## What This Means For The Next Sweep

The next useful consolidation batches are:

1. Release/distribution docs:
   - done: `041`, `042`, and `044` are deprecated
   - done: `049`, `051`, and `062` now have cleaner, less overlapping roles
   - remaining cleanup: trim depth further only if real user confusion remains
2. Docs-maintenance docs:
   - done first pass: `037` is now the primary maintenance playbook, `029` is the active QA checklist, `040` is the archive/deprecation policy, and `035` plus `039` are now deprecated
   - remaining cleanup: reduce overlap inside `029` and trim stale policy detail from the deprecated pair if they stay in-tree long term
3. Guide naming:
   - rename the most abstract user-facing titles without rewriting every deep
     page at once
4. Oversized active guides:
   - done: `022`, `026`, `063`, and `064` now have much better structure and
     entry framing
   - done: `025` is now a much tighter lookup page instead of a giant command
     wall
   - remaining cleanup: keep trimming only where real user confusion still
     shows up

## Expected Outcome

- maintainers can tell which guides are core, secondary, merge candidates, or
  archive candidates
- future docs cleanup can happen in deliberate batches instead of random prose
  churn
- the front doors can stay clean while the deeper guide set is reduced

## Related Guides

- [`../README.md`](../README.md)
- [`README.md`](./README.md)
- [`040-docs-archive-and-deprecation-policy.md`](./040-docs-archive-and-deprecation-policy.md)

## Next Step

Take the release/distribution cluster next and reduce it to a smaller public
surface before touching the docs-maintenance cluster.
