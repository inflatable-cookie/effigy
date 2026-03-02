# 044 - Distribution First-Publish Execution Runbook

Use this runbook for the first release cycle that should close remaining Distribution acceptance criteria.

## 1) Preconditions

- Release candidate commit is selected.
- Version bump and release notes are prepared.
- Homebrew tap automation path is configured.
  - `.github/workflows/homebrew-tap-metadata.yml`
  - `.github/workflows/homebrew-tap-formula-pr.yml`
- CI pinning guide and wrapper policy docs are already in place.

Required inputs:
- release tag: `vX.Y.Z`
- previous known-good version/tag for rollback

Recommended preflight before opening publish window:

```bash
./scripts/check-distribution-preflight.sh --tag vX.Y.Z
```

## 2) Execution Order

Run this sequence in one release window:
1. Create and push release tag.
2. Validate install from tag.
3. Publish crate and validate crates.io install.
4. Update Homebrew formula and validate fresh install + upgrade path.
5. Capture one consolidated channel matrix report.

Optional one-command execution helper:

```bash
./scripts/check-distribution-first-publish.sh --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z
./scripts/generate-distribution-closeout-report.sh --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z
# use --expect-homebrew when Homebrew checks are expected in this release window
# ./scripts/generate-distribution-closeout-report.sh --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z --expect-homebrew
```

## 3) Command Matrix

### Tag Install Validation

```bash
./scripts/check-release-install-from-tag.sh --tag vX.Y.Z
```

### Crates.io Install Validation

```bash
cargo install effigy --version X.Y.Z --locked --force
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
- crates.io install output log
- Homebrew fresh install + upgrade logs
- CI pinned install log
- one dated checkpoint report in `docs/reports/`

When using the helper script, attach per-step logs from `--artifacts-dir` directly in the checkpoint report.
The closeout report can be generated from those logs using `generate-distribution-closeout-report.sh`.
Artifact completeness can be checked directly with `validate-distribution-artifacts.sh`.
The first-publish helper also writes `distribution-summary.env` in the artifacts directory and validates artifacts before returning success.
Local tooling sanity for this pipeline can be checked with `check-distribution-artifact-pipeline-smoke.sh`.

## 5) Acceptance Criteria Mapping

This runbook is the execution evidence source for:
- one-command install for Rust-native + macOS-default users
- documented/tested version pinning and rollback
- repeatable release and upgrade flow from CI

`Channel docs distinguish dev vs stable channels` is already completed by prior docs batches.

## 6) Failure Handling

If any channel fails:
1. stop rollout completion claims for this batch
2. record failure in checkpoint report with exact command and output summary
3. rollback to previous known-good version/tag
4. re-run failed channel only after fix is merged

## Related Guides

- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [`042-homebrew-tap-and-release-automation.md`](./042-homebrew-tap-and-release-automation.md)
- [`043-wrapper-channel-evaluation-and-policy.md`](./043-wrapper-channel-evaluation-and-policy.md)

## Next Step

When a release tag exists, execute this runbook and publish a single acceptance-closeout report that updates remaining criteria in `distribution-channels.md`.
