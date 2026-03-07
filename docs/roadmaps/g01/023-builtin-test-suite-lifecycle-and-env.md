# 023 - Builtin Test Suite Lifecycle and Environment

Generation: `g01`

Status: Planned
Owner: Platform
Created: 2026-03-07
Depends on: 005, 007

## Vision Alignment

This roadmap closes the gap between Effigy's builtin test routing and the managed task/runtime model. The target is to keep `effigy test` on first-class runners like `cargo nextest run` while giving projects a declarative way to attach setup, teardown, and environment policy without falling back to bespoke wrapper scripts.

## Primary Tags

- `TEST`
- `CONTRACT`
- `OPERATE`

## Target Envelope

Builtin `effigy test` suites can declare setup, teardown, env, and env-file policy directly in `effigy.toml`. Projects with DB-backed or lifecycle-managed test runs can use builtin `effigy test` instead of custom script wrappers, while still preserving deterministic teardown-on-failure behavior.

## Vision Target Delta

Move from command-only builtin test suites plus repo-local wrapper scripts to a single declarative test-suite contract that can orchestrate nextest and other runners with managed lifecycle semantics.

## 1) Problem

Effigy's builtin `test` command already solves runner detection and selection well:

- Rust defaults to `cargo nextest run` when available.
- Mixed-suite repositories can target explicit suites deterministically.
- `--plan` explains the selected runner and command.

However, builtin test suites are currently limited to command strings and a narrow cargo-env bridge:

- `[test.suites.<name>]` only supports `run`
- builtin test env injection only reuses top-level `CARGO_*` values
- builtin test has no `env_file`
- builtin test has no setup hook
- builtin test has no teardown hook
- builtin test has no guaranteed cleanup-after-failure policy

This forces repositories with managed test lifecycles to bypass builtin `effigy test` entirely and define wrapper tasks or scripts. Farmyard is a concrete example:

- reset test DB
- migrate test DB
- inject `TEST_DATABASE_URL`, `DATABASE_URL`, and test-thread policy
- run tests
- always reset the test DB afterward, even when tests fail

That wrapper works, but it means:

- builtin `effigy test` and explicit managed test flows diverge
- repositories lose builtin suite selection and nextest-default behavior
- lifecycle behavior becomes hidden inside repo-local scripts
- `--plan` cannot explain the full managed test flow

## 2) Goals

- [ ] Extend builtin test suites to support declarative `env`
- [ ] Extend builtin test suites to support declarative `env_file`
- [ ] Add declarative `setup` steps for builtin test suites
- [ ] Add declarative `teardown` steps for builtin test suites
- [ ] Add explicit teardown policy with an `always` mode
- [ ] Keep builtin `effigy test` compatible with `cargo nextest run` as the actual Rust runner
- [ ] Reuse existing managed task env/env_file/step resolution instead of inventing a parallel config system
- [ ] Surface setup/teardown/env behavior in `effigy test --plan`
- [ ] Let Farmyard-style managed DB test flows migrate off custom wrapper scripts

## 3) Non-Goals

- [ ] No replacement of nextest itself as the Rust test runner
- [ ] No broad plugin system for arbitrary external test lifecycle engines
- [ ] No attempt to model every per-framework hook concept from upstream runners
- [ ] No global secret-management feature beyond existing env/env_file primitives
- [ ] No silent magic DB conventions; lifecycle remains explicit in manifest config

## 4) UX Contract

Primary command forms remain unchanged:

- `effigy test`
- `effigy test --plan`
- `effigy test <suite> ...`
- `effigy <catalog>/test ...`

Managed builtin suite example:

```toml
[env]
managed-test = [
  { TEST_DATABASE_URL = "postgres://root@127.0.0.1:5432/acowtancy-test" },
  { DATABASE_URL = "postgres://root@127.0.0.1:5432/acowtancy-test" },
  { RUST_TEST_THREADS = "1" },
]

[test.suites.managed]
run = "cargo nextest run --workspace"
env = "managed-test"
env_file = [".env", ".env.test"]
setup = [
  { run = "cargo run -p farmyard-db --bin reset_test_db" },
  { run = "cargo run -p farmyard-db --bin migrate_test_db" },
]
teardown = [
  { run = "cargo run -p farmyard-db --bin reset_test_db" },
]
teardown_policy = "always"
```

Expected behavior:

- builtin test suite selection still chooses the configured suite or auto-detected fallback
- suite env/env_file is resolved before setup and run steps
- setup runs before the suite test command
- teardown runs after the suite test command
- with `teardown_policy = "always"`, teardown still runs when setup partially succeeds or the test command fails
- `--plan` shows setup, run, teardown, env source, and teardown policy

## 5) Config Model (Target)

Current suite shape:

```toml
[test.suites]
integration = "cargo nextest run"
```

Target suite shape:

```toml
[test.suites.integration]
run = "cargo nextest run"
env = "managed-test"
env_file = [".env.test"]
setup = [
  { run = "cargo run -p app-db --bin migrate_test_db" },
]
teardown = [
  { run = "cargo run -p app-db --bin reset_test_db" },
]
teardown_policy = "always"
```

