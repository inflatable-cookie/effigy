# Initial Current-Version Release Tag Closeout

## Result

Effigy now has an explicit `release.initial-tag-current-version` mode for a new
repository that already declares its intended first release version.

The exception is deliberately narrow:

- configuration must opt in
- the changelog must contain no released versions
- the selected version must equal the current version, never be lower
- the matching local tag must not exist
- normal release gates still run

Release planning omits the version-file mutation when the selected and current
versions match. Changelog promotion and any configured sync files remain in
the plan. Once a released changelog entry exists, normal strictly increasing
version behavior resumes.

## Consumer Proof

Swallowtail already declared `0.1.0` and had no release tags. With the new mode
enabled:

- `release status` reported current and next version `0.1.0`, tag `v0.1.0`,
  and no blocker
- `release prepare --plan` reported one changelog mutation and no version-file
  mutation
- `release simulate` ran all 11 configured gates in 52,143 ms
- all gates passed, including 1,463 stable tests with 11 skipped
- simulation wrote no prepared state

No release prepare, tag, commit, push, registry publication, GitHub Release,
or authenticated provider work ran.

## Validation

- `cargo test -p effigy-release`
- `cargo test -p effigy-doctor validate_manifest_schema_`
- `cargo test -p effigy validate_prepare_version_override_rejects_non_incrementing_versions --lib`
- focused library tests across `effigy`, `effigy-release`, `effigy-manifest`,
  and `effigy-doctor`
- `cargo clippy -p effigy-release -p effigy-manifest -p effigy-doctor -p effigy --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- `effigy qa:docs`
- installed local build self-doctor: 16 pass, one known god-file warning, zero
  errors

## Vision Target Delta

- Primary tags: `RELEASE`, `CONTRACT`, `OPERATE`
- Baseline: first release planning required a version strictly greater than the
  repository's version source, forcing new repositories toward a fake prior
  version, a wrong tag, or a manual bypass.
- Current state: explicit first-tag planning can reuse the declared version
  under closed, test-backed conditions and still run normal release gates.
- Remains open: None for this bounded mode. Actual release execution remains a
  separately authorized operation.
