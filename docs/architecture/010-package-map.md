# Package Map

Status: active
Updated: 2026-05-02

## Purpose

This is the live code-ownership map for Effigy's runtime/container core.

Use it when you need current module truth:

- which crate owns a subsystem
- which runner module owns a runtime seam
- which docs are current authority versus background design history

Do not use older design docs as the primary ownership source when this file
 says otherwise.

## Authority Boundary

Current authority surfaces:

- [000-overview.md](./000-overview.md) for the short architecture frame
- this file for current crate and module ownership
- [020-container-infrastructure-design.md](./020-container-infrastructure-design.md)
  for longer-form container design background, not the live runtime ownership
  map
- [021-production-deployment-export-architecture.md](./021-production-deployment-export-architecture.md)
  for deploy/export architecture
- `docs/contracts/005-container-runtime-contract.md` for local runtime guarantees
- `docs/contracts/009-execution-surface-convergence.md` for cross-surface
  execution responsibility rules
- `docs/contracts/011-runtime-context-contract.md` for boot-time context and
  path authority
- `docs/contracts/012-container-manager-contract.md` for backend manager
  ownership
- `docs/contracts/013-task-execution-request-contract.md` for task request and
  resolved-plan ownership

## Workspace Crates

### Surface and presentation

| Crate | Responsibility |
| --- | --- |
| `effigy` | top-level binary/library crate; wires CLI entry, runner orchestration, and TUI export surfaces |
| `effigy-cli` | CLI argument models, parse helpers, header/help/version presentation pieces |
| `effigy-ui` | renderer abstraction, theme, plain/JSON output helpers |
| `effigy-tui` | reusable TUI runtime and multiprocess terminal UI building blocks |

### Task, manifest, and routing model

| Crate | Responsibility |
| --- | --- |
| `effigy-manifest` | manifest model, bundles, local-bundle parsing, deploy-model derivation inputs |
| `effigy-routing` | catalog discovery, selector routing, task lookup order |
| `effigy-tasks` | shared task model and task-shape helpers |
| `effigy-builtin` | builtin task inventory and builtin-facing task helpers |
| `effigy-exec` | execution-binding model and routing helpers shared below the runner |
| `effigy-execution` | canonical task execution request, surface, runtime-policy, environment-plan, and resolved-route model |
| `effigy-managed` | managed-run/task-plan support |
| `effigy-rhai` | Rhai integration and scripting support |

### Container and local runtime

| Crate | Responsibility |
| --- | --- |
| `effigy-context` | boot-time runtime context, cwd/repo target authority, host facts, and container handoff capture |
| `effigy-container-manager` | plugin-ready container manager facade, backend ids, backend operation requests, and operation reports |
| `effigy-containers` | effective container policy, compose assembly, workspace mount rewrite, and lower-level container/runtime compatibility helpers |
| `effigy-catalog` | shipped and user/project service catalogs, compose assembly inputs, catalog schema |
| `effigy-gateway` | local gateway loopback and host-port registry primitives |
| `effigy-runtime` | runtime metadata and working-dir ownership helpers |
| `effigy-process` | host process/runtime process primitives used by runner surfaces |

### Operator and policy domains

| Crate | Responsibility |
| --- | --- |
| `effigy-bootstrap` | bootstrap repo bring-up model and helpers |
| `effigy-distribution` | binary distribution and install/update helpers |
| `effigy-release` | release orchestration helpers |
| `effigy-changelog` | changelog parsing, extraction, and release-note support |
| `effigy-contracts` | contract file loading and contract-surface helpers |
| `effigy-docs-policy` | docs checks and policy validation helpers |
| `effigy-doctor` | doctor findings and diagnostics model |
| `effigy-env` | env-schema integration and env contract helpers |
| `effigy-demo` | demo model and execution helpers |
| `effigy-scan` | repository scans used by doctor/policy surfaces |
| `effigy-core` | shared low-level primitives: build info, shell helpers, runtime-dir helpers |

## Top-Level Binary Surface

| Module | Responsibility |
| --- | --- |
| [`src/lib.rs`](../../src/lib.rs) | library exports for CLI entry, JSON envelope helpers, and rendered version/header surfaces |
| [`src/cli/`](../../src/cli) | CLI entrypoint, execution context, help/version dispatch, JSON envelope emission, parse-error rendering |
| [`src/runner/mod.rs`](../../src/runner/mod.rs) | runner entry module and command-surface registration |
| [`src/tui/`](../../src/tui) | binary-local TUI export shim over `effigy-tui` |

## Runner Ownership Map

### Command entry and target resolution

| Module | Responsibility |
| --- | --- |
| [`src/runner/entrypoints.rs`](../../src/runner/entrypoints.rs) | top-level command dispatch into runner surfaces |
| [`src/runner/command_context/*`](../../src/runner/command_context.rs) | context-backed cwd/repo helper wrappers, resolved-root semantics, embedded repo override handling |
| [`src/runner/embedded_runner.rs`](../../src/runner/embedded_runner.rs) | shared embedded command replay for Rhai, bootstrap, and builtin nested dispatch |

### Runtime/container activation

| Module | Responsibility |
| --- | --- |
| [`src/runner/runtime_session_context.rs`](../../src/runner/runtime_session_context.rs) | typed runtime/session context for lease refresh and public-workspace cleanup policy |
| [`src/runner/container_runtime.rs`](../../src/runner/container_runtime.rs) | handoff marker and in-container recursion guard surface |
| [`src/runner/container_runtime_prep.rs`](../../src/runner/container_runtime_prep.rs) | shared container runtime activation, exec readiness, alias reconciliation, gateway-ready prep |
| [`src/runner/host_container_lease.rs`](../../src/runner/host_container_lease.rs) | non-shell host-container lease refresh, persistence, and reaper bootstrap |

