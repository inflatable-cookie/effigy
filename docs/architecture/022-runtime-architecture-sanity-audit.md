# 022 - Runtime Architecture Sanity Audit

Status: Active
Owner: Platform
Created: 2026-05-07

## Executive Summary

Effigy has good architectural foundations, but the runtime/container core still
has too much caller-local orchestration.

The important crates already exist:

- `effigy-context` owns boot-time path and host/container facts.
- `effigy-execution` owns task execution request and route intent.
- `effigy-containers` owns the backend facade and typed container operation
  plans.
- `effigy-artifacts` owns artifact references, staging, and OCI transport
  boundaries.
- `effigy-runtime` owns runtime data/read/write helpers.
- `effigy-containers` owns effective container policy and compose assembly.

The current weakness is not the absence of abstractions. It is incomplete
ownership migration. Several runner paths still call lower-level helpers
directly, rebuild execution/runtime intent locally, or combine planning,
prompting, command rendering, side effects, and output formatting in one file.

`g04` should make the runtime/container core boring, typed, and inspectable.
The primary success metric is ownership purity. File size matters only where it
reveals mixed ownership.

## Current Crate Map

Runtime/container critical-path crates:

| Crate | Current role | Audit finding |
| --- | --- | --- |
| `effigy-context` | boot-time context | good authority surface; new runner code should continue consuming it |
| `effigy-execution` | task request and route intent | needs promotion into full execution planning authority |
| `effigy-containers` | backend facade, operation plans, and compatibility helpers | needs continued ownership tightening in `lib.rs`, `workspace.rs`, `exec.rs`, and `policy_support.rs` |
| `effigy-containers` | effective policy and compose assembly | too much mixed ownership in `lib.rs`, `workspace.rs`, `exec.rs`, and `policy_support.rs` |
| `effigy-runtime` | runtime data/read/write/shell helpers | still constructs compose/runtime commands directly |
| `effigy-artifacts` | artifact source/destination model | solid substrate; data pipeline should consume it rather than re-owning staging details |
| `effigy-rhai` | script runtime and host API | host API registration is a god file and callback surface needs stronger domain typing |

## Critical Runtime Paths

### Direct Task Execution

Current path:

1. CLI parses `Command::Task`.
2. runner captures or retrieves `EffigyRuntimeContext`.
3. `TaskExecutionRequestBuilder` creates a request.
4. `src/runner/execute/api.rs` unwraps the request back into a task invocation.
5. runner preflight, binding, managed/standard dispatch, runtime prep, process
   execution, and cleanup proceed through runner-owned modules.

Gap:

- `effigy-execution` owns request shape but not the real dispatch plan.
- runner code still owns most execution planning.

### Container-Backed Task Activation

Current path:

1. execution pipeline resolves binding and policy.
2. `container_runtime_prep` prepares runtime.
3. standard/managed paths still call compose helpers for cleanup or recovery.
4. gateway, alias, lease, exec-readiness, and mount prep are all present but
   not modelled as one typed activation pipeline.

Gap:

- runtime activation is a procedure, not a plan.
- callers can still introduce local booleans or direct helper calls.

### Container Commands

Current path:

1. `container_command/mod.rs` dispatches by CLI subcommand.
2. lifecycle, support, data, gateway, runtime, and cache code call a mixture of
   `effigy-runtime`, `effigy-containers`, and lower-level helpers.
3. manager reports exist, but operation shape is not consistently manager-owned.

Gap:

- no single typed `ContainerOperationRequest`.
- safety policy and side-effect class are scattered.

### Container Data Seed/Dump

Current path:

1. CLI parsing creates `BootstrapDbSeedInput` or `ContainerDbDumpInput`.
2. `db_seed.rs` resolves paths, prompts, stages artifacts, writes legacy env,
   resolves logical DB targets, and dispatches seed tasks.
3. `container_command/data.rs` loads policies, prompts, resolves dump targets,
   renders DB commands, runs container exec, writes dumps, captures artifacts,
   and renders reports.

