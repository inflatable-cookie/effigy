# Env Schema Roadmap Reconciliation

Status: complete
Created: 2026-03-10
Roadmap: g01.025
Batch: roadmap-reconciliation

## Summary

- Added the missing resolver timeout unit test so exec-timeout behavior is explicitly proven.
- Reconciled the roadmap wording and checklist state to match Effigy's shipped env-schema implementation rather than the original module-path and implementation sketch.
- Reduced the remaining open roadmap items to the two genuinely-unfinished security audit tasks.

## Changes

- Added `resolve_exec_timeout` coverage in `src/env_schema/resolver/tests.rs`.
- Updated parser, resolver, module-integration, configuration, and test sections in `docs/roadmaps/g01/025-varlock-env-spec-integration.md` to reflect:
  - actual module paths (`src/env_schema/*`, `src/env_schema.rs`)
  - shipped parser/resolver behaviors
  - the real `[env_schema]` manifest section naming
  - completed test coverage for resolver exec behavior and secret-handling basics

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`
- Movement: baseline `the roadmap still described several already-shipped env-schema capabilities as open because the original sketch no longer matched the codebase` -> current `the roadmap now isolates the true remaining work instead of burying it under stale checklist items`
- Remaining gap: `only the strict drop-time zeroization proof and the broader secret-output audit remain open`

## Validation Performed

- command: `cargo fmt --all -- --check`
  - result: pass
- command: `cargo test env_schema::resolver::tests -- --nocapture`
  - result: pass
- command: `git diff --check`
  - result: pass

## Risks

- The roadmap now accurately tracks the shipped surface, but it intentionally does not claim the unsafe post-drop memory inspection target is satisfied.
- The remaining secret-output audit still needs a deliberate sweep if new env-schema diagnostics or output modes are added later.

## Next Task

- Implement the final meaningful `g01.025` security-closeout batch: audit env-schema-related output/error surfaces for secret leakage, add any missing regression coverage around those surfaces, and then decide whether the remaining drop-time zeroization item should be completed with a stronger unsafe test or explicitly deferred as intentionally out of scope.
