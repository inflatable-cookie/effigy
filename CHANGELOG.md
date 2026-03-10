# Changelog

All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/).
During v0.x, MINOR bumps may include breaking changes.

## [Unreleased]

### Added
- Add changelog library implementing the Northstar Changelog Profile — parse,
  format, validate, analyze, and extract changelogs with `effigy changelog`
  subcommands (`validate`, `format`, `analyze`, `extract`)
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
