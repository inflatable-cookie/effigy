# Effigy v0.13.1 Release Note

Date: 2026-09-25
Owner: Effigy maintainers
Related roadmap: g10.017 — Opt-in Chromium workspace runtime
Release: 0.13.1

## Summary

Effigy 0.13.1 adds an optional Chromium system-library layer to the built-in
`workspace-rust-bun` container service. The default image stays unchanged.
The compiled catalog now tracks the published catalog-pack `v1.1.0`. The full
change list is in the [0.13.1 changelog](../../../CHANGELOG.md#0131---2026-09-25).

## User-Visible Changes

- Set `browser_runtime = "chromium"` on a `workspace-rust-bun` service to install
  Debian Bookworm Chromium runtime libraries and basic fonts at image build.
  The default is `"none"`; an unsupported value fails the build.
- The generated catalog snapshot and lock now identify the published
  `effigy-catalog-pack` `v1.1.0` artifact by digest.
- The image supplies system libraries only. Each consumer installs its own
  Playwright package and matching Chromium revision as the workspace user.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`.
- Movement: the shared workspace image now offers a tested Chromium dependency
  layer behind an explicit parameter, with published catalog provenance and a
  supporting Effigy release.
- Remaining gap: expose the option with a 0.13.1 floor in the
  Underlay bundle, then rerun Acowtancy's browser evidence in its rebuilt
  workspace.

## Migration Notes

- Existing services need no change. To opt in, set
  `browser_runtime = "chromium"` on the `workspace-rust-bun` service and rebuild
  its image. Install the repository-pinned Playwright browser as the workspace
  user after rebuild.
- The Underlay bundle does not expose this input yet. Its version-gated input
  change is a separate follow-up after the Effigy release.

## Validation

- Exact source commit `8d4771d6161ff6bda77697e9a5c99713e35992fb`:
  [manual `ci.yml` run 36129308284](https://github.com/inflatable-cookie/effigy/actions/runs/36129308284)
  passed supply-chain, Linux and macOS tests, lint, and released-surface jobs.
- `effigy release prepare --yes --check-gates` passed all seven configured
  gates against the 0.13.1 candidate files and wrote prepared state.
- The g10.017 [linux-arm64 smoke](./25-101900-g10-017-chromium-workspace-runtime.md)
  built default and opt-in images. As non-root `dev`, Playwright 1.55.1's
  Chromium build 1193 had no missing shared libraries, launched headless,
  rendered local HTML, and closed cleanly.

## Rollback Notes

- `v0.13.0` is the previous known-good Effigy release for CI pinning if this
  release causes a regression. Do not move or reuse a published tag; a
  post-tag fix requires a new patch release.
- Rebuild with `browser_runtime = "none"` to omit the optional library layer.
  No browser binary or Playwright package is stored in the shared image.

## Compatibility

- Existing `workspace-rust-bun` manifests retain the default `"none"` image.
- Catalog-pack update support still begins at Effigy 0.13.0; the 0.13.1
  support policy includes both 0.13.0 and 0.13.1.
- The browser layer was proved on Debian Bookworm linux-arm64 with Playwright
  1.55.1. Consumers own the browser revision and any project-specific tests.

## Next

Gate and publish the Underlay bundle input at Effigy 0.13.1, then resume
Acowtancy's browser proof.
