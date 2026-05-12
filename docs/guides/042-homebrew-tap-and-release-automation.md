> Status: Deprecated
> Superseded by: [`062-distribution-system-guide.md`](./062-distribution-system-guide.md) and [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
> Kept for: historical Homebrew-tap implementation detail

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

Do not maintain parallel formula variants (`effigy-beta`, alternate channels, or checkout-specific formulas) until channel policy explicitly adds them.

## 4) Release Tag Bump Workflow

Release flow:
1. Tag is created in core repo (`vX.Y.Z`).
2. `release-binaries.yml` runs: gates → build (3 targets) → GitHub Release → homebrew job.
3. Homebrew job downloads published macOS binaries and computes per-architecture SHA256 hashes.
4. Homebrew job checks out tap repo, regenerates `Formula/effigy.rb` with new URLs/hashes, commits, and pushes.

Build matrix targets:
- `aarch64-apple-darwin` (Apple Silicon)
- `x86_64-apple-darwin` (Intel Mac)
- `x86_64-unknown-linux-gnu` (Linux x86_64)
- `aarch64-unknown-linux-gnu` (Linux ARM64, cross-compiled)

Linux GNU release artifacts are built on an Ubuntu 22.04 baseline and checked
with `effigy distribution check-glibc-floor` so published binaries do not drift to
newer glibc requirements unexpectedly.

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

on_linux do
  if Hardware::CPU.intel?
    url "<release-url>/effigy-x86_64-unknown-linux-gnu"
    sha256 "<linux-x86_64-hash>"
  elsif Hardware::CPU.arm?
    url "<release-url>/effigy-aarch64-unknown-linux-gnu"
    sha256 "<linux-arm64-hash>"
  end
end

def install
  binary = stable.url.split("/").last
  bin.install binary => "effigy"
end
```

Each release updates all architecture URLs and their corresponding SHA256 hashes
for macOS (arm64, x86_64) and Linux (x86_64).

## 6) Automation

Implemented as the `homebrew` job in `.github/workflows/release-binaries.yml`:
- trigger: runs after the `release` job succeeds (tag push `v*`)
- guard: `if: needs.release.outputs.has-tap-token == 'true'` (the `release` job
  checks `EFFIGY_TAP_GH_TOKEN` at step level and exports an output; `secrets`
  context is not available in job-level `if` conditions)
- steps:
  1. Download macOS and Linux binaries from the just-created GitHub Release
  2. Compute SHA256 hashes via `curl | sha256sum`
  3. Check out `inflatable-cookie/homebrew-tap` using PAT
  4. Write updated `Formula/effigy.rb` via heredoc (macOS arm64/x86_64 + Linux x86_64)
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
- fresh install (macOS):
  - `brew install inflatable-cookie/tap/effigy`
- fresh install (Linux):
  - `brew install inflatable-cookie/tap/effigy` (via Linuxbrew)
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

After the first production run, attach the generated tap PR URL in a dated
checkpoint log using
[`054-release-checkpoint-log-template.md`](./054-release-checkpoint-log-template.md)
and proceed to first-publish channel matrix execution.
