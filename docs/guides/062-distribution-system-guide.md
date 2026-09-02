# 062 - Distribution Commands

Use this guide when you want Effigy to help with release and distribution work
without hardwiring your repo to Effigy's exact release process.

This is the public front door for the distribution surface.

If you only want one page first, use this one. Reach for the deeper release
policy and orchestration guides only after you know you need them.

Use:
- this guide for `effigy deliver release ...`
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

For Effigy's own repo, the normal sequence is:

1. push the clean candidate commit to `main`
2. dispatch `ci.yml` and watch the exact candidate SHA to success
3. `effigy deliver release gates`
4. `effigy deliver release prepare --plan`
5. `effigy deliver release preflight --tag vX.Y.Z`
6. the remaining distribution evidence commands after the release mutation path
   is ready

## Start With The Right Level

- Need one focused check or report: use the matching `release` subcommand.
- Need evidence capture around an existing release flow: use
  `evidence validate`, `evidence summary`, and `evidence closeout`.
- Need the release cut itself: use [`051-release-orchestration.md`](./051-release-orchestration.md).
- Need channel rules or CI install policy: use
  [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md).

## What Exists Today

Effigy already ships these distribution commands:

- `effigy deliver release validate`
- `effigy deliver release check-binary`
- `effigy deliver release preflight`
- `effigy deliver release proof`
- `effigy deliver release evidence validate`
- `effigy deliver release evidence closeout`
- `effigy deliver release evidence summary`

The reusable core today is validation and evidence. The fuller
`proof` path still carries more Effigy and Cargo assumptions.

### Subcommand Matrix

| Subcommand | Purpose | Key Flags | JSON Schema |
| --- | --- | --- | --- |
| `preflight` | Run pre-publish checks (docs, smoke, metadata) and write a `.env`-style preflight report for downstream tooling | `--tag`, `--skip-docs`, `--skip-smoke`, `--output`, `--json` | `effigy.distribution.preflight.v1` |
| `validate` | Validate declared `[distribution.metadata]` file expectations for the release tag | `--tag`, `--json` | `effigy.distribution.metadata.v1` |
| `check-binary` | Check a compiled Linux binary's required GLIBC floor against a policy max | `<BIN>`, `--glibc-floor`, `--json` | (command envelope; no dedicated payload schema) |
| `proof` | Side-effecting publish evidence orchestration: verify-install, optional Homebrew tap update, summary write, artifact validation | `--tag`, `--repo-url`, `--brew-formula`, `--skip-homebrew`, `--artifacts-dir`, `--json` | (emits command-envelope JSON with per-step logs) |
| `evidence validate` | Check that the `--artifacts-dir` contains the expected per-channel logs and a distribution summary | `--artifacts-dir`, `--expect-homebrew`, `--json` | `effigy.distribution.artifacts.v1` |
| `evidence closeout` | Generate the dated release closeout log from captured per-channel logs | `--tag`, `--artifacts-dir`, `--output`, `--owner`, `--expect-homebrew`, `--json` | `effigy.distribution.closeout.v1` |
| `evidence summary` | Write `distribution-summary.env` into `--artifacts-dir` from the given tag/channel evidence | `--tag`, `--artifacts-dir`, `--crate-version`, `--repo-url`, `--brew-formula`, `--homebrew-executed`, `--log-file`, `--json` | `effigy.distribution.summary.v1` |

Typical full distribution cycle:

1. `effigy deliver release preflight --tag vX.Y.Z --output ./artifacts/distribution-preflight-vX.Y.Z.env`
2. `effigy deliver release proof --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z`
3. `effigy deliver release evidence validate --artifacts-dir ./artifacts/distribution-vX.Y.Z`
4. `effigy deliver release evidence closeout --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z`

`proof` already runs `effigy deliver release verify-install`, writes
`distribution-summary.env`, and validates captured artifacts before succeeding.
The later validate/closeout steps are for evidence and sign-off.

If you only need a quick readiness proof before a cut, start smaller:

```sh
effigy deliver release preflight --tag vX.Y.Z
```

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
required-files = [".github/workflows/release-binaries.yml"]

[distribution.closeout]
owner = "release"
related = "docs/roadmaps/distribution.md"
next-step = "Review the captured evidence and publish release sign-off notes."
```

This contract is intentionally small. It gives repos enough control over
identity, preflight tasks, metadata expectations, and closeout defaults without
forcing a full Effigy-shaped release model.

When a repo adopts `[distribution]`, `validate` no longer assumes
Effigy's default workflow/docs package-quality gate unless the repo explicitly
opts into that metadata policy.

## Recommended Adoption Path

### 1. Use One Primitive First

If you only need one check or one report, use that command directly.

Examples:

```sh
effigy deliver release check-binary ./target/release/my-tool --glibc-floor 2.35
effigy deliver release evidence validate --artifacts-dir ./artifacts/distribution
effigy deliver release evidence closeout --tag v1.2.3 --artifacts-dir ./artifacts/distribution
```

### 2. Add Validation and Evidence

If your repo already has a release process but lacks consistent evidence
capture, use:

- `evidence validate`
- `evidence summary`
- `evidence closeout`

This gives you a stable validation and reporting layer without replacing your
publish flow.

### 3. Add The Manifest Contract

Add `[distribution]` when your package identity, preflight tasks, artifact
expectations, or closeout defaults differ from Effigy's self-hosting defaults.

### 4. Use `proof` Only When It Fits

Use `proof` when your repo fits the current built-in publish and
verification model. If your release path is materially different, keep that
part repo-owned and use the validation and evidence commands around it.

## Current Boundary

### Strongly Reusable Today

- `release check-binary`
- `release validate` when package identity and file expectations
  are declared through the manifest instead of inherited from Effigy's
  self-hosting defaults
- `release evidence validate`
- `release evidence closeout`
- `release evidence summary`
- bounded `release proof` when package, publish, and closeout
  policy fit your repo and any Effigy-specific install probes are either
  supported or disabled through `[distribution.publish]`

### Still Bounded Or Effigy-Biased Today

- `release preflight`
- `release proof` for repos that need a non-Cargo install source
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

Start with `evidence validate`, `evidence summary`, and `evidence closeout`.
Add `[distribution]` when identity or closeout policy differs from the default.
