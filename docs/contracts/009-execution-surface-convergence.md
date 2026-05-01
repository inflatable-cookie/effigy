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

The first covered surfaces are:

- managed `dev`
- managed task execution
- standard routed task execution
- deferred execution
- bootstrap `run`
- bootstrap `start`
- `effigy workspace`
- `stay_in_shell`
- `effigy exec`
- exec aliases
- `effigy container up`
- `effigy container shell`
- run-array builtin command re-entry
- Rhai `run_effigy_command`
- demo task re-entry through normal task dispatch

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
| Embedded command repo targeting | shared helper under `src/runner/command_context/*` or adjacent shared runner module |
| Execution binding resolution | `src/runner/execute/binding.rs` |
| Standard routing policy | `src/runner/execute/routing.rs` plus `crates/effigy-exec` |
| Runtime activation and prep | `src/runner/container_runtime_prep.rs` |
| Handoff marker and recursion guard | `src/runner/container_runtime.rs` |
| Interactive workspace/session ownership | shared session-ownership helper under `src/runner/system_command/workspace.rs` or adjacent shared runner module |
| Gateway route reconciliation | `src/runner/container_command/gateway_registration.rs` |
| Container-local alias reconciliation | `src/runner/container_command/support.rs` through shared runtime prep |
| Host-container lease refresh and notices | `src/runner/host_container_lease.rs` |
| Embedded Effigy command re-entry | one shared embedded-runner entry introduced after this contract |

No new caller path should re-own one of these responsibilities locally unless
the contract is widened deliberately.

## Responsibility matrix

The table below describes the required behavior by surface.

| Surface | Repo targeting source | Binding source | Runtime activation source | Gateway / alias behavior | Lease behavior | Session ownership | Cleanup ownership | JSON / projection posture | Inline workspace container posture | Recursion / handoff posture |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Managed `dev` | command-context resolved root | execution binding + managed plan | shared runtime prep before shell handoff | must reconcile before session opens | none; session-owned | session-owned | session ownership model | interactive only | unsupported until one shared interactive contract exists | must obey shared handoff marker |
| Managed task execution | execution preflight resolved root | execution binding + managed plan | shared runtime prep when container-backed | same container contract as other container-backed tasks | non-shell paths refresh lease; shell paths do not | non-shell or session-owned depending on task shape | activation or session ownership model | task-shaped | unsupported surfaces must fail with shared family | must obey shared handoff marker |
| Standard routed task execution | execution preflight resolved root | execution binding + routing | shared runtime activation | same container contract as other container-backed tasks | refresh lease for non-shell activation | none unless `stay_in_shell` | activation-owned | task-shaped | inline supported only where the shared contract says so | must obey shared handoff marker |
| Deferred execution | deferral working dir resolved as repo root | execution binding resolved from target manifest | shared runtime activation | same container contract as standard task activation | same lease contract as explicit container task | none | activation-owned | deferral trace may differ; runtime semantics must not | same inline support/failure family as explicit task binding | must obey shared handoff marker |
| Bootstrap `run` | bootstrap target repo root | embedded task or managed-run binding | same shared activation as equivalent task surface | same as equivalent task surface | same as equivalent task surface | none | activation-owned | bootstrap wraps phase labels only | same as delegated task surface | must not rely on path-specific recursion shortcuts |
| Bootstrap `start` | bootstrap target repo root | delegated start selector binding | same prep and session-readiness contract as direct `dev` / workspace | same as equivalent shell surface | no lease; session-owned if interactive | session-owned | session ownership model | interactive only | same as delegated shell surface | same handoff contract as direct shell surface |
| `effigy workspace` | command-context resolved root | workspace-backed binding | shared runtime prep before shell opens | must reconcile before shell opens | none | session-owned | session ownership model | interactive only | unsupported until one shared interactive contract exists | must obey shared handoff marker |
| `stay_in_shell` | original task repo target | original task binding | same prep contract as interactive shell handoff | must reconcile before shell opens | none | session-owned | session ownership model | interactive only | must follow shared supported/unsupported rule | must obey shared handoff marker |
| `effigy exec` | command-context resolved root | dev exec surface or explicit service target | shared runtime activation | same container contract as other container-backed execution surfaces | refresh lease when non-shell activation owns warm reuse | none | activation-owned | exec-shaped | not applicable unless exec widens to inline later | not a handoff by default; still must respect container recursion rules when applicable |
| Exec aliases | same as `effigy exec` | alias table resolved from container config | same as `effigy exec` | same as `effigy exec` | same as `effigy exec` | none | activation-owned | exec-shaped | not applicable | same as `effigy exec` |
| `effigy container up` | command-context resolved root | explicit container selection | direct lifecycle path, not task activation | must register gateway and reconcile aliases during bring-up | clears task lease on explicit operator up | attached mode is session-owned; detached mode is operator-owned runtime | explicit lifecycle policy | container command report | not applicable | no nested task handoff implied |
| `effigy container shell` | command-context resolved root | explicit container selection | explicit operator path; runtime must already satisfy shell contract or prepare through shared shell-ready path once widened | shell-visible container contract must match workspace shell guarantees where both target the same runtime | no lease | session-owned | session ownership model | interactive only | not applicable | must set and respect shared handoff marker when re-entering Effigy |
| Run-array builtin re-entry | parent task repo target | shared embedded-runner entry | delegated to the resolved inner surface | same as delegated inner surface | same as delegated inner surface | same as delegated inner surface | same as delegated inner surface | nested command projection rules only | same as delegated inner surface | must not invent its own recursion rules |
| Rhai `run_effigy_command` | script repo target | shared embedded-runner entry | delegated to the resolved inner surface | same as delegated inner surface | same as delegated inner surface | same as delegated inner surface | same as delegated inner surface | nested command projection rules only | same as delegated inner surface | must not invent its own recursion rules |
| Demo task re-entry | demo repo target | shared task dispatch entry | delegated to the resolved inner surface | same as delegated inner surface | same as delegated inner surface | same as delegated inner surface | same as delegated inner surface | demo may wrap output, not runtime semantics | same as delegated inner surface | same as delegated inner surface |

