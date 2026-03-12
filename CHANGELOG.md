# Changelog

All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/).
During v0.x, MINOR bumps may include breaking changes.

## [Unreleased]

### Added
- Add built-in `effigy docs` validation commands for markdown link checks, JSON
  example section checks, and docs-index consistency checks, with the matching
  `scripts/check-doc-*.sh` entrypoints reduced to thin compatibility wrappers
- Add built-in `effigy contracts check-json` to run schema-index-driven JSON
  contract validation, selected-schema reporting, and changed-only checks
  without the previous shell-library implementation, with
  `scripts/check-json-contracts*.sh` reduced to thin wrappers over the built-in
- Add built-in `effigy contracts validate-selection` for JSON selection
  artifact validation, replacing the prior jq-heavy shell implementation behind
  `scripts/validate-json-contract-selection-artifact.sh`
- Add built-in `effigy distribution validate-metadata`,
  `validate-artifacts`, `generate-closeout`, and `write-summary` so
  distribution metadata checks, first-publish summary contracts,
  artifact-bundle validation, and acceptance-closeout log generation no longer
  depend on shell as the primary implementation language

### Changed
- Route `qa:json` and `qa:json:ci` directly through `effigy contracts
  check-json`, and reduce `scripts/check-selection-artifact-validator-smoke.sh`
  to a compatibility wrapper over targeted Rust coverage instead of owning its
  own validator fixture logic in shell
- Route `dist:metadata` and the release `metadata` gate through `effigy
  distribution validate-metadata`, and reduce
  `scripts/check-distribution-artifact-pipeline-smoke.sh` to a compatibility
  wrapper over targeted CLI coverage for the built-in distribution flow
- Route `qa:docs` and release-gate `qa` orchestration through native
  `effigy.toml` task composition, broaden `effigy docs check-links` default
  scope to the full `docs/` tree, and reduce `scripts/check-quality-gates.sh`
  plus `src/bin/effigy-qa.rs` to compatibility delegation over those task
  surfaces
- Reduce `scripts/check-distribution-first-publish.sh` so the
  `distribution-summary.env` contract is written by `effigy distribution
  write-summary`; the wrapper now retains only publish/install side effects,
  step-log capture, and final built-in artifact validation
- Add built-in `effigy distribution preflight` with summary-file output for
  the non-publish distribution gate path, and reduce
  `scripts/check-distribution-preflight.sh` to a compatibility wrapper over
  the native preflight surface
- Replace `scripts/check-prepush-ci.sh` with a thin wrapper over native
  `prepush:ci` task aliases, and update active operator docs to lead with
  `effigy` task/command entrypoints instead of the old shell-first QA wording
- Reduce `scripts/check-distribution-first-publish.sh` to the intentional
  external side-effect boundary by delegating tag verification, summary
  writing, and artifact validation to native Effigy commands instead of
  shell-wrapper entrypoints
- Retire redundant docs/contracts/distribution wrapper scripts and update
  active workflows, tasks, and operator guides to call native `effigy`
  commands or targeted Rust tests directly where no external script boundary
  is needed
- Remove the unused `scripts/check-json-contracts-ci.sh` wrapper and keep
  pull-request versus mainline JSON-contract policy in workflow YAML instead of
  duplicating it in shell
- Add built-in `effigy docs add-log-index` and retire the old
  `scripts/add-log-index-entry.sh` helper so docs/log index maintenance stays
  inside the native docs command surface
- Add built-in `effigy docs check-workflow-paths` and retire the old
  `docs/scripts/check-doc-workflow-paths.sh` helper so docs workflow-reference
  validation no longer depends on a standalone shell script
- Add optional `[docs_policy.indexes]` manifest support and
  `effigy docs check-index --policy-index <NAME>` so repo-specific markdown
  index rules can be supplied declaratively instead of hardcoded into generic
  built-ins
- Add optional `[docs_policy.next_actions]` manifest support and
  `effigy docs check-next-action --policy <NAME>` so repo-specific heading and
  actionable-verb rules can be enforced by a reusable built-in engine instead
  of a standalone shell checker

### Changed
- Move the active vision index validation path onto
  `effigy docs check-index --policy-index vision`, so
  `docs/scripts/check-vision-index.sh` is no longer needed as a standalone
  entrypoint in the repo's active docs QA flow
- Move the active vision next-action validation path onto
  `effigy docs check-next-action --policy vision`, so
  `docs/scripts/check-vision-next-task.sh` is no longer needed as a standalone
  entrypoint in the repo's active docs QA flow
- Move next-action negative-path coverage out of a shell regression harness and
  into Rust CLI tests, so docs QA validates the live repo state without mixing
  in fixture-only shell checks
