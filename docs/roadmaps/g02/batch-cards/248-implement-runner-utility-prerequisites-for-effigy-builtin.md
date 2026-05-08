# 248 Implement Runner-Utility Prerequisites For Effigy-Builtin

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Relocate the small runner-side utilities that `src/runner/builtin/**`
reaches into today, plus invert one app-surface callback, so the
eventual `effigy-builtin` extraction (future card) is a mechanical move.
This card does all the motion entirely inside the runner + existing
crates. No new crate is introduced.

This card is independent of card `247` (decide scan extraction) — neither
blocks the other. It is a prerequisite for the eventual
`effigy-builtin` implement card (not yet drafted, tentatively `250`+),
alongside cards `247` (decide) and `249` (implement effigy-scan).

## Context

Card `244`'s decision section recorded the runner-side utilities that
builtin currently reaches into, discovered via a function-level coupling
sweep:

| Symbol | Current path | Builtin caller(s) |
|---|---|---|
| `shell_quote` | `runner::util` | test execution, test render/planning |
| `parse_dotenv_entries` | `runner::util` | test cargo-env resolution |
| `normalize_builtin_test_suite` | `runner::util` | test planning/resolve |
| `vitest_command_for_js_package_manager` | `runner::tooling` | test planning/resolve |
| `BUILTIN_TASKS` | `runner::model::constants` | completion scripts/command_index |
| `DEFAULT_BUILTIN_TEST_MAX_PARALLEL` | `runner::model::constants` | test planning/config |
| `TASK_MANIFEST_FILE` | `runner::model::constants` | init, init/output, migrate/plan |
| `parse_json` / `parse_toml` / `read_utf8` | `crate::data_loading` | migrate/io |
| `encode_json` | `runner::render` | builtin/scan/execution/core |
| `deferred_builtins_for_root` | `crate::runner` | builtin/support |
| `detect_test_runner_plans` | `crate::testing` | test planning/resolve/plan_resolution |

All of these are small (single-function or small-module) and have no
cross-crate consumers beyond builtin + (in two cases) runner-internal
code. The relocation targets are already-extracted crates.

Scope (see `244` §3 and §8 for sweep detail):

- `shell_quote`, `parse_dotenv_entries`, `normalize_builtin_test_suite`
  belong in `effigy-tasks` (task-facing helpers) or `effigy-core`
  (generic shell utility). Pick per symbol — dotenv belongs alongside
  `effigy-env`'s dotenv parsing or in `effigy-tasks`; the shell quoter
  is likely `effigy-core`; the builtin-test-suite normalizer belongs in
  `effigy-tasks` since it touches task-facing strings.
- `vitest_command_for_js_package_manager` is JS-toolchain wiring; belongs
  in `effigy-tasks` (it parameterizes test runners) unless it naturally
  pairs with another toolchain helper elsewhere.
- `BUILTIN_TASKS` + `DEFAULT_BUILTIN_TEST_MAX_PARALLEL` — both are
  builtin-specific constants. Options: (a) inline into the future
  `effigy-builtin` crate as crate-private constants; (b) relocate into
  `effigy-tasks`; (c) leave as runner constants consumed by builtin via
  a short-lived import. Decision lives in this card.