Gap:

- data target resolution and DB command rendering need a domain crate.
- prompt/output behavior should stay runner-side.
- artifact staging should be consumed as a service, not recreated locally.

### Rhai Runtime-Sensitive Work

Current path:

1. `effigy-rhai/src/host_api.rs` registers every host module.
2. `src/runner/script_command/mod.rs` wires callbacks.
3. some Rhai helpers use `TaskExecutionRequestBuilder`; container helpers still
   call runner container exec helpers directly.

Gap:

- host module registration has no module ownership.
- runtime-sensitive callbacks should be typed by domain and route through
  execution/container operation requests.

## God Files And Mixed Ownership

Files over 700 lines in or near runtime/container paths:

| File | Lines | Mixed ownership |
| --- | ---: | --- |
| `crates/effigy-rhai/src/host_api.rs` | 2164 | Rhai module registry, host FS/process/http/json/toml/config/task/container/docs/deploy/system/cache/gateway APIs |
| `crates/effigy-cli/src/command_parsing.rs` | 1822 | many top-level parser surfaces in one file |
| `crates/effigy-containers/src/exec.rs` | 1645 | process invocation, backend detection, Colima repair, stats/log parsing, inspect parsing |
| `src/runner/container_command/data.rs` | 1629 | policy load, prompts, data target planning, DB dump command rendering, exec, artifact capture, output |
| `crates/effigy-containers/src/lib.rs` | 1546 | policy model, load, validation, project naming, inline workspace, DNS override, eject |
| `crates/effigy-containers/src/workspace.rs` | 1541 | mount rewrite, host SSH/composer/mkcert integration, isolation, YAML path normalization |
| `crates/effigy-containers/src/policy_support.rs` | 1306 | generated compose document ownership and policy application |
| `src/runner/db_seed.rs` | 1203 | prompt, artifact staging, env compatibility, seed task dispatch, logical DB target, builtin DB import |
| `src/runner/container_command/gateway_registration/mod.rs` | 1176 | route reconciliation, live process probes, gateway mutation |
| `src/runner/script_command/mod.rs` | 1080 | Rhai callback wiring, feature router, command parsers, JSON option conversion |
| `crates/effigy-runtime/src/data.rs` | 970 | data list/cache/transfer/pull-production/report helpers |
| `src/runner/execute/pipeline/managed.rs` | 854 | managed plan dispatch, gateway start, cleanup, special process materialization, presentation |
| `src/runner/container_command/support.rs` | 849 | runtime support, shared service helpers, alias reconciliation, volume usage shelling |
| `src/runner/artifact_command.rs` | 801 | artifact CLI, inspect/stage/capture, rendering, fake adapter tests |
| `crates/effigy-cli/src/command_parsing_container.rs` | 777 | all container parser branches |
| `src/runner/container_command/lifecycle.rs` | 776 | lifecycle, exec helpers, status/logs/shell/reset/eject support |
| `src/runner/execute/pipeline/standard.rs` | 723 | standard execution, runtime activation, shell handoff, cleanup |

Line count is not the primary target. These files matter because each one owns
several reasons to change.

## Direct-Call Drift Inventory

These are migration targets, not immediate defects.

### `compose_args(...)` in runner/runtime paths

Known runtime-facing matches include:

- `src/runner/managed_shell.rs`
- `src/runner/container_runtime_prep/mod.rs`
- `src/runner/system_command/workspace_provisioning.rs`
- `src/runner/deferral/run.rs`
- `src/runner/execute/pipeline/managed.rs`
- `src/runner/execute/pipeline/standard.rs`
- `src/runner/container_command/support.rs`
- `src/runner/container_command/lifecycle.rs`
- `src/runner/exec_command/transport.rs`
- `src/runner/exec_command/transport/colima.rs`
- `src/runner/demo_command/execute/task/selection.rs`
- `crates/effigy-runtime/src/read.rs`
- `crates/effigy-runtime/src/write.rs`
- `crates/effigy-runtime/src/shell.rs`
- `crates/effigy-runtime/src/session.rs`

