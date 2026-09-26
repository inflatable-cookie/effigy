# Effigy release procedure

Effigy ships a versioned binary through GitHub Releases and a Homebrew tap. The release configuration is in [`config/release.toml`](../../../config/release.toml). A human must explicitly request a release and confirm its version. Only `main` can be tagged. Do not publish Effigy's internal crates to crates.io.

## Prepare

1. Start from clean, pushed `main`. Record `candidate_sha=$(git rev-parse HEAD)`.
2. Dispatch `gh workflow run ci.yml --ref main`. Select the `workflow_dispatch` run for that exact SHA with `gh run list --workflow ci.yml --branch main --commit "$candidate_sha" --event workflow_dispatch --limit 1 --json databaseId,headSha,status,conclusion,url`, verify `headSha`, and wait with `gh run watch <RUN_ID> --exit-status`. Missing, pending, red, cancelled, or different-commit evidence blocks release.
3. Run `effigy release status --check-gates`, `effigy release simulate`, and `effigy release prepare --plan`. Confirm the target version with Tom. During `v0.x`, PATCH is for compatible fixes; MINOR may break behavior with explicit migration notes. CI installs should pin exact versions.
4. Run `effigy release prepare --yes --check-gates`. It updates `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and prepared state. In the same release change, update `support/catalog-pack-update.toml` (`as_of_release`, `required_versions`, and, when needed, `oldest_update_capable_release`), `PackUpdateCapability::for_this_build` and its tests. Review versioned install examples in `README.md`.
5. Draft release notes from `effigy changelog extract CHANGELOG.md --version X.Y.Z` for Tom's review. Keep the reviewed notes with the GitHub release; routine release logs are not repository knowledge.

## Execute

1. Run `effigy release gates`; fix any failure and rerun. The `ci` gate checks the hosted run against the exact candidate source SHA.
2. Review `effigy release execute --plan`. `effigy release execute --yes` commits the prepared files on `main` as `release: vX.Y.Z`, creates and pushes the annotated `vX.Y.Z` tag, and clears prepared state. Use it only after Tom explicitly authorizes the release.
3. Dispatch `gh workflow run release-binaries.yml -f tag=vX.Y.Z` and watch the run. A pushed tag alone does not publish the binaries.

## Verify and recover

- After artifacts publish, run `effigy release verify-install --tag vX.Y.Z`. Check the GitHub release and Homebrew tap.
- If publication fails after tagging, keep the tag. Fix the cause and release the next PATCH; never re-tag a failed release.
- For a broken published binary, pause new publishes, tell consumers the affected version, point install guidance at the last good version, and prepare a PATCH fix.

[Release workflow guide](../../guides/051-release-orchestration.md) documents the CLI. [Distribution policy](../../guides/049-ci-binary-distribution-and-release-protocol.md) covers the channel and platform matrix.