- Replace the last docs-policy shell bundle with visible `qa:docs:vision` task
  composition plus generic `effigy docs check-headings` /
  `effigy docs check-contains` validators, so repo policy stays in the
  manifest/task graph instead of a dedicated bash entrypoint
- Remove the redundant `effigy-release-qa` helper binary and point
  `cargo qa-release` directly at `effigy release gates --repo .`, reducing one
  more compatibility layer without changing the operator-facing release gate
  path
- Stop advertising the broken `qa:release` task alias and lead with
  `effigy release gates --repo .` plus `cargo qa-release`, because
  manifest-task wrapping around release gates still self-nests under the
  workspace lock when release gates invoke nested Effigy commands
- Classify the remaining top-level release/bootstrap scripts into durable
  external boundaries versus timed compatibility backups, and document explicit
  retirement criteria for the three release wrapper scripts instead of keeping
  their lifespan open-ended
- Add a dedicated release-wrapper retirement record template guide so the next
  real release cycle can capture a concrete keep/retire decision in one
  reusable checkpoint artifact
- Add a dedicated release checkpoint log template guide so real release evidence,
  distribution evidence, and wrapper-retirement decisions can be captured in a
  single dated maintainer artifact
- Rewrite the top-level README, docs landing pages, and the first layer of
  workflow guides around newcomer, day-to-day, troubleshooting, automation,
  release, contribution, and support-doc user flows, add an
  everyday-workflows guide, refresh the IA snapshot, and move repo-specific
  self-hosting detail plus redundant `--repo .` examples out of the primary
  docs paths so the main Effigy feature set is easier to discover without
  teaching unnecessary flags before readers drop into the deeper reference
  material
- Add built-in `effigy docs check-forbidden`, wire `qa:docs` through a
  self-hosted agent-defaults guard, and update active adoption/setup/help
  surfaces plus workflow examples, and remove the same bad default from live
  release remediation hints and completion help examples so copied `--repo .`
  usage fails validation instead of spreading into downstream agent instructions

## [0.2.5] - 2026-03-11

### Added
- Add changelog library implementing the Northstar Changelog Profile — parse,
  format, validate, analyze, and extract changelogs with `effigy changelog`
  subcommands (`validate`, `format`, `analyze`, `extract`)
- Add `effigy release status` and non-destructive `effigy release prepare --plan`
  with `[release]` manifest config, version-file autodetection (`Cargo.toml`,
  `package.json`, `pyproject.toml`, `VERSION`), changelog readiness checks,
  version/changelog mutation previews, optional gate execution, and JSON
  payloads
- Add non-interactive `effigy release prepare --yes` to apply supported
  version/changelog updates and write `.release-prepared.json` state without
  committing, tagging, or pushing
- Add `effigy release execute --plan` as a preflight that loads
  `.release-prepared.json`, warns on stale prepared state, and verifies the git
  working tree matches the prepared file set before any commit/tag/push work
- Add non-interactive `effigy release execute --yes` to create the release
  commit and tag, push branch and tag to `origin`, print post-release checks,
  remove `.release-prepared.json` only after full success, and refuse to re-tag
  after a failed push
- Add standalone `effigy release gates` with sequential timed gate execution,
  fail-fast behavior, JSON output, and captured failed-gate output for release
  readiness checks outside the full prepare flow
- Add `effigy release simulate` as a full dry-run that runs release gates,
  previews version/changelog mutations plus commit/tag creation, reports
  fail-fast gate metadata, and guarantees no files or `.release-prepared.json`
  state are written
- Add `--dry-run` as a non-destructive alias for `effigy release prepare --plan`
  and `effigy release execute --plan`, so preview-first release flows can use
  either spelling while still producing the same plan payloads
- Preserve existing layout for release version-file updates in `Cargo.toml`,
  `pyproject.toml`, and `package.json` by mutating only the targeted version
  field instead of reformatting the whole file during `effigy release prepare`
  flows
- Add a self-hosted `[release]` section to this repo’s `effigy.toml` and route
  `qa:release` through `effigy release gates`, with contract tests that keep
  the configured baseline gate set aligned with `scripts/check-release-gates.sh`
- Add real `release.sync-files = ["Cargo.lock"]` support for Cargo-based
  release preparation, including prepare-plan/prepare-apply coverage and a
  Cargo-fixture parity test against `scripts/prepare-release.sh --apply`
- Add built-in `effigy release verify-install` for tag-based install
  validation, with the legacy `scripts/check-release-install-from-tag.sh`
  helper now delegating to the built-in command
