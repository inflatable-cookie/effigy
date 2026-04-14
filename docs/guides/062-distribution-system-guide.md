# Distribution System Guide

Use this guide when you want Effigy to help with release and distribution work
without hardwiring your repo to Effigy's exact release process.

Effigy's distribution surface is intended to be:

- built in
- optional
- composable
- evidence-oriented

That means a repo can use one distribution primitive, or adopt a fuller
distribution flow, without accepting a single mandatory release protocol.

## What Exists Today

Effigy already ships these distribution commands:

- `effigy distribution validate-metadata`
- `effigy distribution check-glibc-floor`
- `effigy distribution preflight`
- `effigy distribution first-publish`
- `effigy distribution validate-artifacts`
- `effigy distribution generate-closeout`
- `effigy distribution write-summary`

Some of those commands are already broadly reusable. Others still reflect
Effigy's self-hosting defaults and are being moved toward a manifest-driven
optional contract.

## Minimal Manifest Contract

Repos can now start shaping distribution policy in `effigy.toml` with an
optional `[distribution]` section.

```toml
[distribution.package]
name = "my-tool"
repo-url = "https://github.com/example/my-tool.git"
brew-formula = "example/tap/my-tool"

[distribution.preflight]
docs-task = "qa:docs"
smoke-task = "dist:preflight:smoke"

[distribution.metadata]
required-docs = ["docs/guides/installation.md", "docs/guides/release.md"]
required-files = [".github/workflows/release-binaries.yml", "scripts/check-linux-glibc-floor.sh"]
```

This first contract intentionally stays narrow:

- package identity defaults
- preflight task names
- metadata file requirements

That is enough to make `validate-metadata` and `preflight` repo-configurable
without forcing a full release-orchestration model on every repo.

## Use Levels

### Use One Primitive Only

If you only need one check or one report, use that command directly.

Examples:

```sh
effigy distribution check-glibc-floor --binary ./target/release/my-tool --max-glibc 2.35
effigy distribution validate-artifacts --artifacts-dir ./artifacts/distribution
effigy distribution generate-closeout --tag v1.2.3 --artifacts-dir ./artifacts/distribution
```

### Use Validation and Evidence Only

If your repo already has a release process but lacks consistent evidence
capture, use:

- `validate-artifacts`
- `write-summary`
- `generate-closeout`

That gives you a stable validation/report layer without replacing your current
publish flow.

### Use a Fuller Distribution Flow

If your repo wants Effigy to orchestrate preflight and first-publish checks,
the intended direction is an optional manifest-driven distribution contract.

That surface is now being productized so repos can opt in to:

- package identity
- preflight tasks
- enabled channels
- artifact expectations
- closeout behavior

without inheriting Effigy's exact release policy.

## Current Generic vs Self-Hosting Boundary

### Strongly Reusable Today

- `distribution check-glibc-floor`
- `distribution validate-artifacts`
- `distribution generate-closeout`
- `distribution write-summary`

### Still Effigy-Biased Today

- `distribution validate-metadata`
- `distribution preflight`
- `distribution first-publish`

Those commands currently carry more Effigy-specific assumptions about things
like docs, workflow expectations, and channel defaults. The active product
direction is to move those assumptions behind optional manifest config.

## Recommended Adoption Pattern

1. Start with one reusable primitive.
2. Add artifact validation and closeout generation if evidence matters.
3. Adopt manifest-driven preflight and publish orchestration only if it fits
   your repo's release model.

This keeps distribution support helpful instead of prescriptive.

## Related Guides

- [041-distribution-ci-pinning-and-wrapper-migration.md](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [042-homebrew-tap-and-release-automation.md](./042-homebrew-tap-and-release-automation.md)
- [044-distribution-first-publish-execution-runbook.md](./044-distribution-first-publish-execution-runbook.md)
- [049-ci-binary-distribution-and-release-protocol.md](./049-ci-binary-distribution-and-release-protocol.md)
- [051-release-orchestration.md](./051-release-orchestration.md)
- [059-manifest-composition-guide.md](./059-manifest-composition-guide.md)

## Expected Outcome

You should know which parts of the distribution surface are already useful
today, which parts are still self-hosting-biased, and how the optional
manifest-driven adoption path is intended to evolve.

## Next Step

If you want reusable distribution support across repos, implement the minimal
manifest-driven `[distribution]` contract first, then widen command coverage
from that foundation.