Target:

- runner and runtime code use container operation or manager adapters.
- `compose_args` remains only in lower-level manager/backend compatibility
  modules while migration is in progress.

### `run_docker_capture` in runner/runtime paths

Known matches include:

- `src/runner/container_runtime_prep/mod.rs`
- `src/runner/execute/pipeline/standard.rs`
- `src/runner/container_command/support.rs`
- `src/runner/container_command/lifecycle.rs`
- `crates/effigy-runtime/src/data.rs`
- `crates/effigy-runtime/src/read.rs`
- `crates/effigy-runtime/src/write.rs`
- `crates/effigy-runtime/src/signals.rs`

Target:

- runner code stops calling Docker-named helpers.
- runtime crate exposes manager-backed operations, not Docker-named helpers.

### `load_container_policy(...)` in runner/runtime paths

Known matches include:

- `src/runner/system_command.rs`
- `src/runner/execute/pipeline/managed.rs`
- `src/runner/execute/pipeline/standard.rs`
- `src/runner/host_container_lease.rs`
- `src/runner/container_command/data.rs`
- `src/runner/container_command/mod.rs`
- `src/runner/container_command/lifecycle.rs`
- `src/runner/exec_command/surface.rs`
- `crates/effigy-runtime/src/data.rs`
- `crates/effigy-runtime/src/read.rs`
- `crates/effigy-runtime/src/write.rs`
- `crates/effigy-runtime/src/shell.rs`

Target:

- policy load can remain in container policy owner, but most callers should
  receive a typed operation or activation request that owns when policy is
  loaded.

### Raw `Command::new(...)`

Expected direct process execution remains valid for host process, gateway
elevation, demo terminal, artifact transport, release tests, and generic shell
process surfaces.

Risky runtime-sensitive matches include:

- `crates/effigy-rhai/src/host_api.rs`
- `crates/effigy-rhai/src/lib.rs`
- `crates/effigy-runtime/src/signals.rs`
- `crates/effigy-runtime/src/write.rs`
- `src/runner/container_command/support.rs`
- `src/runner/container_command/data/hooks.rs`
- `src/runner/container_command/gateway_registration/mod.rs`
- `src/runner/execute/pipeline/managed.rs`
- `src/runner/exec_command/transport.rs`
- `src/runner/exec_command/transport/colima.rs`

Target:

- runtime/container-sensitive commands route through process or manager
  adapters.
- allowed raw host commands are explicitly documented.

### Direct Rhai Container Helpers

Current callback wiring in `src/runner/script_command/mod.rs` still exposes:

- `container_exec`
- `container_exec_with_options`
- `container_up`
- `container_down`
- `container_shell`

Target:

- `exec::run(...)` remains the preferred host/container-aware execution path.
- `container::*` helpers route through `ContainerOperationRequest`.

## Target Pipeline Architecture

### Execution Pipeline

Owner: `effigy-execution`

Responsibilities:

- request construction
- preflight input modelling
- binding plan
- runtime policy
- output policy
- dispatch plan
- diagnostics

Runner responsibility:

- parse CLI
- call planner
- execute side-effect adapters
- render output

### Runtime Activation Pipeline

Owner: new `effigy-runtime-plan`

Stages:

1. load or receive effective policy
2. validate runtime target
3. ensure runtime running
4. ensure shared services
5. prepare host bind mount dirs
6. probe exec readiness
7. repair/restart when allowed
8. reconcile gateway routes
9. reconcile container-local aliases
10. refresh lease when activation owns warm reuse

### Container Operation Pipeline

Owner: `effigy-containers`

Responsibilities:

- operation request
- operation plan
- side-effect class
- safety/confirmation requirement
- manager operation adapter
- operation report shape

Runner responsibility:

- parse CLI
- prompt when requested by operation plan
- call side-effect adapter
- render text/JSON

### Data Pipeline

