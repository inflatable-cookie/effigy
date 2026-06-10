# Release Execute Yes Commit Tag Push

Status: complete
Created: 2026-03-11
Roadmap: g01.027
Batch: release-execute-yes-commit-tag-push

## Summary

- Added the first irreversible `effigy release execute` path.
- Execution now moves from prepared-state validation into real git commit, tag,
  and push behavior when the operator passes `--yes`.
- Cleanup now happens only after the full execute flow succeeds.

## Changes

- Added `effigy release execute --yes` to the CLI, help text, and release
  command dispatch.
- Implemented non-interactive execute approval with explicit rejection of
  `--plan` + `--yes` combinations.
- Added git branch and `origin` remote checks, local tag collision detection,
  release commit creation, tag creation, and branch/tag push behavior.
- Added execute result payloads that report partial side effects on failure,
  including whether the commit or tag was already created locally.
- Added post-release monitoring instructions on success and state-file cleanup
  only after the full execute flow completes.
- Enforced the no-retag invariant by blocking retries once a failed push has
  already created the local release tag.

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Movement: baseline `Effigy could validate execute readiness but could not perform the release step` -> current `Effigy can now execute a prepared release end to end in non-interactive mode with guarded cleanup and explicit push-failure handling`
- Remaining gap: `interactive execute approval prompts, simulate flow, gate timing/reporting, and self-hosting migration remain open`

## Validation Performed

- command: `cargo test --lib parse_release_ -- --nocapture`
  - result: pass
- command: `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_execute_ -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_prepare_ -- --nocapture`
  - result: pass

## Risks

- Execute currently assumes the standard `origin` remote name and a checked-out
  branch; richer remote/branch targeting can be added later if needed.
- Push happens as two commands (branch, then tag), so a remote-side failure can
  still leave the local repo ahead of the remote even though Effigy reports the
  partial state clearly.
- Interactive approval prompts are still absent; the shipped safety boundary is
  explicit non-interactive approval via `--yes`.

## Next Task

- Implement the next meaningful `g01.027` batch by adding `release simulate`
  and/or standalone gate execution so the remaining roadmap work shifts from
  core execution mechanics to safer previews, richer operator feedback, and
  self-hosting migration onto Effigy’s own release process.
