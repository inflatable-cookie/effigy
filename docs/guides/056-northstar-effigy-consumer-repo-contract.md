# 056 - Northstar + Effigy Consumer Repo Contract

Use this guide when you want a consuming repository to mean the same thing when
an agent or maintainer says "use Northstar and Effigy."

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Target movement: consumer repos move from partial Effigy adoption or partial
  Northstar structure toward one reusable docs, planning, validation, and
  release contract.

## Start Here

Use this guide when you are preparing a consuming repo to:

- scaffold or normalize the docs skeleton
- make Effigy the default execution surface
- add changelog and release readiness
- validate that the repo contract still holds after edits

Start with this sequence:

```sh
effigy tasks
effigy doctor
effigy test --plan
```

Then inspect the repo against the file and task checklist in Sections 1 and 2.

Before you scaffold anything, decide which repo shape you are dealing with:

- single repo: docs, changelog, and validation all live at the repo root
- workspace container: the root repo orchestrates child repos, while one nested
  repo acts as the documentation authority

Do not force the single-repo template onto a workspace container if the docs
authority is already a separate repo.

## 1) Minimum Contract

For a consumer repo to claim "use Northstar and Effigy" as a real operating
contract, it should provide all of the following.

### Required files

```text
README.md
AGENTS.md
effigy.toml
CHANGELOG.md
docs/README.md
docs/vision/README.md
docs/roadmaps/README.md
docs/logs/README.md
```

Workspace-container note:
- the full required set applies to the repo that owns the Northstar planning
  spine
- a thin orchestration root does not need to duplicate that file set if a
  nested docs-authority repo already owns it

### Recommended file groups

```text
docs/guides/
docs/contracts/
docs/architecture/
docs/research/
```

The required set is the minimum bar for shared semantics. The recommended
groups depend on repo size and product complexity.

### Workspace-container exception

When a project uses a thin workspace root plus a nested docs-authority repo:

- keep the workspace root focused on orchestration surfaces such as `AGENTS.md`,
  `README.md`, `package.json`, and root `effigy.toml`
- make the nested docs-authority repo carry the Northstar docs skeleton,
  `qa:docs`, `qa:northstar`, and any docs-policy configuration
- route the workspace root through the docs-authority repo's `qa` surface
  instead of duplicating the docs contract at the root
- keep release posture and changelog requirements on the repos that are
  actually releasable; do not force them onto the orchestration root or a
  docs-only authority repo unless those repos really ship artifacts

Treat that as a first-class contract shape, not as drift.

## 2) Minimum Effigy Surface

The root `effigy.toml` should expose one obvious operator path for discovery,
health, testing, validation, and release readiness.

Do not duplicate that surface through `package.json` scripts. `package.json`
scripts should stay package-native; agents and humans should run
`effigy <task>` directly.

### Required command semantics

- `effigy tasks`
  - discovery surface for supported repo work
- `effigy doctor` or `effigy health`
  - default health/routing surface
- `effigy test --plan`
  - default test inspection surface
- `effigy qa`
  - top-level validation bundle

### Required tasks

At minimum, the repo should expose:

- `qa`
- `validate`
- `health` or a repo-owned equivalent surfaced through `doctor`

### Recommended tasks

- `qa:docs`
- `qa:northstar`
- `qa:json`
- `build`
- `dev`

### Demo adoption note

If the consumer repo also has an operator-visible proof or demo surface:

- keep the native demo registry in `[demos.<id>]`
- prefer a dedicated fragment such as `demos/effigy.demos.toml` once the proof
  surface is non-trivial
- let demos carry inline `run = [ ... ]` sequences when wrapper tasks add no
  real reuse

Use these pages for the practical detail instead of re-explaining them here:

- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)

### Default test policy

Pick one of these and document it explicitly:

- built-in `effigy test` is the default test entrypoint
- explicit `tasks.test` is the repo-owned source of truth

Do not leave this ambiguous in multi-runner repos.

## 3) Minimum AGENTS Contract

`AGENTS.md` should tell a fresh agent exactly how to operate the repo.

Minimum semantics:

1. start with `effigy tasks`
2. use `effigy doctor` or `effigy health`
3. inspect tests with `effigy test --plan`
4. prefer `effigy <task>` for supported repo work
5. use `effigy --json <command>` for machine consumers
6. use `--repo <PATH>` only when intentionally targeting another repo
7. fall back to raw tools only when Effigy does not cover the path

