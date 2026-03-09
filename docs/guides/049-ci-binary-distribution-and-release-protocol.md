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

- Version: `0.2.0`
- Active workflows in `.github/workflows/`:
  - `ci.yml` — PR and push validation (format, clippy, tests)
  - `release-binaries.yml` — tag-triggered: gates → build → GitHub Release → Homebrew tap
  - `json-contracts.yml` — JSON contract and docs link validation
- Distribution channels:
  - **Homebrew** — `brew install inflatable-cookie/tap/effigy` (macOS, prebuilt binaries)
  - **GitHub Releases** — prebuilt binaries for macOS (arm64, x86_64) and Linux (x86_64)
  - **`cargo install` from tag** — fallback when prebuilt binaries are unavailable

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
| `aarch64-unknown-linux-gnu`     | Linux | arm64  | Optional |

All three required targets are built and smoke-tested in CI (`release-binaries.yml`).
The Linux binary builds and runs `check-release-smoke.sh` on `ubuntu-latest`.

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
  │     └─ aarch64-unknown-linux-gnu (optional)
  │
  ├─ 3. Smoke test each binary on native runner
  │
  ├─ 4. Create GitHub Release with binaries attached
  │
  ├─ 5. Homebrew tap metadata + formula PR (existing)
  │
  └─ 6. crates.io publish (existing, when ready)
```

### 4c) Release Gate Prerequisite

The cross-compile and publish stages must not run unless release gates pass.
This is enforced by workflow job dependency, not by convention.

## 5) Consumer CI Install Pattern

### 5a) Recommended Snippet

Consumer repos should use this pattern in their CI configuration:

```yaml
- name: Install effigy
  run: |
    EFFIGY_VERSION="${EFFIGY_VERSION:-0.1.0}"
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

### 6b) Tagging Rules

- Tags must match `vMAJOR.MINOR.PATCH` exactly (e.g., `v0.1.0`)
- The tag version must match `Cargo.toml` version
- Tags must only be created on the `main` branch
- Tags must only be created after all release gates pass

### 6c) Release Execution Protocol

When a human asks an agent to create a release, the agent must follow these
steps in order. No step may be skipped. If any step fails, the agent must stop
and resolve the failure before continuing.

1. **Determine the release version.**
   - Run `./scripts/prepare-release.sh` to see the recommended bump type based
     on `CHANGELOG.md` [Unreleased] entries. Breaking entries → MINOR, otherwise
     → PATCH.
   - If the human specifies a version, use it.
   - If the human says "patch" or "minor", compute the next version from the
     current `Cargo.toml` version.
   - Confirm the target version with the human before proceeding.

2. **Prepare the version bump and changelog.**
   - Run `./scripts/prepare-release.sh --apply` to update `Cargo.toml` version,
     move [Unreleased] entries to a dated version heading, and sync `Cargo.lock`.
   - Review the changes. If the human specified a different version than the
     script computed, update `Cargo.toml` manually instead.

3. **Draft release notes.**
   - Follow `036-release-notes-authoring-template-and-examples.md`.
   - Place in `docs/logs/YYYY-MM/` with the standard naming convention.
   - Use the `CHANGELOG.md` entries for the version as a starting point.
   - Present the draft to the human for review before continuing.

4. **Run release gates.**
   - Execute `cargo qa-release` (or the underlying scripts).
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
- Use `effigy qa:release` (or the underlying scripts) to validate before
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
- Run release gate checks locally (`cargo qa-release`, smoke scripts)
- Draft release notes for human review
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

## 9) Setup Action (Future)

When multiple consumer repos need Effigy in CI, consider publishing a reusable
GitHub Action:

```yaml
- uses: inflatable-cookie/setup-effigy@v1
  with:
    version: '0.2.0'
```

This is not required for initial rollout. Evaluate after three or more consumer
repos are using the curl-based install pattern.

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

When ready to execute the first release, follow the activation sequence in
Section 8, then execute the first-publish runbook in guide 044.
