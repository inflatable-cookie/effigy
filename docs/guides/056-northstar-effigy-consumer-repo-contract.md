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

- docs link validation
- docs index consistency
- required headings or metadata
- next-action coverage where the repo uses that policy
- forbidden copied defaults such as `--repo .` in active agent/setup/workflow
  surfaces

Example:

```toml
[tasks]
"qa:docs:agent-defaults" = "effigy docs check-forbidden AGENTS.md README.md .github/workflows/ci.yml --forbid '--repo .'"
```

## 7) Starter File Set

When the repo is missing the contract, the starter set should be created in
this order:

1. `effigy.toml`
2. `AGENTS.md`
3. `docs/README.md`
4. `docs/vision/README.md`
5. first vision document
6. `docs/roadmaps/README.md`
7. first roadmap generation README and first active milestone
8. `docs/logs/README.md`
9. `CHANGELOG.md`
10. `[release]` config when the repo is actually being prepared for releases

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

## Next Step

Assess the first pilot repo against the checklist in Section 9, then normalize
the missing surfaces before building the reusable `northstar-effigy` skill
bundle.
