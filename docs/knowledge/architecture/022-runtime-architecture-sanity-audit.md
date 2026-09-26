# Runtime architecture and ownership review

Effigy's runtime and container paths use typed requests, plans, adapters, and
reports. The [package map](010-package-map.md) owns current file locations;
[contract 015](../contracts/015-runtime-operation-pipeline-contract.md) owns the
boundary rules. This review explains the critical paths and the remaining
places where direct lower-level calls need care.

## Crate Map

| Owner | Boundary |
| --- | --- |
| `effigy-context` | captured cwd, repo target, host and container facts |
| `effigy-execution` | task request and resolved route intent |
| `effigy-runtime-plan` | staged activation and lease expectations |
| `effigy-containers` | effective policy, Compose assembly, manager and operation plans |
| `effigy-runtime` | runtime read, write, data, and shell adapters |
| `effigy-data` | database service selection and SQL seed/dump plans |
| `effigy-artifacts` | local and OCI artifact references, staging, capture, apply |
| `effigy-rhai` | script host API modules and typed callbacks |
| runner | CLI dispatch, prompts, side effects, rendering, cleanup |

The source of truth for a command is its captured context and composed
manifest. A runner caller must not rediscover cwd or choose a container
backend after constructing a request.

## Critical Paths

### Direct task execution

1. The CLI identifies a built-in or manifest task and captures
   `EffigyRuntimeContext`.
2. `TaskExecutionRequestBuilder` records surface, target, runtime policy,
   output mode, and environment intent.
3. The execution plan resolves host, container, or in-container handoff.
4. Runner preflight and binding adapters perform allowed side effects and
   dispatch standard or managed execution.
5. Output and cleanup report through the initiating surface.

Direct CLI tasks, embedded run arrays, Rhai `exec::run`, deferral, bootstrap,
demo re-entry, and managed tasks must converge on equivalent route plans for
equivalent inputs. Bootstrap must keep the target repository's path authority
rather than the caller's current directory.

### Container-backed activation

`effigy-runtime-plan` models policy selection, runtime target, readiness,
route/alias reconciliation, and lease policy. The runner's
`container_runtime_prep/*` modules execute stages: policy validation, runtime
start, mount preparation, sibling services, exec readiness and permitted
recovery, gateway registration, aliases, and lease refresh.

Public shell sessions own their lifecycle and ask about shutdown on exit.
Non-shell tasks use a temporary host lease. Those modes must not infer
ownership from whether the container happened to be running when queried.

### Container commands

`container_command/*` parses a concrete operation and calls container policy,
manager, runtime, data, or gateway adapters. A destructive plan exposes its
confirmation requirement before execution. Backend choice belongs behind
`ContainerManager`; the runner may prompt and render but should not duplicate
Docker/Colima selection logic.

### Data seed and dump

`effigy-data` classifies database services, selects logical targets, and builds
seed/dump commands. `effigy-artifacts` handles local versus OCI sources and
destinations. Runner adapters prompt, stage or capture artifacts, perform
container exec or task dispatch, and render reports. `--push` is the explicit
publication boundary for OCI dumps. Secret values never enter report payloads.

### Rhai host calls

`effigy-rhai/src/host_api/*` groups APIs by domain. Runtime-sensitive
`exec::run` uses the same task request builder as CLI execution. Direct
`container::*` callbacks remain compatibility surfaces; they should use the
container operation boundary rather than inventing another transport path.

## Direct-Call Drift

`compose_args`, `run_docker_capture`, and `load_container_policy` still occur
in some runner and runtime modules. A match is not automatically a bug: an
adapter may need a backend call. The test is whether a caller is the named
adapter for that operation or has rebuilt a policy/route decision locally.

The current drift guard is `effigy qa:architecture:runtime-container-drift`.
It catches new direct cwd discovery, raw backend commands, Compose helper
calls, legacy capture paths, and Rhai bypasses outside path-scoped allowances.
A new allowance needs a concrete adapter or debt owner; widening a directory
pattern to make the check pass loses the boundary.

Raw `Command::new` also has legitimate host-process uses: gateway elevation,
artifact transport, release probes, and generic shell execution. Review a new
call by domain and side-effect ownership, not by the token alone.

## Pipeline Responsibilities

### Execution

`effigy-execution` owns request construction and route/output intent. Runner
owns parsing, side-effect adapters, and presentation. Route selection must be
inspectable without starting a container.

### Runtime activation

`effigy-runtime-plan` owns the staged activation plan. Its stages account for
selected repo and container, policy validation, running state, shared services,
bind mounts, exec readiness, permitted repair, gateway routes, aliases, and
lease behavior. The runner executes the stages in order and reports their
outcomes.

### Container operation

`effigy-containers` owns operation intent, side-effect classification,
confirmation policy, backend facade, and typed reports. `effigy-runtime` owns
read/write/shell/data adapters. The runner handles command grammar and human
interaction. A new container command should add a typed operation or extend an
existing adapter rather than shelling out directly from a command handler.

### Artifact and data

`effigy-data` owns service and target resolution, command plans, and source or
destination intent. `effigy-artifacts` owns artifact identity and transport.
The runner invokes those plans and handles credentials through the approved
secret boundary, never by copying them into JSON or diagnostics.

### Rhai

`effigy-rhai` owns module registration, argument conversion, and typed callback
interfaces. Runner callbacks use the execution/container/data pipelines. The
script API is not a shortcut around runtime policy.

## Risk Cases to Preserve

| Scenario | Boundary at risk |
| --- | --- |
| direct host and container tasks | equivalent plan and dispatch |
| inside-container handoff | local execution without recursive host routing |
| bootstrap | target repo identity and cwd |
| Rhai `exec::run` with `stdin_file` | typed environment and transport |
| run-array and demo re-entry | captured parent context |
| managed dev | activation and closeout ownership |
| `effigy exec` | selected service, user, cwd, and TTY policy |
| local and OCI data seed | staged handoff and target selection |
| local and OCI data dump | plan versus explicit push |
| reset with preserved data | named data volumes survive |
| cache prune | disposable cache only |
| gateway and aliases | reconciliation before shell handoff |
| attached interrupt | manager-owned cleanup report |

Pure plan tests and fake adapters should prove the decision boundaries.
Focused live-container checks are for backend compatibility, not the only
proof that routing or data selection is correct.

## Change Triggers

Review this architecture when a new execution surface, runtime backend,
container operation, data source, or Rhai host API bypasses the existing
request/plan/adapter seams. Update [contract 015](../contracts/015-runtime-operation-pipeline-contract.md)
when ownership itself changes.