- Turn `scripts/check-release-gates.sh` into a compatibility wrapper over
  `effigy release gates` plus optional `effigy release verify-install`, and add
  self-hosting contract checks that keep both legacy wrapper entrypoints aligned
  with the built-in release surfaces
- Add end-to-end migration parity tests showing the legacy release wrappers
  execute the same built-in `release gates` and `release verify-install` paths
  on Effigy-shaped fixtures, closing the remaining section-8 parallel-validation
  proof for shipped release surfaces
- Update the release checklist template and maintainer/operator docs to prefer
  the built-in `effigy release simulate/status/prepare/execute/verify-install`
  flow while keeping legacy release scripts documented as backup channels
- Tighten release protocol/checklist wording so built-in commands are clearly
  the primary operator path, legacy shell wrappers remain explicit backup
  channels until the first successful live built-in release, and workflow
  cutover tasks stay clearly human-gated
- Add end-to-end CLI coverage and maintainer guidance for
  `effigy changelog extract` as the preferred release-note baseline generator
  ahead of any approved workflow migration
- Add cross-project release orchestration coverage for `package.json`,
  `pyproject.toml`, and plain `VERSION` repos, plus agent-adoption examples for
  Node.js, Python, and multi-language release configs
- Add dedicated release orchestration guide `051`, update the command matrix to
  include `release` and `changelog` surfaces, and align `CLAUDE.md` with the
  built-in release workflow reference
- Add text-mode interactive confirmation flows for `effigy release prepare` and
  `effigy release execute`, while keeping `--plan` as preview-only and `--yes`
  as the explicit non-interactive path
- Expand text-mode `effigy release prepare` and `effigy release execute` into
  staged review flows with separate version/state, mutation/working-tree, gate,
  and final approval prompts before any release changes are applied
- Require explicit stale-state acknowledgement for `effigy release execute`:
  text-mode execute now inserts a stale-state approval step, while `--plan` and
  `--yes` require `--allow-stale` to proceed when `.release-prepared.json` is
  older than the default threshold
- Allow text-mode `effigy release prepare` to accept a deliberate custom semver
  override during version review, and carry suggested-versus-selected version
  metadata through prepare output and `.release-prepared.json`
- Add `--version <SEMVER>` override support to `effigy release prepare --plan`
  and `effigy release prepare --yes`, so non-interactive preview/apply flows
  can use the same deliberate version-selection contract as interactive prepare
- Tighten `release prepare --version` validation and surface
  suggested-versus-selected version metadata consistently in `release simulate`
- Add `effigy release simulate --version <SEMVER>` so full dry-run previews can
  exercise the same deliberate selected-version contract as non-interactive
  `release prepare` without writing files or state
- Upgrade `effigy release simulate` and `effigy release prepare --plan` with
  richer per-file mutation details and concise inline diff previews for
  supported write mutations
- Add interactive mutation drill-down to plain `effigy release prepare`, so
  Step 2 review can inspect one planned file mutation in detail before apply
- Add interactive drill-down to plain `effigy release execute`, so stale
  warnings and working-tree items can be inspected in detail before approval or
  before a blocked preflight returns failure
- Replace the fixed linear interactive release review flow with compact prepare
  and execute review menus, so operators can jump directly between review
  sections before apply/execute
- Keep interactive release review menus self-describing with a compact command
  legend plus persistent selected-version or stale-acknowledgement summaries,
  so operators can see the active state without re-reading prompt footers
- Mark reviewed sections directly inside interactive release menus and append
  suggested remediation actions to blocked prepare/execute output, so operators
  can track review progress and see the likely next fix path
- Add `effigy release resume` as a dedicated prepared-state recovery command
  that summarizes `.release-prepared.json`, highlights drift since prepare
  time, and can hand operators directly back into interactive execute review
- Add prepared-state source fingerprints to `.release-prepared.json`, so
  `effigy release resume` and `effigy release execute --plan/--yes` can detect
  branch drift, HEAD movement, and changed prepared-file contents since
  prepare time
- Add direct interactive recovery shortcuts to `effigy release resume` and
  `effigy release execute`: operators can now run `gates`, `reprepare`, or
  `discard` from the review flow, and blocked execute preflight exposes the
  same shortcuts before failing
- Add `@env-spec` integration: declarative `.env.schema` files with annotation
  DSL (`@type`, `@required`, `@sensitive`), value expressions (`exec()`,
  `env()`, `${VAR}` templates), type validation, topological dependency
  resolution, and dual environment injection (plain values via shell wrapping,
  secrets via `Command::env()` to avoid `ps` exposure)
- Add `[env_schema]` configuration section in `effigy.toml` with `enabled`,
  `schema` path override, and `exec_timeout` options
