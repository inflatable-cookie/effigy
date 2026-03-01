# 042 - Homebrew Tap and Release Automation

This guide defines the Homebrew channel workflow for Effigy releases, including formula updates, checksum strategy, and release automation hooks.

## 1) Scope

- Tap repo workflow for `brew install` and `brew upgrade`.
- Release-tag formula bump process.
- Bottle/checksum/update strategy and rollback policy.

## 2) Repository Layout

Recommended split:
- core repo: `inflatable-cookie/effigy`
- tap repo: `inflatable-cookie/homebrew-effigy`
- formula path in tap: `Formula/effigy.rb`

Formula source should reference:
- release tarball from `inflatable-cookie/effigy` tags
- stable semantic version tag (for example `v0.2.3`)

## 3) Formula Strategy

Use one canonical formula:
- name: `effigy`
- install command:
  - `brew install inflatable-cookie/effigy/effigy`
- upgrade command:
  - `brew upgrade effigy`

Do not maintain parallel formula variants (`effigy-dev`, `effigy-beta`) until channel policy explicitly adds them.

## 4) Release Tag Bump Workflow

Release flow:
1. Tag is created in core repo (`vX.Y.Z`).
2. CI job computes tarball SHA256 for the tag.
3. CI updates `Formula/effigy.rb` with:
   - `url` pointing at the tag tarball
   - `sha256` matching the tarball
   - `version` if needed
4. CI opens or pushes a tap PR/commit.
5. Formula CI smoke:
   - `brew audit --strict --formula Formula/effigy.rb`
   - `brew style Formula/effigy.rb`
   - `brew install --build-from-source ./Formula/effigy.rb`
6. Merge tap change after checks pass.

## 5) Checksum and Bottle Policy

Current policy (phase C baseline):
- required: source tarball `sha256` update on every release.
- optional: bottled binaries may be added later; source install remains baseline.

When bottles are introduced:
- generate bottles from release tag source.
- publish bottle artifacts and update formula bottle block in one atomic PR.
- keep source build path valid as fallback.

## 6) Automation Hooks

Add a release-triggered workflow in core repo:
- trigger: tag push `v*`
- responsibilities:
  - run release gates (`./scripts/check-release-gates.sh`)
  - compute release tarball checksum
  - call tap update automation (or open PR)

Tap repo workflow should run:
- `brew audit --strict --formula`
- `brew style`
- build-from-source smoke install

## 7) Rollback and Recovery

If Homebrew update is broken:
1. Revert tap formula commit to previous known-good version.
2. Publish hotfix formula bump if needed.
3. Keep pinned install guidance available via:
   - `cargo install --git ... --tag <known-good>`

## 8) Validation Matrix

Per release:
- fresh install:
  - `brew install inflatable-cookie/effigy/effigy`
- upgrade path:
  - install old version, then `brew upgrade effigy`
- command smoke:
  - `effigy --help`
  - `effigy --json tasks`
  - `effigy test --plan`

## Related Guides

- [`010-path-installation-and-release.md`](./010-path-installation-and-release.md)
- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)

## Next Step

After the first tap automation run is stable, close the distribution backlog phase C items and proceed to phase E wrapper reassessment.
