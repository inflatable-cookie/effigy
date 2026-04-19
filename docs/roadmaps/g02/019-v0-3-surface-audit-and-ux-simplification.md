# 019 - v0.3 Surface Audit And UX Simplification

Generation: `g02`

Status: Planned
Owner: Platform
Created: 2026-04-19
Depends on: 007, 011, 012, 013, 014, 015, 016

## Vision Alignment

The `v0.3` product surface is now materially broader than the old
task-and-release-only shape:

- service catalog ownership is shipped
- transparent container execution is shipped
- the gateway and route lifecycle are shipped
- managed dev lifecycle ownership is shipped
- persistent data and multi-project coordination are shipped
- the distribution and release surfaces are stronger and more native than they
  were in `v0.2.13`

The problem is no longer "is there enough product here?".

The problem is that the shipped surface is explained unevenly. Some of the
highest-value commands and flows are real in code and help output, but they are
still hidden, under-explained, or described with stale boundaries in the front
doors.

This roadmap exists to tighten the operator story before and around the `v0.3`
release instead of shipping a broader product with a narrower or drifted public
explanation.

## Primary Tags

- `UX`
- `DOCS`
- `ROUTE`

## Target Envelope

- the front-door docs present the real shipped `v0.3` command surface rather
  than an older smaller subset
- the local-dev operator story reads as one coherent chain instead of several
  isolated features
- distribution and release docs state the supported boundary bluntly and
  consistently
- roadmap, strict-lane, guide, and help references stop disagreeing about
  release intent or shipped capability
- simplification work favors fewer clearer entrypoints over more specialist
  prose

## Vision Target Delta

- Move from `broad v0.3 feature set with drift between help, guides, and
  planning refs` toward `one aligned and simpler operator-facing product story
  for the shipped v0.3 surface`.

## Problem

The release audit exposed a surface-alignment problem rather than one new class
of deep runtime breakage.

The main gaps are:

- the primary command matrix omits meaningful shipped surfaces such as
  `service`, `exec`, and `gateway`, and it under-describes newer `container`
  and `distribution` subcommands
- the everyday workflow front doors still teach an older Effigy shape and do
  not make the new local-dev path feel default
- the container system guide still describes stale limits after gateway, route,
  TLS, data, and cross-project status work already landed
- release-planning refs still contain stale version intent that conflicts with
  the deliberate `v0.3` path
- the managed dev front-door story is split between older TUI framing and the
  newer repo-owned managed lifecycle contract
- the distribution surface is honest but still needs one blunt supported-
  boundary statement so release messaging does not over-claim consumer
  generality

If those gaps stay open, `v0.3` risks feeling more complicated and less
finished than it actually is.

## Goals

- align the command-reference front door with the actual shipped CLI surface
- align the main workflow guides with the current local-dev and release paths
- simplify the local-dev story around one readable operator chain:
  `service` -> `container` -> `gateway` -> `exec` -> repo-owned managed task
- clean up stale release-version and release-boundary references across active
  planning docs
- define one explicit supported-boundary statement for distribution and release
  reuse outside Effigy self-hosting
- identify and close any remaining UX friction that is purely discoverability,
  naming, or guidance debt rather than missing substrate

## Non-Goals

- this roadmap does not replace `g02.007` as the active release-prep lane
- this roadmap does not authorize release execution
- this roadmap does not reopen shipped container, gateway, data, or demo
  runtime foundations unless the shipped boundary itself turns out to be
  dishonest
- this roadmap does not widen the distribution surface beyond its current
  intentionally bounded contract just to make the docs look more complete
- this roadmap does not encourage copy-heavy "tour guide" documentation when a
  smaller command or guide reshaping is enough

## Workstreams

### 1. Surface Inventory And Front-Door Alignment

Primary write set:

- `docs/guides/025-command-reference-matrix.md`
- `docs/guides/055-everyday-workflows.md`
- `crates/effigy-cli/src/help/topics/**`

Scope:

- make the command matrix reflect the real top-level shipped surface
- add missing first-class command entries where the CLI already exposes them
- tighten the quick-pick guidance so operators can find `service`, `exec`,
  `gateway`, richer `container`, and fuller `distribution` paths without
  reading deep-dive guides first
- reduce drift between front-door docs and help output

Why this matters:

- the current front door understates the actual product and hides value

### 2. Local Dev Story Simplification

Primary write set:

- `docs/guides/063-container-system-guide.md`
- `docs/guides/012-dev-process-manager-tui.md`
- `docs/guides/055-everyday-workflows.md`
- related command/help references where needed

Scope:

- rewrite the local-dev story so the shipped surfaces read as one operator path
  instead of separate milestone leftovers
- decide how the older `effigy dev` framing and the newer managed-task
  contract should coexist in docs
- make the relationship between service fragments, container bring-up, gateway
  DNS/TLS, ad-hoc exec, and repo-owned managed tasks obvious
- prefer a smaller clearer narrative over several overlapping partial guides

Why this matters:

- `v0.3` should feel simpler than the sum of its milestones

### 3. Release And Distribution Boundary Cleanup

Primary write set:

- `docs/specs/007-distribution-release-and-consumer-rollout-strict-lane.md`
- `docs/roadmaps/g02/007-distribution-release-and-consumer-rollout.md`
- `docs/guides/062-distribution-system-guide.md`
- `docs/guides/049-ci-binary-distribution-and-release-protocol.md`
- `docs/guides/051-release-orchestration.md`

Scope:

- remove stale `v0.2.14` intent where the real path is now `v0.3`
- state the current distribution reuse boundary consistently across planning and
  operator guides
- make sure release-facing docs describe the native command path, the current
  migration floor, and the still-bounded consumer-generalization posture
- keep the release story blunt enough that operators and agents do not infer
  unsupported guarantees

Why this matters:

- release drift in active planning refs is cheap to create and expensive to
  clean up later

### 4. UX Gap Sweep

Primary write set:

- active front-door guides
- CLI help topics
- release notes / changelog framing if needed

Scope:

- audit the remaining friction exposed by the surface review and separate:
  - missing docs
  - naming confusion
  - command-shape overload
  - genuine product gaps
- close the first three directly when the fix is small and honest
- only escalate a gap into new product work if the issue cannot be solved by
  better shaping of the existing surface

Why this matters:

- `v0.3` polish should remove operator uncertainty, not create another long
  tail of optional cleanup

## Exit Condition

This milestone is complete when the shipped `v0.3` surface is presented through
aligned front doors, the supported release/distribution boundary is stated
consistently, and the main local-dev and release paths feel simpler and more
discoverable without inventing a new product scope.

## Next Task

Keep `g02.007` as the active release-prep lane until the deliberate `v0.3`
release path is settled.

Then use this roadmap to execute the post-audit alignment batch across the
front-door guides, release refs, and CLI help surfaces.