Minimum policy notes:

- state whether built-in `effigy test` or explicit `tasks.test` is the default
- identify the docs authority location
- identify any allowed fallback boundaries such as release wrappers or local
  bootstrap helpers

## 4) Minimum Northstar Docs Skeleton

### `docs/README.md`

Should answer:

- what the docs folders mean
- where a newcomer starts
- what the current active planning surfaces are

### `docs/vision/README.md`

Should answer:

- what vision artifacts exist
- which artifact is the current product vision
- which artifact tracks milestone or outcome sequencing

At least one vision document should define:

- long-term outcome
- strategic constraints
- target envelopes or success conditions
- explicit next task

### `docs/roadmaps/README.md`

Should answer:

- generation model
- active milestone queue
- backlog layout
- next roadmap task

At least one active roadmap generation should exist.

### `docs/logs/README.md`

Should answer:

- log naming and segmentation
- cadence rule
- what counts as meaningful batch evidence

### Vision and roadmap semantics

At minimum:

- vision defines the long-horizon outcome and constraints
- roadmaps define milestone batches and sequencing
- logs capture evidence and decisions

Do not collapse all three into a single generic planning note.

## 5) Changelog and Release Minimum Bar

Every repo that is being prepared for real release work should have:

- `CHANGELOG.md`
- a documented release strategy
- a root `[release]` section in `effigy.toml` once release work should run
  through Effigy

### Minimum changelog expectation

Use the Northstar Changelog Profile shape:

```md
# Changelog

## [Unreleased]

### Added
- New capability
```

### Minimum release expectation

Smallest useful release config:

```toml
[release]
changelog = "CHANGELOG.md"
tag-format = "v{version}"
```

Then add `[release.gates]` once the repo is ready for machine-checked release
readiness.

### Release boundary for workspace containers

Do not assume the docs-authority repo itself needs a release config. If the
workspace root is not the releasable artifact and the docs-authority repo is
documentation-only, keep release posture on the actual releasable code repos
until there is a real docs-release requirement.

## 6) Minimum Validation Bundle

The contract should be inspectable by commands, not only by humans.

### Required validation outcome

The repo should have one obvious path that tells a maintainer whether the
contract is still intact.

Recommended shape:

```toml
[tasks]
"qa:docs" = "..."
"qa:northstar" = "..."
qa = [{ task = "validate" }, { task = "qa:docs" }, { task = "qa:northstar" }]
```

### Recommended Northstar checks

Use Effigy built-ins plus task composition for:

- required path presence for the repo front door and docs spine
- docs link validation
- docs index consistency
- required headings or metadata
- next-action coverage where the repo uses that policy
- forbidden copied defaults such as `--repo .` in active agent/setup/workflow
  surfaces

Example:

```toml
[tasks]
"qa:docs:agent-defaults" = "effigy docs check forbidden AGENTS.md README.md .github/workflows/ci.yml --forbid '--repo .'"
```

### Starter `qa:northstar` bundle

Pair the task bundle with this minimal native `[docs_policy]` config:

```toml
[docs_policy.indexes.vision]
file = "docs/vision/README.md"
dir = "docs/vision"
section = "Vision Artifacts"
exclude = ["history/**"]

[docs_policy.next_actions.vision]
index = "vision"
heading = "## Next Task"
allowlist_file = "docs/policy/vision-next-task-verbs.txt"
```

Starter policy file:

```text
ship
review
execute
define
document
validate
```

Use this as the default starter shape unless the repo already has a richer
equivalent:

```toml
[tasks]
"qa:northstar:spine" = "effigy docs check paths README.md AGENTS.md docs/README.md docs/vision/README.md docs/roadmaps/README.md docs/logs/README.md docs/policy/vision-next-task-verbs.txt"
"qa:northstar:agent-contract" = "effigy docs check contains AGENTS.md --require 'effigy tasks' --require 'effigy test --plan' --require 'docs/README.md' --require 'docs/vision/README.md' --require 'docs/roadmaps/README.md' --require 'docs/logs/README.md'"
"qa:northstar:readme" = "effigy docs check contains README.md --require 'docs/README.md'"
"qa:northstar:docs-front-door" = "effigy docs check contains docs/README.md --require 'vision/README.md' --require 'roadmaps/README.md' --require 'logs/README.md'"
"qa:northstar:indexes" = "effigy docs check index --policy-index vision"
"qa:northstar:next-action" = "effigy docs check next-action --policy vision"
"qa:northstar:headings" = "effigy docs check headings docs/vision/README.md --require-heading '## Current Vision'"
"qa:northstar:agent-defaults" = "effigy docs check forbidden AGENTS.md README.md --forbid '--repo .'"
"qa:northstar" = [
  { task = "qa:northstar:spine" },
  { task = "qa:northstar:agent-contract" },
  { task = "qa:northstar:readme" },
  { task = "qa:northstar:docs-front-door" },
  { task = "qa:northstar:indexes" },
  { task = "qa:northstar:next-action" },
  { task = "qa:northstar:headings" },
  { task = "qa:northstar:agent-defaults" },
]
```

