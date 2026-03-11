# Release Self-Hosting Baseline Config

Status: complete
Created: 2026-03-11
Roadmap: g01.027
Batch: release-self-hosting-baseline-config

## Summary

- Added a real `[release]` section to Effigy’s root `effigy.toml`.
- Mirrored the current local release-gate script baseline as release gates.
- Switched `qa:release` to self-host through `effigy release gates --repo .`.

## Changes

- Added root release config for `Cargo.toml`, `CHANGELOG.md`, `v{version}` tag
  formatting, and baseline gate commands for format, test, QA, build, smoke,
  and distribution metadata validation.
- Updated the `qa:release` task to bootstrap the built-in release gate runner
  via `cargo run --bin effigy -- release gates --repo .`.
- Added a contract test that loads the current repo release config, asserts the
  expected gate set, and verifies those commands stay aligned with
  `scripts/check-release-gates.sh`.
- Updated README, AGENTS, the release protocol guide, roadmap progress, and
  changelog notes to reflect the self-hosting baseline.

## Vision Target Delta

- Primary tags: `RELEASE`, `SELFHOST`, `OPERATE`
- Movement: baseline `Effigy had release orchestration commands but its own repo still depended on separate task wiring and helper scripts as the primary local gate entrypoint` -> current `Effigy’s own manifest now declares release config and its local release QA path routes through the built-in release gate runner`
- Remaining gap: `Cargo.lock sync during prepare, old/new parallel validation, tag-install validation migration, and workflow/checklist updates remain open`

## Validation Performed

- command: `cargo test --lib current_repo_release_config_matches_self_hosting_gate_script_baseline -- --nocapture`
  - result: pass
- command: `cargo fmt --all -- --check`
  - result: pass
- command: `git diff --check`
  - result: pass

## Risks

- The self-hosted gate surface currently mirrors the local no-tag release-gate
  path only. The optional tag-install validation branch still lives in
  `scripts/check-release-gates.sh`.
- `release.sync-files` is still not wired to regenerate `Cargo.lock`, so the
  built-in prepare flow is not yet a complete replacement for
  `prepare-release.sh --apply` in this repo.

## Next Task

- Implement the next meaningful `g01.027` batch by adding real Cargo.lock sync
  support for Cargo-based release preparation, then validate that Effigy’s
  built-in prepare flow and `scripts/prepare-release.sh --apply` produce the
  same version/changelog/lockfile results on the same fixture.
