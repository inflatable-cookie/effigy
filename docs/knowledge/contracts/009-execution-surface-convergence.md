# 009 - Execution Surface Convergence

This contract defines the shared behavior Effigy must preserve across its
execution surfaces.

It exists to stop product semantics from depending on caller path, config
shape, or historical implementation seams.

## Purpose

Effigy now has several surfaces that can all mean some version of:

- resolve a repo target
- resolve a task or command binding
- prepare or reuse a local runtime
- dispatch work
- optionally hand off into an interactive session

Those surfaces must not drift just because one path came from managed `dev`,
another from standard task routing, another from bootstrap, and another from
embedded command replay.

This contract defines:

- the execution surfaces Effigy owns
- the shared responsibilities that must converge
- the owning modules for those responsibilities
- the narrow exceptions that may remain caller-specific

## Covered surfaces

Every entry that starts local execution is covered: managed and standard task
routes, deferred execution, bootstrap `run` and `start`, `effigy workspace`,
`stay_in_shell`, `effigy exec` and its aliases, `container up` and
`container shell`, run-array re-entry, and the Rhai runtime helpers. A new
entry joins this contract by naming its owner in the ownership map below.

This contract covers local execution semantics. It does not define provider
export or production deployment behavior.

## Shared responsibilities

Effigy must treat these as shared product responsibilities, not local caller
implementation details:

- repo targeting
- embedded command targeting
- execution binding resolution
- runtime activation
- gateway and alias reconciliation
- lease refresh for non-shell activation
- handoff recursion and in-container re-entry rules
- interactive session ownership
- cleanup ownership
- unsupported-surface failure families

Each responsibility must have one owning module or one clearly named shared
contract surface.

## Ownership map

Current shared ownership should converge around:

| Responsibility | Owner |
| --- | --- |
| Repo targeting and resolved root semantics | `src/runner/command_context/*` |
| Boot-time runtime context | `effigy_context::EffigyRuntimeContext` |
| Task execution request and resolved target | `effigy_execution::TaskExecutionRequestBuilder`; contract `013-task-execution-request-contract.md` |
| Embedded command repo targeting | shared helper under `src/runner/command_context/*` or adjacent shared runner module |
| Execution binding resolution | `src/runner/execute/binding.rs` plus `effigy-execution` binding summaries |
| Standard routing policy | `src/runner/execute/routing.rs`, `crates/effigy-exec`, and `effigy-execution` dispatch plans |
| Runtime activation planning | `effigy-runtime-plan`; contract `015-runtime-operation-pipeline-contract.md` |
| Runtime activation side effects | `src/runner/container_runtime_prep/*` stage adapters |
| Container operation planning | `effigy-containers`; contract `015-runtime-operation-pipeline-contract.md` |
| Container backend operations | `effigy-containers` and `effigy-runtime` adapters |
| Data seed/dump planning | `effigy-data`; artifact transport through `effigy_state::artifact` |
| Handoff marker and recursion guard | `src/runner/container_runtime.rs` |
| Interactive workspace/session ownership | shared session-ownership helper under `src/runner/workspace_command/` or adjacent shared runner module |
| Gateway route reconciliation | `src/runner/container_command/gateway_registration.rs` |
| Container-local alias reconciliation | `src/runner/container_command/support.rs` through shared runtime prep |
| Environment holds, idle stop and notices | `src/runner/environment.rs` (contract 053) |
| Embedded Effigy command re-entry | one shared embedded-runner entry introduced after this contract |

No new caller path should re-own one of these responsibilities locally unless
the contract is widened deliberately.

## Convergence rules

The following must converge across covered surfaces.

### Repo targeting

- the same embedded command must target the same repo whether it is run from
  run-array, Rhai, bootstrap, or direct CLI dispatch
- repo targeting must be resolved once and then carried, not re-guessed in each
  nested surface

### Binding resolution

- binding resolution must happen once per surface entry
- downstream code may consume the resolved binding, but should not silently
  re-derive a different one from local context

### Request construction

- direct and embedded task dispatch must start from
  `TaskExecutionRequestBuilder` once the caller has its selector, command argv,
  runtime context, surface label, output mode, runtime policy, handoff policy,
  cleanup policy, and environment plan
- request construction must preserve the captured `EffigyRuntimeContext`
  instead of re-probing process cwd or handoff env vars
- Rhai `exec::run(...)` must submit command intent and runtime policy through
  the request builder instead of choosing host process execution versus
  container exec locally
- `stdin_file`, cwd, and env overrides must travel through
  `ExecutionEnvironmentPlan`
- equivalent inputs across direct CLI, bootstrap, Rhai, run-array, deferral,
  and managed task surfaces should produce equivalent resolved plans
  unless this contract names a deliberate surface difference

### Runtime activation

- all non-shell container-backed activation must use one shared activation
  contract
