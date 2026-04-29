# 062 - Distribution Commands

Use this guide when you want Effigy to help with release and distribution work
without hardwiring your repo to Effigy's exact release process.

This is the public front door for the distribution surface.

If you only want one page first, use this one. Reach for the deeper release
policy and orchestration guides only after you know you need them.

Use:
- this guide for `effigy distribution ...`
- [`051-release-orchestration.md`](./051-release-orchestration.md) for the
  release cut workflow
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
  for maintainer policy and binary channel rules
- [`052-changelog-workflows-and-northstar-profile.md`](./052-changelog-workflows-and-northstar-profile.md)
  for changelog work

This surface is built to be optional. A repo can use one command, or adopt a
larger evidence flow, without taking Effigy's whole release process.

## Start Here

If you are deciding where to start:

- cutting a normal release: read `051`
- validating or rehearsing publish/distribution evidence: read this guide
- setting CI install policy or binary channel rules: read `049`
- extracting or validating release notes: read `052`

## Start With The Right Level

- Need one focused check or report: use a single `distribution` subcommand.
- Need evidence capture around an existing release flow: use
  `validate-artifacts`, `write-summary`, and `generate-closeout`.
- Need the release cut itself: use [`051-release-orchestration.md`](./051-release-orchestration.md).
- Need channel rules or CI install policy: use
  [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md).

## What Exists Today

Effigy already ships these distribution commands:

- `effigy distribution validate-metadata`
- `effigy distribution check-glibc-floor`
- `effigy distribution preflight`
- `effigy distribution first-publish`
- `effigy distribution validate-artifacts`
- `effigy distribution generate-closeout`
- `effigy distribution write-summary`

The reusable core today is validation and evidence. The fuller
`first-publish` path still carries more Effigy and Cargo assumptions.

### Subcommand Matrix

| Subcommand | Purpose | Key Flags | JSON Schema |
| --- | --- | --- | --- |
| `preflight` | Run pre-publish checks (docs, smoke, metadata) and write a `.env`-style preflight report for downstream tooling | `--tag`, `--skip-docs`, `--skip-smoke`, `--output`, `--json` | `effigy.distribution.preflight.v1` |
| `validate-metadata` | Validate declared `[distribution.metadata]` file expectations for the release tag | `--tag`, `--json` | `effigy.distribution.metadata.v1` |
| `check-glibc-floor` | Check a compiled Linux binary's required GLIBC floor against a policy max | `--binary`, `--max-glibc`, `--json` | (command envelope; no dedicated payload schema) |
| `first-publish` | Side-effecting primary publish orchestration: verify-install, cargo publish, optional Homebrew tap update, summary write, artifact validation | `--tag`, `--crate-version`, `--repo-url`, `--brew-formula`, `--skip-homebrew`, `--artifacts-dir`, `--json` | (emits command-envelope JSON with per-step logs) |
| `validate-artifacts` | Check that the `--artifacts-dir` contains the expected per-channel logs and a distribution summary | `--artifacts-dir`, `--expect-homebrew`, `--json` | `effigy.distribution.artifacts.v1` |
| `generate-closeout` | Generate the dated release closeout log from captured per-channel logs | `--tag`, `--artifacts-dir`, `--output`, `--owner`, `--expect-homebrew`, `--json` | `effigy.distribution.closeout.v1` |
| `write-summary` | Write `distribution-summary.env` into `--artifacts-dir` from the given tag/channel evidence | `--tag`, `--artifacts-dir`, `--crate-version`, `--repo-url`, `--brew-formula`, `--homebrew-executed`, `--log-file`, `--json` | `effigy.distribution.summary.v1` |

Typical full distribution cycle:

1. `effigy distribution preflight --tag vX.Y.Z --output ./artifacts/distribution-preflight-vX.Y.Z.env`
2. `effigy distribution first-publish --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z`
3. `effigy distribution validate-artifacts --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z`
4. `effigy distribution generate-closeout --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z`

