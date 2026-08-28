# 051 - Release Workflow

Use this guide when you want Effigy to own release readiness, preparation,
execution preflight, install verification, and release-note extraction from a
repo-local `[release]` contract.

This is the canonical guide for the shipped `effigy release` surface.

Use:
- this guide for the release workflow and `[release]` config
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
  for maintainer policy and binary channel rules
- [`062-distribution-system-guide.md`](./062-distribution-system-guide.md) for
  the wider distribution commands
- [`052-changelog-workflows-and-northstar-profile.md`](./052-changelog-workflows-and-northstar-profile.md)
  for changelog-specific work

## Start Here

Use this guide when a repo is ready to put release work behind one built-in
surface instead of a growing set of shell steps.

If you only need the shortest operator path, use:

- `simulate`
- `status --check-gates`
- `prepare --plan`
- `prepare --yes --check-gates`
- `execute --plan`
- `execute --yes`

If you are approaching the release flow for the first time, start with:

```sh
effigy release simulate
effigy release status --check-gates
effigy release prepare --plan
effigy release gates
```

Use the commands by intent:

- `simulate` for a no-write preview of the likely release path
- `status --check-gates` for current readiness plus configured gate results
- `prepare --plan` for the exact file mutations Effigy would make
- `prepare` / `execute` text mode when a human is actively reviewing the flow
- `prepare --yes` / `execute --yes` when non-interactive operation is required
- `resume` when a prepared release needs to be inspected or recovered
- `release gates` when you want the same local release-gate verdict CI should
  reach before you mutate anything

## 1a) Breaking Command Moves In Release Notes

When a breaking release re-homes existing commands, keep the migration note
short and explicit. Do not make operators infer the new home from a broad CLI
cleanup summary.

Good shape:

- say which old command is gone
- show the exact replacement command
- group related moves into one short section instead of scattering them across
  multiple bullets

For the current helper-command cleanup, the release-note section should read
roughly like this:

- `effigy migrate` -> `effigy tasks migrate`
- `effigy unlock` -> `effigy tasks unlock`
- `effigy cache ...` -> `effigy tasks cache ...`
- `effigy completion ...` -> `effigy config completion ...`
- `effigy catalogs` removed; use `effigy tasks`

## 1) Core Commands

Current built-in release commands:

- `effigy release status`
  - inspect version-file state, changelog validity, suggested bump, and optional
    gate results
- `effigy release gates`
  - run configured release gates as a standalone fail-fast timed check
- `effigy release resume`
  - recover an existing `.release-prepared.json` state and inspect drift
- `effigy release simulate`
  - full dry-run preview of gates, version and tag selection, and file
    mutations with no written state; accepts `--version <SEMVER>`
- `effigy release prepare --plan`
  - preview the exact version-file and changelog mutations; accepts
    `--version <SEMVER>`; `--dry-run` is an alias
- `effigy release prepare`
  - text-mode review before Effigy writes release changes
- `effigy release prepare --yes`
  - apply supported mutations and write `.release-prepared.json` without
    committing, tagging, or pushing; accepts `--version <SEMVER>` for
    deliberate non-interactive overrides
- `effigy release execute --plan`
  - validate prepared state and working tree before any irreversible step;
    detects stale state and source drift; `--dry-run` is an alias
- `effigy release execute`
  - text-mode final review before commit, tag, and push
- `effigy release execute --yes`
  - create the release commit and annotated tag, push to `origin`, and clean up
    the state file only after a full success; the annotation message exactly
    equals the rendered tag; age-based staleness requires explicit
    `--allow-stale`
- `effigy release verify-install`
  - Effigy self-hosting only: install the tagged `effigy` binary from git into
    a temporary root and validate the installed command against a fixture repo
  - library and service repos must use a repo-owned consumer smoke instead
  - when Effigy auto-detects `origin`, scp-style SSH remotes such as
    `git@github.com:owner/repo.git` are normalized for install verification, so
    operators do not need to translate them into `ssh://...` form by hand

Related built-ins:

- `effigy changelog extract CHANGELOG.md --version X.Y.Z`
  - extracts the per-version changelog body used as release-note source
    material

## 2) Minimal `[release]` Config

Smallest useful release config:

```toml
[release]
changelog = "CHANGELOG.md"
tag-format = "v{version}"
```

With that in place, Effigy will auto-detect the version file from the repo
root in this order:

1. `Cargo.toml`
2. `package.json`
3. `pyproject.toml`
4. `VERSION`

Use explicit `version-file` when the repo should not rely on autodetection.

## 3) Config Reference

Supported `[release]` fields:

- `version-file`
  - optional explicit path to the version source
  - supported formats: `Cargo.toml`, `package.json`, `pyproject.toml`, `VERSION`
- `version-path`
  - optional field path for structured files
  - examples:
    - `package.version` for TOML
    - `version` for JSON
  - not supported for plain `VERSION`
