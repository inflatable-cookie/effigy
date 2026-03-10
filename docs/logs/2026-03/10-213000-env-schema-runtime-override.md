# Env Schema Runtime Override

Status: complete
Created: 2026-03-10
Roadmap: g01.025
Batch: runtime-override

## Summary

- Added task-runtime `--env-schema <PATH>` support for standard task execution.
- Made the runtime prefer the explicit schema path over manifest/default schema discovery.
- Added regression coverage for parsing, task execution, built-in rejection, and help text.

## Changes

- Extended task runtime argument parsing to capture `--env-schema <PATH>`.
- Threaded the override through execution preflight into standard task env-schema resolution.
- Resolved override paths relative to the selected catalog root, with absolute paths preserved.
- Rejected `--env-schema` on built-in commands so the flag is not silently ignored.
- Updated the env-schema guide, general help text, changelog, and roadmap runtime checklist state.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`
- Movement: baseline `env-schema existed but task execution could not override schema selection per run` -> current `standard task execution supports explicit schema-path override with verified runtime/error behavior`
- Remaining gap: `resolved env values are not yet available for internal conditional logic`

## Validation Performed

- command: `cargo fmt --all -- --check`
  - result: pass
- command: `cargo test run_manifest_task_env_schema_override -- --nocapture`
  - result: pass
- command: `cargo test builtin_argument_contract_matrix_is_stable -- --nocapture`
  - result: pass
- command: `cargo test render_help_writes_structured_sections -- --nocapture`
  - result: pass
- command: `cargo test parse_task_runtime_args -- --nocapture`
  - result: pass

## Risks

- `--env-schema` is currently limited to standard task execution; built-in task flows still reject it explicitly.
- The override only swaps the schema file path; `.env` override loading still comes from the catalog root.

## Next Task

- Implement the next `g01.025` runtime batch: expose resolved env-schema values to internal conditional logic and extend end-to-end coverage around that internal availability contract.