Owner: new `effigy-data`

Responsibilities:

- logical target resolution
- seed source plan
- dump target plan
- database command rendering
- artifact handoff plan
- compatibility env plan

Runner responsibility:

- prompt
- container exec/task dispatch
- artifact adapter invocation
- text/JSON output

### Rhai Host Pipeline

Owner: `effigy-rhai` split modules or new `effigy-rhai-host`

Responsibilities:

- module registry
- per-module registration
- typed callback interfaces
- conversion helpers
- host surface drift tests

Runner responsibility:

- provide callbacks backed by execution/container/data pipelines.

## Proposed Crate And Module Splits

New crates:

- `crates/effigy-runtime-plan`
- `crates/effigy-containers`
- `crates/effigy-data`
- `crates/effigy-rhai-host`, only if splitting inside `effigy-rhai` is not
  enough

Existing crate splits:

- `crates/effigy-containers/src/policy/*`
- `crates/effigy-containers/src/workspace/*`
- `crates/effigy-containers/src/runtime/*`
- `crates/effigy-runtime/src/data/*`
- `crates/effigy-runtime/src/read/*`
- `crates/effigy-runtime/src/write/*`
- `crates/effigy-runtime/src/shell/*`
- `crates/effigy-rhai/src/host_api/*`, or `effigy-rhai-host/src/*`

## Migration Risk

High-risk behavior to preserve:

- direct CLI task routing
- bootstrap target repo path authority
- Rhai `exec::run(... run_in=container ...)`
- run-array re-entry
- managed dev activation
- `effigy exec`
- workspace shell handoff and cleanup
- `container data seed`
- `container data dump`
- `container data dump --push`
- `container reset --keep-data`
- named-volume cache pruning versus persistent data preservation
- gateway and alias reconciliation before shell handoff
- attached container closeout on interrupt

Risk controls:

- pure plan tests before side-effect migration
- migrate one caller family at a time
- keep public JSON unchanged unless a card explicitly promotes a schema change
- add drift guards after migration, not before the first enabling adapter exists

## Proof Matrix

Minimum proof before `g04` closes:

| Scenario | Required proof |
| --- | --- |
| direct host task | equivalent plan and successful dispatch |
| direct container task | activation plan includes runtime prep and lease policy |
| inside-container handoff | container intent resolves to local handoff |
| bootstrap task | target repo remains bootstrap target |
| Rhai `exec::run` | container policy uses request builder, not local process guessing |
| run-array re-entry | embedded dispatch preserves parent context |
| demo task re-entry | delegates through normal execution plan |
| managed dev | activation and cleanup ownership unchanged |
| `effigy exec` | manager-backed operation path |
| data seed local SQL | same staged handoff |
| data seed OCI | artifact source resolves through same plan shape |
| data dump local SQL | same file output |
| data dump planned OCI | artifact capture report remains planned |
| data dump OCI push | explicit `--push` reports pushed digest |
| reset keep-data | persistent named volumes preserved |
| cache prune | purge-safe volumes only |
| gateway/alias | route/alias reconciliation before shell handoff |
| attached interrupt | manager-owned cleanup report |

## g04 Roadmap Queue

- `g04.001` - Runtime Architecture Sanity Audit and Generation Rollover
- `g04.002` - Execution Pipeline Ownership
- `g04.003` - Runtime Activation Pipeline
- `g04.004` - Container Operation Pipeline
- `g04.005` - Data Seed Dump Pipeline
- `g04.006` - Rhai Host API Split and Callback Purity
- `g04.007` - Effective Container Policy Decomposition
- `g04.008` - Manager Backed Runtime Read Write Shell
- `g04.009` - CLI Parser Modularisation for Runtime Surfaces
- `g04.010` - Drift Guards and Architecture Proof Matrix
- `g04.011` - Contract Promotion and Closeout

## Next Task

Start card
[`515-add-data-target-selection-plan.md`](../specs/batch-cards/515-add-data-target-selection-plan.md).