- `changelog`
  - optional path to the changelog file
  - defaults to `CHANGELOG.md`
- `pre-1-0`
  - optional boolean
  - when `true`, breaking unreleased changes in `0.x` releases produce a minor
    bump policy instead of forcing a major bump
- `initial-tag-current-version`
  - optional boolean; defaults to `false`
  - permits the first changelog release to tag the version already declared in
    the version file
  - applies only while the changelog has no released versions and the matching
    local tag does not exist
  - does not permit a lower version or a repeated release
- `sync-files`
  - optional list of extra files Effigy should keep in sync during prepare
  - supported:
    - `Cargo.lock`
      - refreshes workspace member versions without accepting unrelated
        dependency changes
    - `package.json`
      - updates the root `version` field to the selected release version while
        preserving file layout
      - may be used when another file, such as `Cargo.toml`, is the primary
        version source
- `tag-format`
  - optional tag template
  - supports `{version}` placeholder
- `[release.gates]`
  - optional named gate map
  - supports string shorthand or table form with `command` and `description`

During `effigy release prepare`, supported structured version files keep their
existing layout:
- `Cargo.toml` and `pyproject.toml` preserve comments and table ordering
- `package.json` preserves existing spacing and object layout around the
  targeted version field

Gate forms:

```toml
[release.gates]
fmt = "cargo fmt --all -- --check"

[release.gates.test]
command = "cargo test"
description = "Run the Rust test suite"
```

Gates run in declaration order. Put cheap checks first.

For a new project that already declares its intended first release version,
opt in explicitly:

```toml
[release]
version-file = "Cargo.toml"
changelog = "CHANGELOG.md"
tag-format = "v{version}"
initial-tag-current-version = true
```

In this mode, `release status`, `release simulate`, and `release prepare`
select the current version. Prepare promotes `[Unreleased]` without rewriting
the version file. The exception closes as soon as the changelog contains a
released version or the matching local tag exists.

## 4) Version File Formats

### Cargo (`Cargo.toml`)

```toml
[release]
version-file = "Cargo.toml"
changelog = "CHANGELOG.md"
sync-files = ["Cargo.lock"]
tag-format = "v{version}"
```

Default version path:
- `package.version`

Mixed Rust/Node packages can keep both manifests aligned:

```toml
[release]
version-file = "Cargo.toml"
sync-files = ["Cargo.lock", "package.json"]
```

### Node.js (`package.json`)

```toml
[release]
version-file = "package.json"
changelog = "CHANGELOG.md"
tag-format = "v{version}"

[release.gates]
test = "npm test"
```

Default version path:
- `version`

### Python (`pyproject.toml`)

```toml
[release]
version-file = "pyproject.toml"
changelog = "CHANGELOG.md"
tag-format = "v{version}"

[release.gates]
test = "pytest -q"
```

Autodetected version paths:
- `project.version`
- `tool.poetry.version`

### Plain `VERSION`

```toml
[release]
version-file = "VERSION"
changelog = "CHANGELOG.md"
tag-format = "release-{version}"

[release.gates]
validate = "sh -lc './scripts/validate-all.sh'"
```

Use this for multi-language repos where the release version should not be owned
by one language manifest.

## 5) Workflow Walkthrough

Recommended operator flow:

```sh
candidate_sha="$(git rev-parse HEAD)"
gh workflow run ci.yml --ref main
# Select the workflow_dispatch run whose headSha equals $candidate_sha.
gh run watch <RUN_ID> --exit-status
effigy release simulate
effigy release simulate --version 0.2.8
effigy release status --check-gates
effigy release prepare
effigy release prepare --dry-run
effigy release prepare --plan
effigy release prepare --yes --check-gates
effigy release resume
effigy release execute
effigy release execute --dry-run
effigy release execute --plan
effigy release execute --yes
# Effigy binary releases only:
effigy release verify-install --tag vX.Y.Z
```

The candidate must be a clean commit already pushed to `main`. Select its CI
run with:

```sh
gh run list --workflow ci.yml --branch main --commit "$candidate_sha" \
  --event workflow_dispatch --limit 1 \
  --json databaseId,headSha,status,conclusion,url
```

Do not accept the latest green run unless its `headSha` is the candidate SHA.
Effigy's self-hosted `ci` release gate checks the same invariant. Missing,
pending, red, cancelled, or different-commit evidence blocks release gates,
simulation, and gate-checked prepare. The other local gates validate the
deterministic version, changelog, and lockfile mutations after hosted CI has
proved the source commit.

For Effigy's own binary publication, explicitly dispatch the immutable tag
after execute succeeds and before install verification:

```sh
gh workflow run release-binaries.yml -f tag=vX.Y.Z
```

The workflow definition runs from `main`, but every release job checks out and
validates the supplied annotated tag. Pushing a tag alone does not publish.

