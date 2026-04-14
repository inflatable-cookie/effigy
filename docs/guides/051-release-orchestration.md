# 051 - Release Orchestration

Use this guide when you want Effigy to own release readiness, preparation,
execution preflight, install verification, and release-note extraction from a
repo-local `[release]` contract.

This is the canonical reference for the shipped `effigy release` surface. Use
[`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
for policy and human approval rules; use this guide for the actual command and
config contract.

## Vision Alignment

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Target movement: release operation becomes a documented Effigy surface instead
  of drifting into repo-local wrapper scripts and one-off release notes logic.

## Start Here

Use this guide when a repo is ready to put release work behind one built-in
surface instead of a growing set of shell steps.

If you are approaching the release flow for the first time, start with:

```sh
effigy release simulate
effigy release status --check-gates
effigy release prepare --plan
```

Use the commands by intent:

- `simulate` for a no-write preview of the likely release path
- `status --check-gates` for current readiness plus configured gate results
- `prepare --plan` for the exact file mutations Effigy would make
- `prepare` / `execute` text mode when a human is actively reviewing the flow
- `prepare --yes` / `execute --yes` when non-interactive operation is required
- `resume` when a prepared release needs to be inspected or recovered

## 1) Command Surface

Current built-in release commands:

- `effigy release status`
  - inspect version-file state, changelog validity, suggested bump, and optional
    gate results
- `effigy release gates`
  - run configured release gates as a standalone fail-fast timed check
- `effigy release resume`
  - prepared-state recovery entrypoint for an existing `.release-prepared.json`
  - summarizes prepared version/tag, stale state, working-tree drift, and
    source-fingerprint drift since prepare time
  - text mode can hand operators directly into the interactive execute review
    flow after they inspect prepared state or drift details
  - recovery menu includes direct `gates`, `reprepare`, and `discard`
    shortcuts for common recovery paths
- `effigy release simulate`
  - full dry-run preview of gates, suggested-versus-selected version/tag
    metadata, and file mutations with no written state; accepts
    `--version <SEMVER>` for deliberate non-destructive overrides
- `effigy release prepare --plan`
  - preview the exact version-file and changelog mutations Effigy would apply;
    accepts `--version <SEMVER>` for deliberate non-interactive overrides
  - both preview surfaces now show per-file mutation details plus concise
    inline diff snippets for the supported write mutations
  - `--dry-run` is a supported alias for this preview mode
- `effigy release prepare`
  - text-mode menu-driven prepare review; operators can jump between version
    review, mutation review, gate results, and final approval before Effigy
    writes release changes
  - during mutation review, operators can inspect a single planned file
    mutation in detail before continuing
  - the menu keeps the selected version, planned tag, and a compact command
    legend visible while the operator reviews
  - reviewed menu sections are marked in-place so maintainers can see what they
    have already inspected before applying
- `effigy release prepare --yes`
  - apply supported mutations and write `.release-prepared.json` without
    committing, tagging, or pushing; accepts `--version <SEMVER>` for
    deliberate non-interactive overrides
- `effigy release execute --plan`
  - validate `.release-prepared.json` and the current working tree before any
    irreversible step; stale state requires explicit `--allow-stale`
  - also detects branch drift, HEAD movement, and prepared-file content drift
    from the fingerprints recorded at prepare time
  - `--dry-run` is a supported alias for this preview mode
- `effigy release execute`
  - text-mode menu-driven execute review, including stale-state
    acknowledgement when needed, before commit/tag/push
  - operators can inspect one stale warning or working-tree item in detail
    during interactive review, and blocked execute preflights expose the same
    drill-down before returning failure
  - the menu keeps the stale acknowledgement state, prepared version/tag, and
    a compact command legend visible while the operator reviews
  - reviewed menu sections are marked in-place so maintainers can see what they
    have already inspected before executing
  - recovery menu and blocked-preflight browser include direct `gates`,
    `reprepare`, and `discard` shortcuts
- `effigy release execute --yes`
  - create the release commit and tag, push to `origin`, and clean up the state
  file only after a full success; stale state requires explicit `--allow-stale`
- `effigy release verify-install`
  - install the tagged binary from git into a temporary root and validate the
    installed command against a fixture repo
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
- `sync-files`
  - optional list of extra files Effigy should keep in sync during prepare
  - currently supported:
    - `Cargo.lock`
      - synced with `cargo generate-lockfile --quiet`
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
effigy release verify-install --tag vX.Y.Z
```

`verify-install` can auto-detect the repo URL from `origin`, or you can pass
`--repo-url <URL>` explicitly. Both paths accept normal HTTPS/file URLs, and
scp-style SSH remotes are normalized automatically for the install step.

What each step proves:

1. `simulate`
   - safe preview, no file writes, no state file
   - optional `--version <SEMVER>` keeps the same no-write contract while
     previewing a deliberate valid override
2. `status --check-gates`
   - release readiness from current repo state plus gates
3. `prepare`
   - shows the prepare preview and asks for confirmation before writing
   - keeps the current selected version and command legend visible while you
     review
4. `prepare --plan`
   - exact proposed version-file and changelog mutation preview
   - blocked output now includes suggested remediation actions alongside the
     blockers
   - `--dry-run` is an equivalent alias when teams prefer that spelling
5. `prepare --yes --check-gates`
   - writes supported release changes and `.release-prepared.json`
6. `resume`
   - summarizes the prepared state and drift since prepare time
   - useful when a release pauses between prepare and execute or when you need
     to recover context before re-entering execute review
7. `execute`
   - shows the execute preflight and asks for final confirmation before
     commit/tag/push
   - keeps the current stale acknowledgement state and command legend visible
     while you review
8. `execute --plan`
   - confirms the prepared state still matches the working tree
   - blocked output now includes suggested remediation actions alongside the
     blockers
   - `--dry-run` is an equivalent alias when teams prefer that spelling
9. `execute --yes`
   - commits, tags, pushes, and removes prepared state on success
10. `verify-install`
   - validates the tagged install path after a release tag exists

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
  explicit acknowledgement or `--allow-stale` before execute can proceed
- unexpected working-tree changes block execute
- branch drift, HEAD movement, or changed prepared-file contents since prepare
  are reported as source-drift blockers in `resume` and `execute --plan`
- push failure leaves the prepared state file in place
- retries do not re-create an already-created local tag

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
- use `effigy release verify-install` instead of bespoke tag-install helpers

For Effigy itself:

- `smoke:release` is the repo's native binary-artifact smoke task
- `scripts/check-linux-glibc-floor.sh` remains an intentional shell boundary
  because it depends on Linux binary-inspection tooling

## 10) Current Limits

Still intentionally not shipped:

- custom interactive editing flows such as operator-selected version overrides
  beyond a single semver override, or full inline/unified diffs before apply
- automatic workflow migration in `.github/workflows/` without explicit human
  approval

## Expected Outcome

After applying this guide, a repo can describe release operation in one
declarative `[release]` section, run release readiness and preparation through
Effigy, and keep any remaining wrapper scripts as explicit compatibility
surfaces rather than as the primary release logic.

## Related Guides

- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)

## Next Step

After adding `[release]` to a repo, run `effigy release simulate` and
`effigy release status --check-gates` before replacing any existing
release wrapper or CI entrypoint.
