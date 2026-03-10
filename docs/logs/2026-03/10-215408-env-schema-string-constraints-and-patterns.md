# Env Schema String Constraints And Patterns

Status: complete
Created: 2026-03-10
Roadmap: g01.025
Batch: validation-constraints

## Summary

- Added parser support for env-schema string constraint annotations: `@min`, `@max`, and `@pattern`.
- Refactored env-schema validation onto explicit validator types so string-length and regex checks compose with existing port/url/enum validation.
- Added focused parser, validator, and task-runtime regressions proving invalid schema values fail before task execution.

## Changes

- Extended env-schema annotations in `src/env_schema/ast.rs` and `src/env_schema/parser.rs` to carry regex patterns and finalize string length constraints.
- Added parser-side validation for duplicate annotations, non-numeric string-length values, invalid `@pattern` regexes, and impossible `@min`/`@max` combinations.
- Refactored `src/env_schema/validator.rs` around a shared `Validator` trait with concrete validators for port, URL, enum, string, and regex pattern checks.
- Added parser/validator coverage plus an end-to-end runtime regression in `src/tests/runner_tests/runner_core_tests/task_env_tests.rs`.
- Updated the env-schema guide, changelog, and roadmap validator checklist state.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`
- Movement: baseline `env-schema validation handled basic types but string constraints and regex contracts were not expressible from schema syntax` -> current `schema authors can declare length and regex constraints, and Effigy rejects invalid resolved values before launching tasks`
- Remaining gap: `remaining parser/integration checklist items are mostly coverage and public-API cleanup, not missing runtime validation behavior`

## Validation Performed

- command: `cargo fmt --all -- --check`
  - result: pass
- command: `cargo test env_schema::parser::tests -- --nocapture`
  - result: pass
- command: `cargo test env_schema::validator::tests -- --nocapture`
  - result: pass
- command: `cargo test run_manifest_task_env_schema_pattern_validation_blocks_execution -- --nocapture`
  - result: pass
- command: `git diff --check`
  - result: pass

## Risks

- Annotation token parsing is still whitespace-split, so `@pattern` values with literal spaces are not supported unless expressed without spaces in the token.
- Regex patterns are validated at parse time and recompiled during validation; this is correct for now, but not yet optimized for repeated validation passes.

## Next Task

- Implement the next `g01.025` parser/completion batch: add RFC-style `.env.schema` example fixtures plus UTF-8 and edge-case integration coverage, then tighten the remaining parser checklist state around those proven contracts.