For Effigy's own release prep, the useful pre-cut habit is:

```sh
effigy release gates
```

Treat a red gate as a fix-the-repo signal, not something to work around.

`verify-install` can auto-detect the repo URL from `origin`, or you can pass
`--repo-url <URL>` explicitly. Both paths accept normal HTTPS/file URLs, and
scp-style SSH remotes are normalized automatically for the install step.

What each step is for:

1. `simulate`
   - safe preview, no file writes, no state file
   - optional `--version <SEMVER>` keeps the same no-write contract while
     previewing a deliberate valid override
2. `status --check-gates`
   - release readiness from current repo state plus gates
3. `prepare`
   - interactive review before writing
4. `prepare --plan`
   - exact proposed version-file and changelog mutation preview
5. `prepare --yes --check-gates`
   - writes supported release changes and `.release-prepared.json`
6. `resume`
   - re-enter a prepared release and inspect drift since prepare time
7. `execute`
   - interactive final confirmation before commit, tag, and push
8. `execute --plan`
   - confirms the prepared state still matches the working tree
9. `execute --yes`
   - commits, creates an annotated tag whose message equals the rendered tag,
     pushes, and removes prepared state on success
10. `verify-install` (Effigy binary releases only)
   - validates Effigy's tagged install path after the manually dispatched
     release workflow publishes its artifacts
   - library and service repos use their own consumer smoke instead

## 6) Gate Configuration

Gate commands are normal shell commands. They do not need to be Rust-specific.

Examples:

```toml
[release.gates]
rust = "cargo test"
node = "npm test"
python = "pytest -q"
smoke = "sh -lc './scripts/smoke.sh'"
```

Gate behavior:

- runs sequentially
- records timing per gate
- stops on first failure
- surfaces captured output for failed gates
- can be invoked directly with `effigy release gates`

Effigy's own manifest includes `ci = "sh scripts/check-release-ci.sh"`. This
is a repository policy gate, not a provider assumption in the generic release
engine. GitHub-hosted consumers can adopt the same exact-SHA pattern; other
providers should supply an equivalent gate for their CI system.

Important prepare rule:

- if `[release.gates]` is configured, `effigy release prepare --yes` requires
  `--check-gates`

## 7) State File and Safety

Prepare writes:

- `.release-prepared.json`

That state file records:

- previous version
- suggested version
- prepared version
- whether a custom version override was used
- tag
- prepared timestamp
- whether gates were checked and passed
- modified file list
- source fingerprints for the prepared branch, prepared HEAD, and each prepared
  file's content digest

Execute safety checks:

- missing state file blocks execute
- stale state emits a warning using the default one-hour threshold and requires
  explicit acknowledgement or `--allow-stale` before execute can proceed;
  this override covers age only
- unexpected working-tree changes block execute
- branch drift, HEAD movement, or changed prepared-file contents since prepare
  are reported as source-drift blockers in `resume` and `execute --plan`;
  `--allow-stale` cannot override them because the recorded gates apply to the
  prepared source identity, so regenerate state with `release prepare`
- JSON `release execute --plan` includes `suggested_actions`; source drift
  reports the same mandatory reprepare recovery used by `release resume`
- push failure leaves the prepared state file in place
- retries do not re-create an already-created local tag
- release tags are annotated Git objects; signing and configurable annotation
  templates are not implied

## 8) Release Notes Baseline

Use the changelog extractor before drafting human-reviewed release notes:

```sh
effigy changelog extract CHANGELOG.md --version X.Y.Z
```

This prints the release body without the outer version heading, which makes it a
good baseline for:

- `docs/logs/YYYY-MM/...` release notes
- release PR summaries
- later workflow automation once `.github/workflows/` changes are explicitly
  approved

## 9) Migration from Custom Scripts

Recommended migration direction:

- move release gate definitions into `[release.gates]`
- keep wrapper scripts only when an external caller still depends on them
- prefer `effigy release simulate/status/prepare/execute` for operator-driven
  runs
- for Effigy's binary release, use `effigy release verify-install` instead of
  bespoke tag-install helpers
- for library and service repos, retain an honest repo-owned consumer smoke

## 10) Current Limits

Still intentionally not shipped:

- custom interactive editing flows beyond a single semver override, or full
  inline/unified diffs before apply
- automatic workflow migration in `.github/workflows/` without explicit human
  approval
- a claim that the fuller `release proof` path is already universally generic
  across non-Cargo consumer repos
- generic library or service install verification through `release
  verify-install`; that command deliberately validates the Effigy binary

## Expected Outcome

After applying this guide, a repo can describe release operation in one
`[release]` section and run readiness, preparation, execution, and
verification through built-in commands instead of wrapper scripts.

## Related Guides

- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)

## Next Step

After adding `[release]` to a repo, run `effigy release simulate` and
`effigy release status --check-gates` before replacing any existing release
wrapper or CI entrypoint.
