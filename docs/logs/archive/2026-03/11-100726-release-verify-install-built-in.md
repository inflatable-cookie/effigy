# Release Verify Install Built-In

Status: complete
Created: 2026-03-11
Roadmap: g01.027
Batch: release-verify-install-built-in

## Summary

- Added `effigy release verify-install` as a built-in release verification path.
- Migrated the legacy tag-install helper script to a compatibility wrapper that
- delegates to the built-in command.
- Closed the remaining tag-install validation gap between the release shell
  helpers and the built-in release command surface.

## Changes

- Added `release verify-install` to the CLI parser, release help text, and
  release runtime dispatch.
- Implemented built-in tag-install verification by installing the tagged binary
  from a git URL into a temporary root, then running a fixed set of installed
  binary checks against a generated fixture repo.
- Added text and JSON result contracts for install verification, including
  fail-fast behavior and per-step timing/output metadata.
- Replaced the body of `scripts/check-release-install-from-tag.sh` with a
  compatibility wrapper that delegates to `cargo run --bin effigy -- release
  verify-install`.
- Added CLI coverage for successful local tagged-install verification against a
  tiny git fixture and for fail-fast install-step failure behavior.

## Vision Target Delta

- Primary tags: `RELEASE`, `VERIFY`, `SELFHOST`
- Movement: baseline `tag-based install validation still lived only in a shell helper outside the built-in release command surface` -> current `Effigy now provides a first-class built-in install verification command and the legacy helper delegates to it`
- Remaining gap: `broader real-repo old/new parallel validation, release checklist updates, and helper-script retirement remain open`

## Validation Performed

- command: `cargo test --lib parse_release_ -- --nocapture`
  - result: pass
- command: `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
  - result: pass
- command: `cargo test --lib current_repo_release_config_matches_self_hosting_gate_script_baseline -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_verify_install_ -- --nocapture`
  - result: pass

## Risks

- The built-in verify-install surface currently checks the fixed Effigy-style
  installed-binary contract rather than a manifest-configurable verification
  matrix. If cross-project adoption needs per-repo install checks later, that
  should be added intentionally rather than overloading the current contract.
- Install verification still depends on `cargo install --git`, so failures can
  be slow relative to the rest of the release command set.

## Next Task

- Implement the next meaningful `g01.027` batch by adding broader old/new
  migration validation for Effigy’s real repo, so `check-release-gates.sh`,
  `check-release-install-from-tag.sh`, and the built-in release surfaces can be
  compared systematically before helper-script retirement.