- `TASK_MANIFEST_FILE` — already used by `effigy-routing` post-`246`
  (owned copy inside the routing crate) and by runner/scan. Consumed
  here by three builtin files. Either inline into the future
  `effigy-builtin` crate (following routing's precedent) or move to
  `effigy-manifest` (data-layer constant) so every consumer imports one
  copy.
- `parse_json` / `parse_toml` / `read_utf8` live in `src/data_loading.rs`
  (main crate). They are thin wrappers. Move to `effigy-core` (filesystem
  / serialization helpers) so `effigy-builtin` and any other crate can
  reuse them.
- `encode_json` lives in `runner::render`. One builtin caller
  (builtin/scan/execution/core/mod.rs). Move to `effigy-ui` or
  `effigy-core` — prefer `effigy-ui` if JSON encoding is a rendering
  concern; otherwise `effigy-core` alongside other format helpers.
- `deferred_builtins_for_root` lives in `src/runner/mod.rs` and is
  called from `builtin/support.rs`. Invert so `builtin/support` takes the
  deferred-builtins list as a parameter at the call-site boundary. The
  runner computes it and passes it in.
- `detect_test_runner_plans` lives at `src/testing.rs` (main crate
  surface). Called from `builtin/test/planning/resolve/plan_resolution.rs`.
  Either:
  - (a) relocate into `effigy-tasks` (it is test-plan-resolution logic), or
  - (b) invert so builtin test planning takes a detector callback at the
    call boundary.

  Prefer (a) if the function is pure-logic; (b) if it reaches into
  app-specific state. This card makes the determination.

## In Scope

Make the following changes inside the runner and existing crates.
No new crate. No code moves into `effigy-builtin` (that crate does not
exist yet).

### Relocations

- Move `shell_quote`, `parse_dotenv_entries`, `normalize_builtin_test_suite`
  from `src/runner/util` into the chosen destination crate(s). Update
  all call sites.
- Move `vitest_command_for_js_package_manager` from `src/runner/tooling`
  into the chosen destination crate. Update all call sites.
- Move `parse_json` / `parse_toml` / `read_utf8` from `src/data_loading.rs`
  into `effigy-core` (or equivalent). Update all call sites, including
  runner-internal callers that are not builtin.
- Move `encode_json` from `src/runner/render` into the chosen destination
  crate. Update all call sites.
- Decide `BUILTIN_TASKS`, `DEFAULT_BUILTIN_TEST_MAX_PARALLEL`,
  `TASK_MANIFEST_FILE` per the options above. Apply the chosen
  relocation (or document the decision to inline at extraction time and
  leave them in `runner::model::constants` until then).

### Inversions

- Invert `deferred_builtins_for_root` consumption:
  `builtin/support.rs` accepts the deferred-builtins list as an argument;
  the runner call site (wherever help / support is invoked) computes it
  and passes it in.
- Resolve `detect_test_runner_plans`:
  - if relocating: move to `effigy-tasks` and update the import in
    `plan_resolution.rs`;
  - if inverting: thread a detector callback through the test-planning
    call sites.

## Out Of Scope

- Introducing `effigy-builtin` — that is the future `250`+ implement card.
- Any scan-extraction work — that is cards `247` / `249`.
- Reshaping `RunnerError` or introducing `BuiltinError`
  (the error boundary belongs to the `effigy-builtin` crate move, not a
  prereq inside the runner).
- Touching `runner::locking`, `runner::deferral`, `runner::command_context`.
- Any `builtin/scan/**` changes beyond what the constant/JSON-encoder
  relocations strictly require.

## Acceptance Criteria

- Each symbol listed above has a recorded destination (or is explicitly
  deferred in the commit message with justification).
- `src/runner/util`, `src/runner/tooling`, `src/data_loading.rs`,
  `src/runner/render`, `src/runner/model/constants.rs`, and
  `src/runner/mod.rs` each shed the relocated / inverted surface.
- `src/runner/builtin/**` continues to compile without reaching into
  `crate::runner::deferred_builtins_for_root` (callback inversion
  lands).
- `crate::testing::detect_test_runner_plans` is resolved — either
  relocated or inverted — and `builtin/test/planning/resolve/plan_resolution.rs`
  no longer imports it under that path.
- No new crate created. Each relocation lands in an existing workspace
  crate.
- Test totals unchanged: 683 runner lib + 16 effigy-managed + 89
  effigy-env (plus any unit tests that travel with relocated helpers
  accounted for in the commit message).

## Validation

- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Landed Decisions

### Relocations applied

- `parse_json` / `parse_toml` / `read_utf8` → `effigy-core::data_loading`
  (new module). Deleted `src/data_loading.rs`. Two runner callers
  (`builtin/migrate/io`, `doctor/manifest/scan`) flipped to
  `effigy_core::data_loading::*`. `effigy-core` gained `serde`,
  `serde_json`, and `toml` deps — cost-neutral since every downstream
  already transitively required them.
- `TASK_MANIFEST_FILE` → `effigy-manifest` as a `pub const`. Removed from
  `runner::model::constants`. Ten call sites (builtin init, migrate,
  doctor, runner mod, error display, scan loading) flipped to
  `effigy_manifest::TASK_MANIFEST_FILE`. Historical inlined copies in
  `effigy-bootstrap`, `effigy-release`, and `effigy-routing` are left in
  place with a doc-comment pointer to the canonical constant; they will
  migrate when they next touch adjacent code.
- `vitest_command_for_js_package_manager` and
  `js_package_manager_binary` → methods `vitest_command()` and
  `binary_name()` on `ManifestJsPackageManager` inside `effigy-manifest`.
  Pivoted away from `effigy-tasks` because `effigy-manifest` already
  depends on `effigy-tasks` (dep cycle). Method placement keeps behavior
  colocated with the enum and saves the two existing callers from an
  extra import.
- `src/testing/**` (detect_test_runner\*, TestRunner, TestRunnerPlan,
  TestRunnerCandidate, TestRunnerDetection) → `effigy-tasks::testing`.
  `effigy-tasks` gained `serde_json` dep (existing deep dep chain
  required it anyway). The two runner callers flipped to
  `effigy_tasks::testing::*`. Integration tests at
  `src/tests/testing_tests.rs` stay in the main crate (they rely on
  `contract_test_support`) but now import symbols from `effigy_tasks`.
- `shell_quote` → direct `effigy_core::shell::shell_quote` at every
  builtin call site (5 files). One non-builtin caller
  (`runner/deferral/run.rs`) also flipped. The runner-internal
  `src/runner/util/shell.rs` shim now re-exports only
  `with_local_node_bin_path` (still used by three non-builtin runner
  modules).
- `parse_dotenv_entries` → direct `effigy_env::dotenv::parse_dotenv_entries`
  at the one builtin call site. Deleted the `src/runner/util/dotenv.rs`
  shim entirely.
- `normalize_builtin_test_suite` → direct
  `effigy_tasks::normalize_builtin_test_suite` at the two call sites
  (builtin suite-selection, builtin test target config). Removed the
  stray duplicate implementation in `src/runner/util/parsing/runtime.rs`
  and the `runner::util::parsing::normalize_builtin_test_suite` shim.

### Deferred with justification

- `BUILTIN_TASKS` and `DEFAULT_BUILTIN_TEST_MAX_PARALLEL` stay in
  `runner::model::constants`. They are builtin-specific values with
  exactly one consumer each outside builtin. Moving them now would
  spread indirection; inlining into `effigy-builtin` at extraction time
  is the cleanest path. Noted in the commit message.
- `encode_json` stays in `runner::render`. The single builtin caller
  (`builtin/scan/execution/core`) can keep reaching for it; the roughly
  fourteen non-builtin runner callers would each need a `.map_err`
  shim if we relocated. Decision: move at `effigy-builtin` extraction
  time, when we know whether the builtin-side errors want to be
  `BuiltinError::Render(...)` or continue to surface as
  `RunnerError::Ui`.

### Inversions applied

- `deferred_builtins_for_root` no longer called from
  `builtin/support.rs`. The function `render_builtin_general_help_for_root`
  became `render_builtin_general_help(deferred_builtins, output_json)`
  with the set passed in. `run_builtin_help` gained a
  `deferred_builtins: &BTreeSet<String>` parameter. The registry's
  `Help` arm computes `deferred_builtins_from_catalogs(catalogs,
  target_root)` using already-loaded catalogs (no fresh filesystem
  discovery) and passes it in.
- `detect_test_runner_plans` relocation (option a) over callback
  inversion: the function is pure data-flow (repo root in, plan list
  out), `effigy-tasks` is already an available crate, and adding a
  callback would have cost more indirection than the move.

### Test totals after landing

- `cargo test` summary: 1 + 0 + 681 + 0 + 0 + 4 + 10 + 190 + 27 + 0
  (all passing). Runner lib test count drops from 683 → 681 because the
  two migrated tests (`js_package_manager_binary_*` and
  `js_package_manager_vitest_commands_*`) now live with their code in
  `effigy-manifest`; runtime-side `normalize_builtin_test_suite` had no
  dedicated test in the runner tooling suite. `effigy-managed` still
  reports 16 and `effigy-env` still reports 89 (unchanged).

## Next Task

Execute [`249-implement-effigy-scan-extraction.md`](./249-implement-effigy-scan-extraction.md).
With `247` decided and `248` complete, the prerequisites for the
eventual `effigy-builtin` implement card are both done — only `249`
remains before drafting it.
