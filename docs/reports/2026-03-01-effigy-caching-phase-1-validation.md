# 2026-03-01 - Effigy Caching Phase 1 Validation

## Scope

Validate phase-1 caching contract implementation:
- explicit task opt-in (`[tasks.<name>.cache].enabled = true`)
- deterministic fingerprint from declared inputs + command + selected env + declared outputs metadata
- skip only when fingerprint matches and declared outputs exist
- explicit cache inspection and invalidation command paths

## Commands Executed

- `cargo test cache_tests:: -- --test-threads=1`
- `cargo test render_help_writes_structured_sections -- --test-threads=1`
- `cargo test run_manifest_task_builtin_config_prints_reference -- --test-threads=1`

All commands exited successfully.

## Evidence

### Cache hit path

Validated by:
- `runner::cache_tests::task_cache_hit_skips_unchanged_rerun`

Observed behavior:
- First run executes task and writes output marker.
- Second unchanged run returns cache-hit skip output.
- Marker file remains single-write (`run`), confirming execution was skipped.

### Invalidation: input change

Validated by:
- `runner::cache_tests::task_cache_invalidates_on_input_change`

Observed behavior:
- Updating declared input (`input.txt`) forces re-run.
- Marker file transitions from `run` to `runrun`.

### Invalidation: selected env change

Validated by:
- `runner::cache_tests::task_cache_invalidates_on_selected_env_change`

Observed behavior:
- Changing declared env key (`EFFIGY_CACHE_TEST_TOKEN`) forces re-run.
- Marker file transitions from `run` to `runrun`.

### Invalidation: command change

Validated by:
- `runner::cache_tests::task_cache_invalidates_on_command_change`

Observed behavior:
- Mutating command from `printf one` to `printf two` forces re-run.
- Marker file captures both command outputs (`onetwo`).

### Invalidation: missing declared output

Validated by:
- `runner::cache_tests::task_cache_invalidates_when_declared_output_is_missing`

Observed behavior:
- Removing declared output (`out/result.txt`) prevents cache reuse.
- Follow-up run executes and marker transitions from `run` to `runrun`.

### Safety: non-opt-in task behavior unchanged

Validated by:
- `runner::cache_tests::non_opt_in_task_always_executes`

Observed behavior:
- Task without `[tasks.<name>.cache]` executes on each run.
- Marker file shows two executions (`runrun`).

### Explicit inspection and invalidation paths

Validated by:
- `runner::cache_tests::cache_builtin_inspect_and_invalidate_paths_are_available`

Observed behavior:
- `effigy cache inspect build` reports present entry.
- `effigy cache invalidate build` removes entry.
- Re-inspect reports missing entry.

## Notes

- Phase-1 contract guardrails held in this validation slice:
  - no implicit input/output discovery used in tests
  - no cache reuse with missing declared outputs
  - non-opt-in task behavior unchanged
- Coverage here is local-cache only and intentionally excludes remote/distributed cache concerns.