- non-shell activation refreshes the task lease; shell and interactive paths
  do not. A session-owned shell holds no lease
- explicit operator lifecycle (`container up`) may clear or supersede task
  leases; a task surface may not
- an inline workspace container is supported only where the shared contract
  says so and fails with the shared family everywhere else
- that contract owns:
  - startup
  - readiness
  - gateway and route reconciliation
  - alias reconciliation
  - lease refresh
- `effigy exec` belongs to this contract unless a later documented exception is
  deliberately introduced

### Interactive ownership

- shell and attached-session cleanup must derive from one ownership model
- readiness completion must include route/gateway completion where public
  runtime exposure is part of the surface contract
- adopted runtimes must not tear down or stay alive based on caller-specific
  local booleans alone

### Cleanup ownership

- in-process run-array shell steps that set `timeout_ms` spawn the step in a
  new Unix process group, stop that group on expiry, reap the direct child, and
  report exit `124`. Expiry sends SIGTERM, then SIGKILL if the group remains
  live after the grace interval. The wait acquires the existing owned-children
  signal scope (shared when a heavy-run scope is already active) so SIGINT,
  SIGTERM, and SIGHUP still reach the owned group; a recorded cancellation
  then uses the same group SIGTERM/SIGKILL reap as expiry, because a background
  descendant may ignore SIGINT. The caller process group is never signalled.
- managed lifecycle cleanup uses the same owned-group timeout reap (90s budget)
  and interrupt forwarding, and still reports a cleanup timeout as a
  task-invocation error.
- without `timeout_ms`, a sequence shell stays in the caller group unless a
  heavy-run signal scope is active, matching standard task execution.
- non-Unix hosts keep pid-only kill; descendants may outlive a timeout.

### Unsupported-surface failures

- unsupported inline workspace container cases must fail with one shared family
  of operator-facing errors
- interactive `--json` rejection must remain caller-specific only where the
  surface is genuinely interactive by product design

### Embedded recursion and handoff

- all Effigy-in-Effigy re-entry must respect the shared handoff marker
- nested callers must not each define their own recursion escape hatch
- request resolution must turn container intent into local container handoff
  when the captured runtime context is already inside Effigy's handoff marker

## Allowed differences

Not every difference is drift. These caller-specific differences may remain as
long as they are explicit and tested.

### Output and projection differences

These may vary by surface:

- task JSON payloads
- exec output payloads
- container command reports
- bootstrap progress wrapping

These are presentation differences, not runtime-contract differences.

### Command-construction differences

These may vary by surface:

- standard tasks run task command strings
- `exec` runs raw command argv
- exec aliases render configured alias commands
- managed flows may render or stream a managed plan

These must not widen into lifecycle differences.

### Legitimate operator lifecycle differences

These may remain different:

- explicit `container up` and `container down` are operator lifecycle commands
- attached `container up` is not the same product surface as non-shell task
  activation
- direct operator lifecycle may clear or supersede task leases

Those differences must stay explicit in code and docs.

## Known intentional exceptions

The following exceptions are currently allowed and should remain explicit until
they are either removed or widened deliberately.

1. Managed presentation is allowed to differ from standard task presentation.
   The managed surface may still own TUI/render-plan concerns that standard
   routed tasks do not.

2. `container up` remains a direct operator lifecycle surface rather than being
   silently routed through task activation.

3. Inline workspace containers are not yet universally supported. Supported and
   unsupported surfaces must be named explicitly and must fail consistently.

4. Interactive surfaces may reject `--json` where the product truly opens a
   shell or attached session rather than returning a result object.

## Drift triggers

Update this contract when Effigy changes:

- which execution surfaces are public or embedded entrypoints
- repo-targeting propagation rules
- runtime context ownership or request-builder fields
- binding-resolution ownership
- activation semantics for `exec`, deferred execution, bootstrap, or shell
  handoff
- session ownership semantics
- inline workspace container support posture
- unsupported-surface error family
- runtime operation pipeline ownership from
  `015-runtime-operation-pipeline-contract.md`

## Validation direction

This contract should be proven by targeted parity tests rather than one broad
smoke suite.

Minimum proof areas:

- `effigy exec` parity with non-shell container task activation
- repo-targeting parity across run-array, Rhai, bootstrap, and direct CLI
- resolved plan parity across direct task, bootstrap task, and Rhai task
- Rhai `exec::run(...)` container-targeted command proof with `stdin_file`
- inside-container handoff proof for container-targeted requests
- bootstrap `start` parity with direct `dev` or workspace shell semantics
- deferred versus explicit container-task lease parity
- interactive ownership parity across workspace, managed handoff, and
  `stay_in_shell`
- unsupported inline-container failure-family parity
- container data seed/dump local and `oci://` parity through `effigy-data` and
  `effigy_state::artifact`
- drift guard coverage from `effigy qa:architecture:runtime-container-drift`
