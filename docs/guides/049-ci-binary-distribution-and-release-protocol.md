# 049 - CI Binary Distribution and Release Protocol

Use this guide as the authoritative reference for how Effigy binaries are built,
published, and consumed by CI runners in other projects. All agent threads
working on release, distribution, or cross-repo CI integration must follow the
protocols defined here.

## Vision Alignment

- Primary tags: `RELEASE`, `MAINT`, `OPERATE`
- Target movement: CI runners in consumer repos can install a pinned Effigy
  binary in seconds without a Rust toolchain.

## 1) Current State

- Release posture: deliberate `v0.3` prep is active; use exact versions in
  consumer CI instead of relying on this guide to imply a moving "current"
  number
- Active workflows in `.github/workflows/`:
  - `ci.yml` — PR and push validation (format, clippy, tests)
  - `release-binaries.yml` — tag-triggered: gates → build → GitHub Release → Homebrew tap
  - `json-contracts.yml` — JSON contract and docs link validation
- Distribution channels:
  - **Homebrew** — `brew install inflatable-cookie/tap/effigy` (macOS, prebuilt binaries)
  - **GitHub Releases** — prebuilt binaries for macOS (arm64, x86_64) and Linux (x86_64)
  - **`cargo install` from tag** — fallback when prebuilt binaries are unavailable

### 1a) Rehearsal Status

As of 2026-03-11, the built-in `effigy release ...` flow has now passed:

- a zero-risk local rehearsal against the real Effigy repo contents
- a hosted throwaway-repo rehearsal that created and pushed real release tags
- the tag-triggered `Release Binaries` workflow on GitHub
- hosted PR validation for `CI` and `JSON Contracts` after release-command
  parity fixes

Decision boundary:

- the built-in release path is now proven in production on the real Effigy repo
- wrapper retirement and any further simplification remain deliberate
  follow-up decisions, not automatic consequences of the first live release

Canonical rehearsal summary:
- [`../logs/2026-03/11-180500-release-cutover-readiness-rehearsal-brief.md`](../logs/2026-03/11-180500-release-cutover-readiness-rehearsal-brief.md)

Workflow-cutover update:

- `.github/workflows/release-binaries.yml` now uses built-in
  `effigy changelog extract` for GitHub Release notes instead of the legacy
  inline `sed` extraction path
- that cutover was validated on the hosted rehearsal repo via a real tag push
  and published GitHub Release:
  [`../logs/2026-03/11-183500-release-workflow-cutover-hosted-validation.md`](../logs/2026-03/11-183500-release-workflow-cutover-hosted-validation.md)
- the workflow cutover is live in production
- wrapper retirement remains a deliberate follow-up choice rather than an
  immediate post-release requirement

## 2) Target Channel Stack for CI

Priority order for CI consumption:

1. **GitHub Releases** (prebuilt binaries) — preferred for CI runners
2. **Homebrew** — preferred for developer machines on macOS
3. **`cargo install` from tag** — fallback when prebuilt binaries are unavailable

The prebuilt binary channel is the primary path for CI. It requires no Rust
toolchain, no Homebrew, and installs in seconds via `curl`.

## 3) Platform Matrix

Prebuilt binaries must cover these targets at minimum:

| Target triple                   | OS    | Arch   | Priority |
|---------------------------------|-------|--------|----------|
| `x86_64-unknown-linux-gnu`      | Linux | x86_64 | Required |
| `aarch64-apple-darwin`          | macOS | arm64  | Required |
| `x86_64-apple-darwin`           | macOS | x86_64 | Required |
| `aarch64-unknown-linux-gnu`     | Linux | arm64  | Required |

All four targets are built and smoke-tested in CI (`release-binaries.yml`).
The ARM Linux target uses cross-compilation via `taiki-e/setup-cross-toolchain-action`
with QEMU for the smoke test.

Linux GNU compatibility policy:
- Linux release binaries are built on `ubuntu-22.04` to keep the glibc floor
  stable at `GLIBC_2.35`
- `scripts/check-linux-glibc-floor.sh` runs in the release build job and fails
  the workflow if a Linux artifact starts requiring a newer glibc symbol
  version
- Effigy now also has one local rehearsal path for this on developer machines:
  `cargo run --bin effigy -- release:linux:rehearse`

Binary naming convention:

```
effigy-<target-triple>
```

Example: `effigy-x86_64-unknown-linux-gnu`, `effigy-aarch64-apple-darwin`

## 4) Release Workflow Pipeline

### 4a) Trigger