`first-publish` already runs `effigy release verify-install`, writes
`distribution-summary.env`, and validates captured artifacts before succeeding.
The later validate/closeout steps are for evidence and sign-off.

## Minimal Manifest Contract

Repos can now start shaping distribution policy in `effigy.toml` with an
optional `[distribution]` section.

```toml
[distribution.package]
name = "my-tool"
repo-url = "https://github.com/example/my-tool.git"
brew-formula = "example/tap/my-tool"

[distribution.publish]
binary-name = "my-tool"
registry-label = "registry"
verify-tag-install = true
verify-binary-json-tasks = true

[distribution.preflight]
docs-task = "qa:docs"
smoke-task = "dist:preflight:smoke"

[distribution.metadata]
required-docs = ["docs/guides/installation.md", "docs/guides/release.md"]
required-files = [".github/workflows/release-binaries.yml", "scripts/check-linux-glibc-floor.sh"]

[distribution.closeout]
owner = "release"
related = "docs/roadmaps/distribution.md"
next-step = "Review the captured evidence and publish release sign-off notes."
```

This contract is intentionally small. It gives repos enough control over
identity, preflight tasks, metadata expectations, and closeout defaults without
forcing a full Effigy-shaped release model.

When a repo adopts `[distribution]`, `validate-metadata` no longer assumes
Effigy's default workflow/docs package-quality gate unless the repo explicitly
opts into that metadata policy.

## Recommended Adoption Path

### 1. Use One Primitive First

If you only need one check or one report, use that command directly.

Examples:

```sh
effigy distribution check-glibc-floor --binary ./target/release/my-tool --max-glibc 2.35
effigy distribution validate-artifacts --artifacts-dir ./artifacts/distribution
effigy distribution generate-closeout --tag v1.2.3 --artifacts-dir ./artifacts/distribution
```

### 2. Add Validation and Evidence

If your repo already has a release process but lacks consistent evidence
capture, use:

- `validate-artifacts`
- `write-summary`
- `generate-closeout`

This gives you a stable validation and reporting layer without replacing your
publish flow.

### 3. Add The Manifest Contract

Add `[distribution]` when your package identity, preflight tasks, artifact
expectations, or closeout defaults differ from Effigy's self-hosting defaults.

### 4. Use `first-publish` Only When It Fits

Use `first-publish` when your repo fits the current built-in publish and
verification model. If your release path is materially different, keep that
part repo-owned and use the validation and evidence commands around it.

## Current Boundary

### Strongly Reusable Today

- `distribution check-glibc-floor`
- `distribution validate-metadata` when package identity and file expectations
  are declared through the manifest instead of inherited from Effigy's
  self-hosting defaults
- `distribution validate-artifacts`
- `distribution generate-closeout`
- `distribution write-summary`
- bounded `distribution first-publish` when package, publish, and closeout
  policy fit your repo and any Effigy-specific install probes are either
  supported or disabled through `[distribution.publish]`

### Still Bounded Or Effigy-Biased Today

- `distribution preflight`
- `distribution first-publish` for repos that need a non-Cargo install source
  or a broader publish-orchestration model than the current built-in matrix

Those commands still assume more about docs checks, smoke shape, publish flow,
and verification. Treat them as bounded tooling, not a universal release
framework.

## Related Guides

- [049-ci-binary-distribution-and-release-protocol.md](./049-ci-binary-distribution-and-release-protocol.md)
- [051-release-orchestration.md](./051-release-orchestration.md)
- [052-changelog-workflows-and-northstar-profile.md](./052-changelog-workflows-and-northstar-profile.md)
- [059-manifest-composition-guide.md](./059-manifest-composition-guide.md)

## Expected Outcome

You should know which commands are broadly reusable, where the current
manifest contract helps, and where repo-owned release policy should still stay
outside this surface.

## Next Step

Start with `validate-artifacts`, `write-summary`, and `generate-closeout`.
Add `[distribution]` when identity or closeout policy differs from the default.