Target schema additions:

- `test.suites.<name>.env`
  - same logical model as task run-step env references or inline env maps
- `test.suites.<name>.env_file`
  - same model as task/run-step env_file
- `test.suites.<name>.setup`
  - array of managed run steps
- `test.suites.<name>.teardown`
  - array of managed run steps
- `test.suites.<name>.teardown_policy`
  - initial values:
    - `always`
    - `on-success`

Design constraint:

- do not add a separate builtin-test-only env resolver if existing managed env/env_file resolution can be reused

## 6) Execution Plan

### Batch 23.1 - Schema and Planning Contract

- [ ] Extend `ManifestTestSuiteTable` to support `env`, `env_file`, `setup`, `teardown`, and `teardown_policy`
- [ ] Extend doctor/schema validation for the richer `test.suites` table shape
- [ ] Update builtin test help/config docs with managed-suite examples
- [ ] Add parser tests for accepted and rejected shapes

### Batch 23.2 - Shared Env Resolution Reuse

- [ ] Extract or adapt managed task env/env_file resolution so builtin test suites can reuse it
- [ ] Remove builtin test's special-case dependence on only `CARGO_*` env where suite-level env is configured
- [ ] Preserve current cargo-env wrapping behavior for plain auto-detected suites
- [ ] Add tests covering suite env, env_file, and mixed fallback behavior

### Batch 23.3 - Setup/Teardown Execution Model

- [ ] Extend builtin test runnable model to represent lifecycle steps, not just a single command string
- [ ] Execute setup, then runner command, then teardown according to policy
- [ ] Guarantee teardown execution for `always` even when the runner command exits non-zero
- [ ] Ensure final exit status reflects runner/setup failure while still surfacing teardown failure clearly
- [ ] Add execution tests for setup failure, runner failure, teardown failure, and teardown-on-failure behavior

### Batch 23.4 - Plan and Result Explainability

- [ ] Extend `effigy test --plan` output to show lifecycle stages and env sources
- [ ] Extend verbose results/json output to describe setup/run/teardown command paths
- [ ] Keep output concise for simple suites that only use `run`
- [ ] Add contract tests for text/json rendering

### Batch 23.5 - Farmyard Migration Proof

- [ ] Replace Farmyard's custom `test:managed` wrapper behavior with builtin test suite config using nextest
- [ ] Remove repo-local managed wrapper assumptions once parity is proven
- [ ] Validate `effigy test --plan --repo .` and `effigy test --repo .` in Farmyard against the managed DB lifecycle flow
- [ ] Publish a checkpoint log documenting the migration and remaining nextest caveats

## 7) Acceptance Criteria

- [ ] A builtin test suite can declare env and env_file without falling back to a custom task wrapper
- [ ] A builtin test suite can declare setup and teardown steps
- [ ] `teardown_policy = "always"` guarantees teardown after runner failure
- [ ] Rust managed suites can still use `cargo nextest run` as the actual test runner
- [ ] `effigy test --plan` explains lifecycle stages, not just the final runner command
- [ ] Farmyard can express its managed test lifecycle without `scripts/tasks/run-managed-tests.ts`

## 8) Risks and Mitigations

- [ ] Risk: builtin test grows a second execution engine parallel to managed tasks
  - Mitigation: reuse managed env/run-step primitives wherever possible
- [ ] Risk: setup/teardown semantics become too complex for simple repos
  - Mitigation: keep command-only suite syntax valid and concise
- [ ] Risk: teardown failures obscure the original test failure
  - Mitigation: preserve primary failure cause and report teardown as secondary failure detail
- [ ] Risk: nextest filtering/passthrough interacts awkwardly with setup/teardown
  - Mitigation: scope passthrough only to the suite `run` command, not lifecycle hooks
- [ ] Risk: auto-detected suites and configured suites diverge in behavior
  - Mitigation: keep richer lifecycle features opt-in for configured suites

## 9) Deliverables

- [ ] Rich builtin test suite manifest contract
- [ ] Builtin test lifecycle execution support
- [ ] Updated help/docs/schema coverage
- [ ] Farmyard migration off custom managed test wrapper
- [ ] Validation log covering nextest-backed managed suites

## 10) Validation

- [ ] Parser and schema tests for new `test.suites` fields
- [ ] Builtin test execution tests for setup/run/teardown outcome combinations
- [ ] Plan rendering tests for lifecycle-aware suites
- [ ] Farmyard adoption validation using builtin `effigy test`
- [ ] No regression to simple command-only builtin test suites

## 11) Outcome

Status: planned

Upon completion, Effigy will support a single high-level testing story:

- builtin `effigy test` remains the default command surface
- nextest remains the preferred Rust runner
- repositories can attach lifecycle and environment policy declaratively
- custom managed wrapper scripts become optional instead of required

Next: execute roadmap `g01.023` in meaningful batches, starting with schema/config expansion and shared env-resolution reuse.
