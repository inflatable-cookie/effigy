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

[distribution.publish]
binary-name = "my-tool"
registry-label = "registry"

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

This contract is still intentionally bounded:

- package identity defaults
- publish identity defaults
- preflight task names
- metadata file requirements
- closeout defaults

That is enough to make `validate-metadata`, `preflight`, `first-publish`,
`write-summary`, and `generate-closeout` meaningfully repo-configurable
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
- `distribution first-publish` when package/publish/closeout policy fits your repo

### Still Effigy-Biased Today

- `distribution validate-metadata`
- `distribution preflight`

Those commands currently carry more Effigy-specific assumptions about things
like docs, workflow expectations, and default preflight task names. The active
product direction is to keep moving those assumptions behind optional manifest
config instead of baking them into the command surface.

## Recommended Adoption Pattern

1. Start with one reusable primitive.
2. Add publish identity and closeout defaults if your package/binary/channel
   names differ from Effigy's self-hosting defaults.
3. Add artifact validation and closeout generation if evidence matters.
4. Adopt manifest-driven preflight and publish orchestration only if it fits
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

You should know which parts of the distribution surface are already reusable,
which manifest sections now shape publish/closeout behavior, and which policy
areas still remain intentionally repo-owned.

## Next Step

If you want reusable distribution support across repos, start with the current
optional `[distribution]` contract, then decide whether one consumer-proof
adoption is now honest enough or whether one final internal policy widening
slice is still warranted.