### Execution surfaces

| Module | Responsibility |
| --- | --- |
| [`src/runner/execute/*`](../../src/runner/execute.rs) | routed task execution, managed/deferred activation handoff, execution binding consumption, `effigy-execution` request consumption |
| [`src/runner/exec_command/mod.rs`](../../src/runner/exec_command/mod.rs) | `effigy exec` command surface and raw container exec dispatch |
| [`src/runner/exec_command/surface.rs`](../../src/runner/exec_command/surface.rs) | dev-container and named-container selection for exec surfaces |
| [`src/runner/deferral/*`](../../src/runner/deferral.rs) | deferral selection, tracing, and delegated runtime activation |
| [`src/runner/script_command.rs`](../../src/runner/script_command.rs) | Rhai-owned runner entry surface over captured runtime context and execution request helpers |

### Workspace/session ownership

| Module | Responsibility |
| --- | --- |
| [`src/runner/system_command/workspace_session.rs`](../../src/runner/system_command/workspace_session.rs) | public workspace session lifecycle, ownership classification, shell-plus-cleanup combination |
| [`src/runner/system_command/workspace_provisioning.rs`](../../src/runner/system_command/workspace_provisioning.rs) | workspace artifact install, permission prep, linux workspace binary provisioning |
| [`src/runner/system_command/workspace.rs`](../../src/runner/system_command/workspace.rs) | command-surface glue, workspace handoff shell, residual session helpers, shutdown/render helpers |
| [`src/runner/interactive_session.rs`](../../src/runner/interactive_session.rs) | shared interactive ownership classification model |

### Gateway/runtime exposure

| Module | Responsibility |
| --- | --- |
| [`src/runner/container_command/gateway_registration.rs`](../../src/runner/container_command/gateway_registration.rs) | gateway route reconciliation, runtime target translation, route-table mutation |
| [`src/runner/container_command/support.rs`](../../src/runner/container_command/support.rs) | gateway/runtime support helpers shared by lifecycle and runtime prep |
| [`src/runner/gateway_command/*`](../../src/runner/gateway_command.rs) | operator gateway command surfaces and daemon management |

### Failure model

| Module | Responsibility |
| --- | --- |
| [`src/runner/error.rs`](../../src/runner/error.rs) | typed runner/runtime/container error families |
| [`src/runner/error/display.rs`](../../src/runner/error/display.rs) | operator-facing rendering for runner errors |
| [`src/runner/error/rendered_output.rs`](../../src/runner/error/rendered_output.rs) | rendered error payload helpers |

## Container Runtime Ownership Map

These are the current `effigy-containers` ownership seams that matter most to
 the runtime/container core.

| Module | Responsibility |
| --- | --- |
| [`crates/effigy-containers/src/lib.rs`](../../crates/effigy-containers/src/lib.rs) | top-level exports and compatibility facade for policy, workspace, runtime, compose, exec, session, and report modules |
| [`crates/effigy-containers/src/policy/`](../../crates/effigy-containers/src/policy/mod.rs) | effective policy model, loading, project-name shaping, validation, and inline workspace policy |
| [`crates/effigy-containers/src/policy_support.rs`](../../crates/effigy-containers/src/policy_support.rs) | facade for generated-compose support |
| [`crates/effigy-containers/src/policy_support/generated_compose.rs`](../../crates/effigy-containers/src/policy_support/generated_compose.rs) | typed generated-compose document, policy application over compose assembly, generated mount and env attachment |
| [`crates/effigy-containers/src/workspace.rs`](../../crates/effigy-containers/src/workspace.rs) | workspace compatibility facade and top-level mount assembly |
| [`crates/effigy-containers/src/workspace/`](../../crates/effigy-containers/src/workspace/) | host integration, library mounts, isolation mounts, and compose path/volume rewrite helpers |
| [`crates/effigy-containers/src/runtime/`](../../crates/effigy-containers/src/runtime/mod.rs) | runtime DNS override materialization and generated compose eject helpers |
| [`crates/effigy-containers/src/compose.rs`](../../crates/effigy-containers/src/compose.rs) | lower-level compose backend compatibility wrappers and compose invocation argument building |
| [`crates/effigy-containers/src/exec.rs`](../../crates/effigy-containers/src/exec.rs) | exec compatibility facade for process, parsing, Colima runtime, and runtime-inspection helpers |
| [`crates/effigy-containers/src/exec/`](../../crates/effigy-containers/src/exec/) | process spawning/capture, runtime output parsing, Colima runtime repair, and low-level runtime inspection helpers |
| [`crates/effigy-containers/src/session.rs`](../../crates/effigy-containers/src/session.rs) | container-local Effigy invocation prefix and session-related shell helpers |
| [`crates/effigy-containers/src/report.rs`](../../crates/effigy-containers/src/report.rs) | container command report rendering models |

## Runtime/Container Hardening Deltas

The current runtime/container architecture is not the same shape described by
 older modularization-era docs.

The important live hardening seams are now:

- typed runtime/session context instead of bootstrap-only env steering
- captured `effigy-context` authority instead of direct cwd/root rediscovery in
  new runner code
- `effigy-container-manager` facade for runner-facing backend selection and
  operation reports
- `effigy-execution` request builder for direct and embedded task plan
  construction
- typed generated-compose ownership instead of repeated YAML reparsing for the
  main generated policy seams
- explicit workspace session and provisioning owners instead of one mixed
  hotspot
- typed runtime/container error families instead of string-first translation as
  the dominant failure shape

Any architecture update that ignores those seams is stale on arrival.