- Add `--env-schema <PATH>` task-runtime override so one-off task invocations
  can select a non-default `.env.schema` file without editing `effigy.toml`
- Allow run-array env directives, task-ref expansions, and configured built-in
  test suite env resolution to consume resolved `.env.schema` values for
  internal Effigy planning/runtime behavior
- Add env-schema string constraint annotations (`@min`, `@max`) and regex
  validation via `@pattern`, with task execution now failing before launch when
  resolved values violate those schema rules
- Redact sensitive env-schema validation values from task/runtime error output
  and back `SecretString` with `zeroize::Zeroizing<String>` for stronger
  in-memory secret handling
- Round out the public env-schema library surface with autodetection helpers,
  explicit `resolve_env` / `validate_env` entry points, and `ResolvedEnv`
  export helpers that return `HashMap<String, EnvValue>`
- Validate `[env_schema]` manifest configuration more strictly and add runtime
  coverage for `enabled`, `schema`, and `exec_timeout` behavior
- Extend env-schema secret redaction coverage across JSON-mode runner failures
  and resolved-env debug output so sensitive values stay masked across normal
  Effigy reporting surfaces
- Add roadmaps for Varlock @env-spec integration (025), changelog library and
  Northstar Profile (026), and release orchestration system (027)

### Changed
- Cut over `.github/workflows/release-binaries.yml` to use built-in
  `effigy changelog extract` for GitHub Release notes with the existing
  generated-notes fallback preserved, and refresh the touched GitHub-managed
  action versions used by release/CI/JSON-contract workflows ahead of the
  Node 24 runtime transition

### Fixed
- Align `scripts/check-distribution-metadata.sh` with the actual
  `release-binaries.yml` workflow and current distribution helper scripts, so
  release metadata validation no longer fails on obsolete workflow file names
  during built-in release rehearsals

## [0.2.4] - 2026-03-10

### Added
- Publish `inflatable-cookie/setup-effigy@v1` GitHub Action for CI binary
  installation with caching
- Add ARM Linux (`aarch64-unknown-linux-gnu`) binary to release pipeline and
  Homebrew formula (for AWS Graviton, Docker on Apple Silicon)

## [0.2.3] - 2026-03-10

### Added
- Homebrew formula now supports Linux (Linuxbrew) via `on_linux` block for
  x86_64 binaries

## [0.2.2] - 2026-03-10

### Fixed
- Fix release pipeline failure caused by using `secrets` context in job-level
  `if` condition — `secrets` is only available at step level in GitHub Actions

## [0.2.1] - 2026-03-10

### Added
- Homebrew tap auto-update in release pipeline — formula in
  `inflatable-cookie/homebrew-tap` is updated automatically on each tagged
  release
- JSON contracts CI workflow (`.github/workflows/json-contracts.yml`) now
  active on PRs, pushes to main, and daily schedule
- CHANGELOG-based release notes — GitHub Releases now use entries from
  CHANGELOG.md instead of auto-generated notes
- Install section in README with three channels: Homebrew, prebuilt binary,
  and cargo install from source

### Changed
- Replace ripgrep (`rg`) with POSIX `grep` in all QA and docs-check scripts
  so CI runners work without ripgrep installed
- Remove `.github-bak/` staging directory — all workflows are now active or
  superseded
- Update doc 042 (Homebrew Tap) to reflect prebuilt binary formula approach
- Update doc 049 (Release Protocol) to reflect current active workflow state

### Fixed
- Fix JSON contracts CI failure caused by `rg` not being available on
  ubuntu-latest runners
- Fix stale `.github-bak/` workflow references across documentation

## [0.2.0] - 2026-03-09

### Breaking
- Change process spawn from login shell (`sh -lc`) to non-login shell (`sh -c`)
  across all execution paths. Fixes PATH clobbering on Linux where `/etc/profile`
  unconditionally resets PATH in login shells. Parent process environment is now
  inherited correctly on all platforms.

### Added
- CI workflow (`.github/workflows/ci.yml`) with format, clippy, and test jobs
  on Linux and macOS
- Release binaries workflow (`.github/workflows/release-binaries.yml`) for
  cross-platform binary distribution via GitHub Releases
- Changelog and automated release preparation (`scripts/prepare-release.sh`)

### Changed
- Rename test fixture catalogs from project-specific names (`farmyard`, `dairy`,
  `cream`) to generic names (`catalog_a`, `catalog_b`, `catalog_c`) across all
  test suites, scripts, and documentation
- Use `frontend` in user-facing help text examples instead of project-specific names

### Fixed
- Resolve 5 pre-existing clippy warnings (needless return, vec init-then-push,
  field reassign with default, manual contains)
