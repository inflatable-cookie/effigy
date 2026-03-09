# 042 - Homebrew Tap and Release Automation

This guide defines the Homebrew channel workflow for Effigy releases, including formula updates, checksum strategy, and release automation hooks.

## 1) Scope

- Tap repo workflow for `brew install` and `brew upgrade`.
- Release-tag formula bump process.
- Bottle/checksum/update strategy and rollback policy.

## 2) Repository Layout

Recommended split:
- core repo: `inflatable-cookie/effigy`
- tap repo: `inflatable-cookie/homebrew-tap`
- formula path in tap: `Formula/effigy.rb`

Formula source should reference:
- prebuilt binaries from `inflatable-cookie/effigy` GitHub Releases
- stable semantic version tag (for example `v0.2.3`)

## 3) Formula Strategy

Use one canonical formula:
- name: `effigy`
- install command:
  - `brew install inflatable-cookie/tap/effigy`
- upgrade command:
  - `brew upgrade effigy`

Do not maintain parallel formula variants (`effigy-dev`, `effigy-beta`) until channel policy explicitly adds them.

## 4) Release Tag Bump Workflow

Release flow:
1. Tag is created in core repo (`vX.Y.Z`).
2. `release-binaries.yml` runs: gates → build (3 targets) → GitHub Release → homebrew job.
3. Homebrew job downloads published macOS binaries and computes per-architecture SHA256 hashes.
4. Homebrew job checks out tap repo, regenerates `Formula/effigy.rb` with new URLs/hashes, commits, and pushes.

Build matrix targets:
- `aarch64-apple-darwin` (Apple Silicon)
- `x86_64-apple-darwin` (Intel Mac)
- `x86_64-unknown-linux-gnu` (Linux — not in formula, available as direct download)

## 5) Formula Design

The formula uses prebuilt binaries (not source builds):

```ruby
on_macos do
  if Hardware::CPU.arm?
    url "<release-url>/effigy-aarch64-apple-darwin"
    sha256 "<arm64-hash>"
  elsif Hardware::CPU.intel?
    url "<release-url>/effigy-x86_64-apple-darwin"
    sha256 "<x86_64-hash>"
  end
end

def install
  binary = stable.url.split("/").last
  bin.install binary => "effigy"
end
```

Each release updates both architecture URLs and their corresponding SHA256 hashes.

## 6) Automation

Implemented as the `homebrew` job in `.github/workflows/release-binaries.yml`:
- trigger: runs after the `release` job succeeds (tag push `v*`)
- guard: `if: ${{ secrets.EFFIGY_TAP_GH_TOKEN != '' }}` (skips gracefully if secret not configured)
- steps:
  1. Download both macOS binaries from the just-created GitHub Release
  2. Compute SHA256 hashes via `curl | sha256sum`
  3. Check out `inflatable-cookie/homebrew-tap` using PAT
  4. Write updated `Formula/effigy.rb` via heredoc
  5. Commit and push directly to tap main branch

Required repository wiring:
- secret: `EFFIGY_TAP_GH_TOKEN` (PAT with `contents:write` access to tap repo)
- tap repo: `inflatable-cookie/homebrew-tap`

## 7) Rollback and Recovery

If Homebrew update is broken:
1. Revert tap formula commit to previous known-good version.
2. Publish hotfix formula bump if needed.
3. Keep pinned install guidance available via:
   - `cargo install --git ... --tag <known-good>`

## 8) Validation Matrix

Per release:
- fresh install:
  - `brew install inflatable-cookie/tap/effigy`
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

After the first production run, attach the generated tap PR URL in a dated checkpoint log and proceed to first-publish channel matrix execution.
