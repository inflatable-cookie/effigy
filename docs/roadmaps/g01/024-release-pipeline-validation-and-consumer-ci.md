# 024 - Release Pipeline Validation and Consumer CI Integration

Generation: `g01`

Status: Complete
Owner: Platform
Created: 2026-03-09
Depends on: 015

## Vision Alignment

This roadmap validates the full automated release pipeline end-to-end and
delivers the original distribution goal: CI runners in consumer repos can
install a pinned Effigy binary in seconds without a Rust toolchain.

## Primary Tags

- `RELEASE`
- `OPERATE`
- `MAINT`

## Target Envelope

- The release pipeline (tag → gates → build → GitHub Release → Homebrew tap)
  runs successfully with automated Homebrew formula updates.
- Consumer repos install Effigy from GitHub Releases in CI workflows.
- JSON contracts CI enforcement is validated on real PRs.

## Vision Target Delta

- Moved from `release pipeline exists but Homebrew automation is untested` toward
  `full automated release with multi-channel distribution and consumer adoption`.

## 1) Validate Release Pipeline End-to-End

Tag a `v0.2.1` patch release to exercise the full pipeline with
`EFFIGY_TAP_GH_TOKEN` wired up.

Validation checklist:
- [ ] Gates pass (format, clippy, nextest, doc tests, smoke)
- [ ] Three binaries built (linux-x86_64, macos-arm64, macos-x86_64)
- [ ] GitHub Release created with CHANGELOG-sourced notes
- [ ] Homebrew tap formula auto-updated in `inflatable-cookie/homebrew-tap`
- [ ] `brew upgrade effigy` installs the new version

This requires at least one user-facing change to justify the tag. Candidates:
- Any pending feature work or bug fix
- CHANGELOG.md entry under `[Unreleased]`

## 2) Consumer Repo CI Integration

Wire Effigy into the first consumer repo CI pipeline.

Install snippet (from doc 049 section 5):

```yaml
- name: Install Effigy
  run: |
    EFFIGY_VERSION="0.2.1"
    curl -fsSL "https://github.com/inflatable-cookie/effigy/releases/download/v${EFFIGY_VERSION}/effigy-$(uname -m | sed 's/arm64/aarch64/')-unknown-linux-gnu" \
      -o /usr/local/bin/effigy
    chmod +x /usr/local/bin/effigy
    effigy help
```

Tasks:
- [ ] Add install step to consumer repo CI workflow
- [ ] Pin to specific version (not `latest`)
- [ ] Validate `effigy tasks` and `effigy test` run in consumer CI
- [ ] Document the pattern in the consumer repo

## 3) JSON Contracts CI Enforcement

The `json-contracts.yml` workflow is now active. Validate it works on real PRs.

Tasks:
- [ ] Confirm the workflow passes on current main
- [ ] Open a test PR that touches JSON output and verify the check runs
- [ ] Review whether any script paths need updating post-.github-bak cleanup

## 4) Linux Homebrew Formula Extension

Extend the Homebrew formula to support Linux via Linuxbrew.

Formula addition:
```ruby
on_linux do
  if Hardware::CPU.intel?
    url "<release-url>/effigy-x86_64-unknown-linux-gnu"
    sha256 "<linux-x86_64-hash>"
  end
end
```

Tasks:
- [ ] Add `on_linux` block to formula template in `release-binaries.yml`
- [ ] Compute and include Linux binary SHA256 in homebrew job
- [ ] Test `brew install` on a Linux environment (CI or local)

## 5) `setup-effigy` GitHub Action

Published as [`inflatable-cookie/setup-effigy@v1`](https://github.com/inflatable-cookie/setup-effigy).

- [x] Create `inflatable-cookie/setup-effigy` action repo
- [x] Support `version` input with download + cache
- [x] Publish as `v1`
- [x] Update doc 049 sections 5a and 9

## 6) ARM Linux Target

Added `aarch64-unknown-linux-gnu` to the build matrix for ARM Linux coverage
(AWS Graviton, Docker on Apple Silicon).

- [x] Add cross-compilation target to build matrix (via `taiki-e/setup-cross-toolchain-action`)
- [x] Smoke test runs via QEMU binfmt_misc on x86_64 runner
- [x] Add to Homebrew formula `on_linux` block with ARM64 entry

## Completion Criteria

This roadmap is complete when:
1. A tagged release has successfully auto-updated the Homebrew tap formula.
2. At least one consumer repo CI pipeline installs and runs Effigy from
   GitHub Releases.
3. JSON contracts workflow is validated on a real PR.

Sections 4-6 are stretch goals that can roll into a follow-up milestone.

## Closeout Note (2026-04-17)

This roadmap is complete for Effigy's actual release posture.

The originally-open proof points are now satisfied strongly enough by real use:

- tagged Homebrew installs have been in routine use for multiple versions
- Effigy has been running in CI across multiple consumer repos for some time
- the release/install path is no longer hypothetical and has moved into normal
  operation rather than one-off validation

The remaining release-system work now belongs in roadmap `027`, where the
built-in release orchestration itself still needs its final live-release
closeout.
