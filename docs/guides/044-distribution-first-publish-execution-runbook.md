> Status: Deprecated
> Superseded by: [`062-distribution-system-guide.md`](./062-distribution-system-guide.md) and [`051-release-orchestration.md`](./051-release-orchestration.md)
> Kept for: historical first-publish execution detail
> 
> Note: This runbook originally included crates.io publish validation. Effigy
> does not publish to crates.io because its workspace contains app-specific
> internal crates not intended as reusable library dependencies. Binary
> distribution (GitHub Releases + Homebrew) and source install (`cargo install
> --git --tag`) are the supported channels.

# 044 - Distribution First-Publish Execution Runbook

Use this runbook for the first release cycle that should close remaining Distribution acceptance criteria.

## 1) Preconditions

- Release candidate commit is selected.
- Version bump and release notes are prepared.
- Homebrew tap automation path is configured.
  - `.github/workflows/release-binaries.yml` (includes homebrew metadata generation and tap PR automation)
- CI pinning guide and wrapper policy docs are already in place.

Required inputs:
- release tag: `vX.Y.Z`
- previous known-good version/tag for rollback

Recommended preflight before opening publish window:

```bash
effigy distribution preflight --tag vX.Y.Z --output ./artifacts/distribution-preflight-vX.Y.Z.env
```

Use `effigy distribution first-publish` as the side-effecting built-in for the
real publish/install cycle.

## 2) Execution Order

Run this sequence in one release window:
1. Create and push release tag.
2. Validate install from tag.
3. Update Homebrew formula and validate fresh install + upgrade path.
4. Capture one consolidated channel matrix log.

Optional one-command execution helper:

```bash
effigy distribution first-publish --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z
effigy distribution generate-closeout --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z
# use --expect-homebrew when Homebrew checks are expected in this release window
# effigy distribution generate-closeout --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z --expect-homebrew
```

The built-in owns the Homebrew execution steps plus per-step log capture.
Tag-install verification, artifact-summary writing, and artifact validation
remain native Effigy subcommands inside the same workflow.

## 3) Command Matrix

### Tag Install Validation

```bash
effigy release verify-install --tag vX.Y.Z
```

### Source Install Validation

```bash
cargo install \
  --locked \
  --git https://github.com/inflatable-cookie/effigy.git \
  --tag vX.Y.Z \
  effigy \
  --force
effigy --help
effigy --json tasks
```

### Homebrew Validation

```bash
brew install inflatable-cookie/effigy/effigy
effigy --help
effigy --json tasks
effigy test --plan
```

Upgrade path:

```bash
brew upgrade effigy
effigy --help
```

### CI Pinned Install Validation

```bash
cargo install \
  --locked \
  --git https://github.com/inflatable-cookie/effigy.git \
  --tag vX.Y.Z \
  effigy \
  --force
effigy --json help
```

## 4) Required Evidence Artifacts

- tag-install output log
- source-install output log
- Homebrew fresh install + upgrade logs
- CI pinned install log
- one dated checkpoint log in `docs/logs/YYYY-MM/`

When using the built-in, attach per-step logs from `--artifacts-dir` directly in the checkpoint log.
The closeout log can be generated from those logs using `effigy distribution generate-closeout`.
Artifact completeness can be checked directly with `effigy distribution validate-artifacts`.
The first-publish built-in uses `effigy release verify-install`, writes `distribution-summary.env` through `effigy distribution write-summary`, and performs final artifact completeness checks through `effigy distribution validate-artifacts` before returning success.
Local tooling sanity for this pipeline can be checked with `cargo test --test cli_output_tests cli_distribution_artifact_pipeline_smoke_fixture_passes -- --nocapture`, which exercises the built-in distribution commands directly.

## 5) Acceptance Criteria Mapping

This runbook is the execution evidence source for:
- one-command install for Rust-native + macOS-default users
- documented/tested version pinning and rollback
- repeatable release and upgrade flow from CI

`Channel docs distinguish dev vs stable channels` is already completed by prior docs batches.

## 6) Failure Handling

If any channel fails:
1. stop rollout completion claims for this batch
2. record failure in checkpoint log with exact command and output summary
3. rollback to previous known-good version/tag
4. re-run failed channel only after fix is merged

## Related Guides

- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`054-release-checkpoint-log-template.md`](./054-release-checkpoint-log-template.md)
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [`042-homebrew-tap-and-release-automation.md`](./042-homebrew-tap-and-release-automation.md)
- [`archive/043-wrapper-channel-evaluation-and-policy.md`](./archive/043-wrapper-channel-evaluation-and-policy.md)

## Next Step

When a release tag exists, execute this runbook and publish a single acceptance-closeout log that updates remaining criteria in `distribution-channels.md`.