The release pipeline activates on git tag push matching `v*`. No release may be
triggered by branch push, manual workflow dispatch alone, or any automated
process that skips the tag.

### 4b) Pipeline Stages

```
tag push (v*)
  │
  ├─ 1. Release gates (existing: format, test, QA, smoke)
  │
  ├─ 2. Cross-compile (parallel matrix build)
  │     ├─ x86_64-unknown-linux-gnu
  │     ├─ aarch64-apple-darwin
  │     ├─ x86_64-apple-darwin
  │     └─ aarch64-unknown-linux-gnu (cross-compiled)
  │
  ├─ 3. Check Linux GLIBC floor before publish
  │
  ├─ 4. Smoke test each binary on native runner
  │
  ├─ 5. Create GitHub Release with binaries attached
  │
  ├─ 6. Homebrew tap metadata + formula PR (existing)
  │
  └─ 7. crates.io publish (existing, when ready)
```

### 4c) Release Gate Prerequisite

The cross-compile and publish stages must not run unless release gates pass.
This is enforced by workflow job dependency, not by convention.

## 5) Consumer CI Install Pattern

### 5a) Recommended: `setup-effigy` Action

The preferred way to install Effigy in GitHub Actions workflows:

```yaml
- uses: inflatable-cookie/setup-effigy@v1
  with:
    version: 'X.Y.Z'
```

