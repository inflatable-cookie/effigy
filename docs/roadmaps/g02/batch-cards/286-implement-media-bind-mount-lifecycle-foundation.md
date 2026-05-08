# 286 Implement Media Bind-Mount Lifecycle Foundation

Status: archived
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Make the next bounded `g02.015` contract real by adding manifest-owned media
bind-mount lifecycle declarations on the generated-compose path.

## Context

`280`, `282`, and `284` now cover retention, inventory, and transfer for named
volumes. The remaining lifecycle gap before hooks is media data: uploads and
other repo-owned bind-mounted assets still have no first-class
container-data-specific contract surface.

This is still lifecycle foundation work. It stays closer to the core data
contract than task-owned seeding or `pull_production` orchestration.

## In Scope

- add bounded `[containers.<name>.data].media` manifest support
- carry media declarations through manifest loading, effective container
  policy, and generated-compose output
- keep the batch on generated-compose ownership where Effigy controls compose
  assembly honestly
- add focused coverage for manifest parsing, compose generation, and container
  policy/report shaping where needed

## Out Of Scope

- direct `compose_file` ownership widening
- media export/import commands
- task-owned seeding orchestration
- `pull_production` hooks
- real-project proof or migration-bundle automation

## Acceptance

- generated-compose containers can declare bounded media bind mounts through a
  manifest-owned data surface instead of folding them into generic host mounts
- compose generation reflects those declarations honestly
- the resulting contract stays explicit about generated-compose ownership and
  does not pretend to manage broader media lifecycle operations yet
- focused tests cover manifest, compose, and policy behavior

## Result

Generated-compose containers can now declare `[containers.<name>.data].media`
with repo-relative source paths and absolute container targets.

What landed:

- manifest-owned `data.media` parsing on the container path
- generated-compose rewrite that prepares repo-owned media directories and
  mounts them onto generated services that already bind the repo root
- explicit rejection for direct `compose_file` ownership on this bounded path
- policy and status/report shaping that keeps media mounts separate from
  generic host mounts
- focused coverage for policy loading, compose generation, and CLI/report
  projection

The lane now stops in planning before widening into task-owned seeding or
`pull_production` hooks.

## Next Task

Execute [`287-plan-post-media-data-orchestration-batch.md`](./287-plan-post-media-data-orchestration-batch.md)
to choose the next bounded `g02.015` widening step after media lifecycle
foundation.
