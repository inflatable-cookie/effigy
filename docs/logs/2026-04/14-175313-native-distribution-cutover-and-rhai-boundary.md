# Native Distribution Cutover and Rhai Boundary

Date: 2026-04-14
Owner: Codex
Related roadmap: `g02.004`

## Summary

- Retired `scripts/check-distribution-first-publish.sh`.
- Added native `effigy distribution check-glibc-floor`.
- Added native `effigy distribution first-publish`.
- Cut preflight, closeout, help, tests, and operator docs over to the native
  distribution surface.
- Left `scripts/check-linux-glibc-floor.sh` in place only because the release
  workflow still invokes it directly and workflow edits require explicit human
  approval.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`, `RELEASE`
- Movement: `Rhai-backed internal dogfooding with one remaining first-publish shell wrapper` -> `native Effigy distribution orchestration for first-publish and glibc-floor checks`
- Remaining gap: workflow-bound glibc guard cutover and the external pilot pause/reopen decision

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Notes

- The native distribution cutover keeps the scripting/orchestration contract
  inside Effigy rather than moving the last meaningful release flow through
  shell for compatibility.
- `scripts/effigy-dev` remains intentionally out of scope as the bootstrap/dev
  entrypoint boundary.

## Next

- Execute `098-decide-post-native-distribution-rhai-boundary` to decide
  whether the lane now pauses cleanly or whether the workflow-bound glibc guard
  earns one final internal batch.