This handles platform detection, downloading, and caching automatically.
See [`inflatable-cookie/setup-effigy`](https://github.com/inflatable-cookie/setup-effigy)
for full documentation.

### 5a-alt) Manual curl Snippet

For non-GitHub-Actions CI systems, or if the action is not suitable:

```yaml
- name: Install effigy
  run: |
    EFFIGY_VERSION="${EFFIGY_VERSION:-X.Y.Z}"
    TARGET="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
    case "$TARGET" in
      linux-x86_64)   TRIPLE="x86_64-unknown-linux-gnu" ;;
      linux-aarch64)  TRIPLE="aarch64-unknown-linux-gnu" ;;
      darwin-arm64)   TRIPLE="aarch64-apple-darwin" ;;
      darwin-x86_64)  TRIPLE="x86_64-apple-darwin" ;;
      *) echo "Unsupported platform: $TARGET" && exit 1 ;;
    esac
    curl -fsSL \
      "https://github.com/inflatable-cookie/effigy/releases/download/v${EFFIGY_VERSION}/effigy-${TRIPLE}" \
      -o /usr/local/bin/effigy
    chmod +x /usr/local/bin/effigy
    effigy --help > /dev/null
```

### 5b) Version Pinning Policy

During `v0.x`:
- Consumer CI must pin an exact version (`EFFIGY_VERSION=0.1.0`)
- Floating or latest-tag references are not supported
- Version bumps in consumer repos should be explicit commits, not automated

### 5c) Caching

Consumer CI may cache the downloaded binary keyed on version:

```yaml
- uses: actions/cache@v4
  with:
    path: /usr/local/bin/effigy
    key: effigy-${{ env.EFFIGY_VERSION }}-${{ runner.os }}-${{ runner.arch }}
```

## 6) Versioning Protocol

### 6a) Current Policy

Per the release contract (`release-contract-v0.md`):
- Format: `0.MINOR.PATCH`
- `PATCH`: bug fixes, no breaking changes
- `MINOR`: may break, must include migration notes

Released-surface rule for deliberate `0.x` minor cuts:
- keep the previous shipped tag as the compatibility floor until the next
  release ships
- for the planned `v0.3.0` cut, treat `v0.2.13` as that floor
- record any intentional break from that floor in
  `tests/fixtures/released_surface/v0.3.0/transition.json`
- every intentional break entry must carry a migration note instead of relying
  on implied changelog context
- the current recorded intentional break for that cut is retirement of the
  legacy release wrapper scripts; keep that fixture aligned with the real
  migration story when more deliberate breaks are approved

Supported-boundary rule for `v0.3` messaging:

- describe Effigy's release/distribution story as strong native self-hosting
  plus reusable validation/evidence primitives
- do not describe the fuller `distribution first-publish` path as universally
  generic while it still carries bounded Cargo-centric assumptions

### 6b) Tagging Rules

- Tags must match `vMAJOR.MINOR.PATCH` exactly (e.g., `v0.1.0`)
- The tag version must match `Cargo.toml` version
- Tags must only be created on the `main` branch
- Tags must only be created after all release gates pass

### 6c) Release Execution Protocol

When a human asks an agent to create a release, the agent must follow these
steps in order. No step may be skipped. If any step fails, the agent must stop
and resolve the failure before continuing.

Current command surface:
- `effigy release status` is available as a non-destructive readiness check for
  `[release]` config, version-file detection, changelog validity, and optional
  gates.
- `effigy release gates` is available as a standalone timed gate runner with
  fail-fast behavior and JSON output, so release readiness can be checked
  independently from prepare/execute flows.
- `effigy release verify-install` is available as the built-in tag-install
  validation command. It installs the tagged binary from git into a temporary
  root and checks the installed binary against a fixture repo before succeeding.
- Remaining shell boundaries are different: keep
  `scripts/check-linux-glibc-floor.sh` and `scripts/effigy-dev` unless their
  external-binary or platform-tooling responsibilities change materially.
- The built-in release flow has now completed local rehearsal, hosted
  rehearsal, and a real production Effigy release through the built-in
  prepare/execute path, including real GitHub tag-triggered workflow execution.
  The operator rule still stands: releases remain explicit human decisions and
  should be monitored closely while the workflow matures.
- `effigy release simulate` is available as a full dry-run that runs release
  gates, previews version/changelog mutations, shows the commit/tag that would
  be created without writing files or `.release-prepared.json`, and accepts
  `--version <SEMVER>` when operators want a deliberate no-write preview of a
  non-default valid release version.
- `effigy release simulate` and `effigy release prepare --plan` now include
  richer per-file mutation previews, including concise inline diff snippets, so
  operator review is no longer limited to one-line before/after summaries.
- Effigy’s own repo now declares a baseline `[release]` section in
  `effigy.toml`, and the local no-tag gate path is
  `effigy release gates`.
- `cargo qa-release` now maps directly to `effigy release gates`
  instead of going through a separate helper binary that shells out to the
  compatibility wrapper.
- `effigy release prepare --plan` is available as a non-destructive preview of
  the proposed version/changelog mutations, but it does not write files.
- Plain `effigy release prepare` is now available in text mode as a
  menu-driven review flow. Operators can jump between version review, mutation
  review, gate results when applicable, and final approval before applying the
  prepare step. During version review, they can keep the current selection or
  enter a different valid semver deliberately. During mutation review, they can
  inspect one planned file mutation in detail before continuing. The menu keeps
  the selected version, planned tag, a compact command legend, and reviewed
  section markers visible while the operator reviews.
- `effigy release prepare --yes` is available as a non-interactive apply path
  that writes the supported version/changelog updates and `.release-prepared.json`
  state. For Cargo-based repos, configured `release.sync-files = ["Cargo.lock"]`
  entries are now applied during prepare. When `[release.gates]` is configured,
  it requires `--check-gates`.
- `effigy release prepare --plan --version X.Y.Z` and
  `effigy release prepare --yes --version X.Y.Z` are now available when an
  operator wants to keep the normal changelog-derived suggestion visible but
  deliberately preview or apply a different valid semver.
- `effigy release execute --plan` is available as a non-destructive preflight
  that loads `.release-prepared.json`, warns on stale state, and checks the git
  working tree for missing or unexpected changes before any irreversible
  release step. Prepared state now also records source fingerprints, so the
  plan can detect branch drift, HEAD movement, and prepared-file content drift
  since prepare time. When the prepared state is stale, the plan requires
  explicit `--allow-stale` before it reports execution readiness.
- `effigy release resume` is available as the dedicated recovery entrypoint for
  an existing `.release-prepared.json` state. It summarizes the prepared
  version/tag, highlights stale state plus working-tree drift since prepare
  time, and now also surfaces branch/HEAD/content drift from prepared-state
  source fingerprints. In text mode it can hand operators directly into the
  interactive execute review flow without rediscovering that state from
  scratch, and it now exposes direct `gates`, `reprepare`, and `discard`
  shortcuts for common recovery paths.
- Plain `effigy release execute` is now available in text mode as a final
  menu-driven review flow covering stale-state acknowledgement when needed,
  prepared-state review, working-tree review, and final approval before
  commit/tag/push. Operators can jump between those sections instead of stepping
  through them linearly, inspect one stale warning or working-tree item in
  detail before proceeding, and blocked execute preflights also allow that
  drill-down before returning the failure. The menu keeps the current stale
  acknowledgement state, prepared version/tag, a compact command legend, and
  reviewed section markers visible while the operator reviews. It now also
  exposes direct `gates`, `reprepare`, and `discard` recovery shortcuts from
  the review menu and the blocked-preflight browser.
- Blocked `effigy release prepare --plan`, `effigy release execute --plan`, and
  matching text-mode failure output now append suggested remediation actions so
  operators see the likely next fix path instead of only raw blocker strings.
- `effigy release execute --yes` is available as a non-interactive execution
  path that creates the release commit and tag, pushes branch and tag to
  `origin`, prints post-release monitoring instructions, and removes
  `.release-prepared.json` only after a full successful push. Stale prepared
  state requires explicit `--allow-stale` in this non-interactive path.
- The remaining prompt-driven roadmap work is about deeper editing and richer
  inline diffs, not the presence of staged human approvals.

1. **Determine the release version.**
   - Run `effigy release status --check-gates` as a preflight when the repo has
   `[release]` configured. Use it to spot version-file mismatches, changelog
   issues, and gate failures before applying any release changes.
   - Run `effigy release simulate` when you want the fullest safe preview:
     gates, planned file mutations, commit message, tag, and the exact
     no-state-file contract before touching the working tree. Add
     `--version <SEMVER>` when you want that dry-run to preview a deliberate
     valid version override without writing state.
   - Run `effigy release prepare --plan` when you want to preview the exact
     version-file and changelog mutations before touching the working tree.
   - Run `effigy release prepare` when you want the built-in text-mode approval
     flow: preview first, then confirm before Effigy writes release changes.
   - Use `effigy release prepare --yes --check-gates` only when you explicitly
     want Effigy to apply the prepared version/changelog changes and persist
     `.release-prepared.json` without committing or tagging yet.
   - Run `effigy release execute --plan` after preparation when you want
     Effigy to validate the prepared state file and current git working tree
     before any future commit/tag/push execution flow.
   - Run `effigy release execute` when you want the built-in text-mode final
     confirmation flow before commit/tag/push.
   - Use `effigy release execute --yes` only when you explicitly want Effigy
     to perform the irreversible release step non-interactively. If push fails,
     Effigy keeps the prepared state file and refuses to re-tag on retry.
   - Run `effigy release verify-install --tag <TAG> [--repo-url <URL>]` when
     you need tag-install validation.
   - Treat the built-in release previews as the source of truth for version
     selection:
     - `effigy release status --check-gates`
     - `effigy release simulate`
     - `effigy release prepare --plan`
   - If the human specifies a version, use it.
   - If the human says "patch" or "minor", compute the next version from the
     current release version and changelog state surfaced by the built-in
     release commands.
   - Confirm the target version with the human before proceeding.

2. **Prepare the version bump and changelog.**
   - Prefer the built-in prepare flow:
     - interactive operator path: `effigy release prepare`
     - non-interactive apply path:
       `effigy release prepare --yes --check-gates`
   - For repos using `[release]` config with `sync-files = ["Cargo.lock"]`,
     `effigy release prepare --yes` updates the version file, moves
     `[Unreleased]` entries into a dated release heading, syncs `Cargo.lock`
     when configured, and writes `.release-prepared.json`.
   - Review versioned install examples in user-facing docs, especially the root
     `README.md`, and refresh any explicit release tags so the front door does
     not lag the newly prepared version.
   - Review the changes. If the human specified a different version than the
     built-in suggestion, use the built-in custom-version path instead:
     `effigy release prepare --yes --check-gates --version X.Y.Z`

3. **Draft release notes.**
   - Follow `036-release-notes-authoring-template-and-examples.md`.
   - Place in `docs/logs/YYYY-MM/` with the standard naming convention.
   - Use `effigy changelog extract CHANGELOG.md --version X.Y.Z` to extract the
     changelog body for that release as the starting point.
   - Treat the extracted changelog body as source material, then add summary,
     validation, rollback notes, and compatibility context for human review.
   - Present the draft to the human for review before continuing.

4. **Run release gates.**
   - Run `effigy release gates` when the repo has `[release.gates]` configured
     and you want the built-in sequential fail-fast gate runner.
   - Otherwise execute `effigy release gates` (or `cargo qa-release`).
   - All gates must pass. If any fail, fix the issue and re-run.
   - Do not proceed until gates pass cleanly.

5. **Commit the version bump and release notes.**
   - Single commit with message: `release: vX.Y.Z`
   - This commit must be on `main`.

6. **Create and push the git tag.**
   - `git tag vX.Y.Z`
   - `git push origin main --tags`
   - This triggers the CI release pipeline.

7. **Verify the release pipeline.**
   - Monitor CI to confirm the release workflow completes.
   - If CI fails, do not re-tag. Fix the issue, bump to the next `PATCH`, and
     start from step 1.

## 7) Agent Thread Protocols

### 7a) What Agents Must Do

- Treat the release gate pipeline as the single source of truth for publish
  readiness
- Use `effigy release gates` to validate before
  any release action
- Reference exact version numbers, never floating references
- Update consumer CI snippets to use the pattern in Section 5a
- Follow the release execution protocol in Section 6c exactly when asked to
  create a release
- Confirm the target version with the human before making any changes
- When making changes that affect user-facing behavior, append an entry to
  `CHANGELOG.md` under the appropriate `[Unreleased]` subsection (Breaking,
  Added, Changed, or Fixed)

### 7b) What Agents Must Not Do

- **Never initiate a release without explicit human instruction.** A human must
  ask for a release; agents do not decide when to release.
- **Never publish to crates.io directly.** Publication is triggered by CI from a
  tag, or performed manually by a human.
- **Never modify `.github/workflows/` without explicit human approval.** Workflow
  changes affect the release pipeline and require review.
- **Never bypass release gates.** If gates fail, fix the underlying issue; do
  not skip or weaken the gate.
- **Never re-tag a failed release.** If a tagged release fails in CI, the fix
  goes into the next `PATCH` version.

### 7c) What Agents May Do Autonomously

- Read and reference release documentation
- Run release gate checks locally (`effigy release gates`, `smoke:release`)
- Draft release notes for human review
- Use `effigy changelog extract` as the preferred release-note baseline
  generator before any workflow-level cutover
- Update consumer repo CI snippets to match the documented install pattern
- Add or update documentation about release processes
- Fix code issues that cause release gate failures

## 8) Active Workflows

All release workflows are now active in `.github/workflows/`:

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| `ci.yml` | PR, push to main | Format, clippy, tests |
| `release-binaries.yml` | Tag push `v*` | Gates → build → release → Homebrew tap |
| `json-contracts.yml` | PR, push to main, daily | JSON contract and docs link validation |

Workflow changes still require explicit human approval.

## 8a) Remaining Human-Gated Adoption Work

The release command surface is shipped in the codebase. The remaining open work
in this protocol is intentionally human-gated adoption work:

- keeping release docs, gate config, and operator guidance pointed at the
  built-in release commands
- continuing release monitoring and adoption follow-through after the first
  production built-in release

Historical workflow audit:
- [`../logs/2026-03/11-170500-release-binaries-changelog-extract-cutover-review.md`](../logs/2026-03/11-170500-release-binaries-changelog-extract-cutover-review.md)

## 8b) Wrapper Retirement Record

Effigy's compatibility-only release wrappers are already retired. The historical
retirement-record template lives at
[`archive/053-release-wrapper-retirement-record-template.md`](./archive/053-release-wrapper-retirement-record-template.md)
and is kept only so old logs linking to it do not dead-end.

If a new wrapper layer is ever retired in the future, record the decision in
the dated release checkpoint log
([`054-release-checkpoint-log-template.md`](./054-release-checkpoint-log-template.md))
rather than reviving the retired template.

Current operating stance:
- prefer built-in release commands for operator-driven runs
- legacy release compatibility wrappers are retired in this repo
- do not reintroduce wrapper-first guidance unless a new external contract
  genuinely requires it

## 9) Setup Action

The [`inflatable-cookie/setup-effigy`](https://github.com/inflatable-cookie/setup-effigy)
GitHub Action is published at `v1`. It is the recommended install method for
GitHub Actions workflows (see Section 5a).

Features:
- Platform detection (Linux x86_64/ARM64, macOS x86_64/ARM64)
- Binary caching via `actions/cache@v4`
- Self-hosted runner support (`$RUNNER_TOOL_CACHE` fallback)

```yaml
- uses: inflatable-cookie/setup-effigy@v1
  with:
    version: 'X.Y.Z'
```

## 10) Rollback

If a released binary is broken:
- Do not delete the GitHub Release or tag
- Create a new `PATCH` release with the fix
- Update consumer repos to pin the new version
- Follow the rollback procedure in `release-contract-v0.md`

## Related Guides

- [`010-path-installation-and-release.md`](./010-path-installation-and-release.md)
- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [`042-homebrew-tap-and-release-automation.md`](./042-homebrew-tap-and-release-automation.md)
- [`044-distribution-first-publish-execution-runbook.md`](./044-distribution-first-publish-execution-runbook.md)
- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)

## Next Step

Use the built-in release flow in guide `051` for future releases, then execute
the distribution follow-through in guide `044` when a release window requires
artifact-level acceptance checks.
