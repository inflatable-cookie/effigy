# Deferral Fallback Phase 2.1 Checkpoint

Date: 2026-02-26
Owner: Platform
Related roadmap: 002 - Deferral Fallback System

## Scope
- Implement unresolved-task deferral behavior.
- Add safety guardrails for recursive deferral.
- Add tests covering primary deferral paths.

## Changes
- Added manifest support for:
  - `[defer]`
  - `run = "..."`
- Added deferral trigger for unresolved-task resolution errors.
- Added interpolation support in deferral command:
  - `{request}`
  - `{args}`
  - `{repo}`
- Added recursion guard via `EFFIGY_DEFER_DEPTH`.
- Added verbose deferral trace rendering.

## Validation
- command: `cargo test`
  - result: pass, includes new deferral tests.
- command: `cargo run --bin effigy -- repo-pulse --repo /Users/betterthanclay/Dev/projects/acowtancy`
  - result: pass.

## Risks / Follow-ups
- Deferral cookbook for PHP bridge is not yet documented in guides.

## Next
- Add a migration cookbook entry showing fallback to `composer global run effigy`.