## Convergence rules

The following must converge across covered surfaces.

### Repo targeting

- the same embedded command must target the same repo whether it is run from
  run-array, Rhai, bootstrap, demo re-entry, or direct CLI dispatch
- repo targeting must be resolved once and then carried, not re-guessed in each
  nested surface

### Binding resolution

- binding resolution must happen once per surface entry
- downstream code may consume the resolved binding, but should not silently
  re-derive a different one from local context

### Runtime activation

- all non-shell container-backed activation must use one shared activation
  contract
- that contract owns:
  - startup
  - exec readiness
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

### Unsupported-surface failures

- unsupported inline workspace container cases must fail with one shared family
  of operator-facing errors
- interactive `--json` rejection must remain caller-specific only where the
  surface is genuinely interactive by product design

### Embedded recursion and handoff

- all Effigy-in-Effigy re-entry must respect the shared handoff marker
- nested callers must not each define their own recursion escape hatch

## Allowed differences

Not every difference is drift. These caller-specific differences may remain as
long as they are explicit and tested.

### Output and projection differences

These may vary by surface:

- task JSON payloads
- exec output payloads
- container command reports
- bootstrap progress wrapping
- demo-specific outer projection

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
- binding-resolution ownership
- activation semantics for `exec`, deferred execution, bootstrap, or shell
  handoff
- session ownership semantics
- inline workspace container support posture
- unsupported-surface error family

## Validation direction

This contract should be proven by targeted parity tests rather than one broad
smoke suite.

Minimum proof areas:

- `effigy exec` parity with non-shell container task activation
- repo-targeting parity across run-array, Rhai, bootstrap, and direct CLI
- bootstrap `start` parity with direct `dev` or workspace shell semantics
- deferred versus explicit container-task lease parity
- interactive ownership parity across workspace, managed handoff, and
  `stay_in_shell`
- unsupported inline-container failure-family parity

## Next Task

Use this contract to centralize repo targeting and embedded command re-entry
first, then move `exec` and the remaining container-backed activation surfaces
onto the shared runtime activation contract.