Treat the bundle as layered:

- Effigy-native checks own the generic engines:
  `check-paths`, `check-index`, `check-next-action`, `check-headings`,
  `check-forbidden`
- repo manifests own policy names, file paths, and any repo-specific required
  headings
- the `northstar-effigy` skill should scaffold this starter bundle, not invent
  a different validation vocabulary per repo

### Product boundary

Keep these pieces Effigy-native:

- generic markdown validation engines
- generic path-presence validation for repo/docs contract surfaces
- release validation and install verification
- docs-policy manifest plumbing

Keep these pieces in the skill/templates layer:

- starter file creation
- repo-shape choice between single repo and workspace container
- concrete docs skeleton prose
- repo-specific heading inventories and policy file contents

Current decision:
- keep bootstrap scaffolding in the `northstar-effigy` skill/templates for now
- revisit an Effigy-native bootstrap surface only if later adoption shows
  repeated pain that the current templates cannot cover cleanly

## 7) Starter File Set

When the repo is missing the contract, the starter set should be created in
this order:

1. `effigy.toml`
2. `README.md`
3. `AGENTS.md`
4. `docs/README.md`
5. `docs/vision/README.md`
6. first vision document
7. `docs/roadmaps/README.md`
8. first roadmap generation README and first active milestone
9. `docs/logs/README.md`
10. `CHANGELOG.md`
11. `[release]` config when the repo is actually being prepared for releases

## 8) Adoption Levels

### Level 0 - Effigy present only

- `effigy.toml` exists
- task surface exists
- docs and release contract are not standardized

### Level 1 - Effigy-first repo

- `AGENTS.md` teaches the Effigy-first loop
- default test semantics are explicit
- top-level validation path exists

### Level 2 - Northstar docs present

- `docs/vision`, `docs/roadmaps`, and `docs/logs` exist
- the docs authority is clear
- vision, roadmap, and log roles are distinct

### Level 3 - Contract-enforced repo

- changelog and release baseline exist
- docs and agent drift are validated through Effigy tasks
- the repo can be normalized by a reusable agent skill rather than bespoke
  handholding

Consumer repos should not claim the full phrase contract until they are at
least Level 3.

## 9) First Pilot Checklist

Use this checklist when assessing a candidate consumer repo:

- [ ] Root `effigy.toml` exists
- [ ] `README.md` links to `docs/README.md`
- [ ] `AGENTS.md` teaches the Effigy-first loop
- [ ] `--repo .` is not taught as a default
- [ ] `docs/README.md` exists and names the docs authority
- [ ] `docs/vision/README.md` exists
- [ ] `docs/roadmaps/README.md` exists
- [ ] `docs/logs/README.md` exists
- [ ] vision document defines long-term outcome and constraints
- [ ] roadmap queue exists and has a clear next milestone
- [ ] `CHANGELOG.md` exists
- [ ] release baseline is documented or intentionally deferred
- [ ] repo has a contract validation path beyond raw human review

## Expected Outcome

- "use Northstar and Effigy" becomes a concrete repo contract instead of a
  repo-local phrase
- consumer repos get one minimum bar for docs, validation, and release
  readiness
- the future `northstar-effigy` skill has a crisp source of truth instead of
  inventing structure per repo

## Related Guides

- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`051-release-orchestration.md`](./051-release-orchestration.md)
- [`052-changelog-workflows-and-northstar-profile.md`](./052-changelog-workflows-and-northstar-profile.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)

## Next Step

Assess the first pilot repo against the checklist in Section 9, then normalize
the missing surfaces before building the reusable `northstar-effigy` skill
bundle. If the repo also needs native demo adoption, continue with
[`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
instead of scattering demo migration notes across this contract page.
