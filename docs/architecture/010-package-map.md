# Package Map

Status: active
Updated: 2026-08-10

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
- `docs/contracts/014-artifact-substrate-contract.md` for artifact transport,
  staging, seed, and dump handoff ownership
- `docs/contracts/015-runtime-operation-pipeline-contract.md` for the `g04`
  request/plan/report/adapter pipeline boundaries
- `docs/contracts/037-explicit-catalog-membership-contract.md` for root-owned
  catalog membership, typed system mounts, and routing normalization
- `docs/architecture/025-external-skill-task-execution.md` and
  `docs/contracts/042-external-skill-task-runner-contract.md` for isolated
  external task-source and consumer-target ownership

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
| `effigy-manifest` | manifest model, composition, explicit catalog-member schema, typed system/workspace mounts, bundles, and deploy-model derivation inputs |
| `effigy-routing` | explicit catalog membership normalization, canonical member identity, catalog loading, selector routing, and task lookup order |
| `effigy-tasks` | shared task model and task-shape helpers |
| `effigy-builtin` | builtin task inventory and builtin-facing task helpers |
| `effigy-exec` | execution-binding model and routing helpers shared below the runner |
| `effigy-execution` | canonical task execution request, dispatch/preflight/binding summaries, surface, runtime-policy, environment-plan, and resolved-route model |
| `effigy-managed` | managed-run/task-plan support |
| `effigy-rhai` | Rhai integration and scripting support |

### Container and local runtime

| Crate | Responsibility |
| --- | --- |
| `effigy-context` | boot-time runtime context, cwd/repo target authority, host facts, and container handoff capture |
| `effigy-containers` | effective container policy, backend facade, typed container operation planning, compose assembly, typed system/workspace mount rendering, workspace mount rewrite, and lower-level container/runtime compatibility helpers |
| `effigy-catalog` | shipped and user/project service catalogs, compose assembly inputs, catalog schema |
| `effigy-gateway` | local gateway loopback and host-port registry primitives |
| `effigy-runtime-plan` | typed runtime activation request, activation plan, readiness/alias/lease plan, and activation report substrate |
| `effigy-runtime` | runtime metadata, data/read/write/shell adapter helpers, and manager-backed runtime IO wrappers |
| `effigy-process` | host process/runtime process primitives used by runner surfaces |
| `effigy-data` | data target resolution, seed/dump source normalization, artifact handoff planning, and database command rendering |
| `effigy-artifacts` | artifact refs, OCI adapter, staging/apply/capture plans, metadata, and operation reports |

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
| `effigy-deps` | machine-local dependency-link identities, Cargo/Bun inventory, Cargo and Bun planning/application/verification, observed status, and atomic desired-state stores |
| `effigy-papercuts` | project/collection papercut discovery, tolerant Markdown parsing, normalized reports, diagnostics, fingerprints, and safe queue insertion |
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
| [`src/runner/container_runtime_prep.rs`](../../src/runner/container_runtime_prep.rs) | side-effect adapter for `effigy-runtime-plan` activation stages: policy validation, running-state checks, mount prep, compose up, exec readiness, alias reconciliation, gateway readiness, and lease refresh |
| [`src/runner/host_container_lease.rs`](../../src/runner/host_container_lease.rs) | non-shell host-container lease refresh, persistence, and reaper bootstrap |

### Execution surfaces

| Module | Responsibility |
| --- | --- |
| [`src/runner/execute/*`](../../src/runner/execute.rs) | routed task execution, managed/deferred activation handoff, execution binding consumption, and `effigy-execution` request/dispatch-plan consumption |
| [`src/runner/exec_command/mod.rs`](../../src/runner/exec_command/mod.rs) | `effigy exec` command surface and container exec dispatch over runtime activation and transport adapters |
| [`src/runner/exec_command/surface.rs`](../../src/runner/exec_command/surface.rs) | dev-container and named-container selection for exec surfaces |
| [`src/runner/deferral/*`](../../src/runner/deferral.rs) | deferral selection, tracing, and delegated runtime activation |
| [`src/runner/script_command.rs`](../../src/runner/script_command.rs) | Rhai-owned runner entry surface over captured runtime context and execution request helpers |

### Container operation and data surfaces

| Module | Responsibility |
| --- | --- |
| [`src/runner/container_command/*`](../../src/runner/container_command.rs) | container command-surface glue: parse resolved CLI model, call operation/runtime/data helpers, render operator output |
| [`src/runner/container_command/data.rs`](../../src/runner/container_command/data.rs) | container data command glue over `effigy-data`, `effigy-artifacts`, container operation plans, and runtime IO adapters |
| [`src/runner/db_seed.rs`](../../src/runner/db_seed.rs) | bootstrap and task-facing DB seed glue over `effigy-data` source normalization, artifact staging, and task execution requests |
| [`src/runner/artifact_command.rs`](../../src/runner/artifact_command.rs) | artifact command glue over `effigy-artifacts` refs, OCI transport, staging, apply, and capture plans |

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

## Runtime Operation Pipeline Crates

`g04` introduced small planning crates so runner modules describe intent instead
of owning every runtime/container decision locally.

| Crate | Main types | Responsibility |
| --- | --- | --- |
| [`effigy-runtime-plan`](../../crates/effigy-runtime-plan/src/lib.rs) | `RuntimeActivationRequest`, `RuntimeActivationPlan`, `RuntimeReadinessPlan`, `RuntimeAliasPlan`, `RuntimeLeasePlan`, `RuntimeActivationReport` | Pure runtime activation planning and report shape. Side effects remain in runner/runtime adapters. |
| [`effigy-containers`](../../crates/effigy-containers/src/lib.rs) | `ContainerManager`, `ContainerOperationRequest`, `ContainerOperationPlan`, `BackendId`, operation kind structs, and side-effect/safety models | Canonical container-domain crate for backend selection, typed lifecycle/read/exec/data/cache/volume planning, compose/runtime compatibility helpers, and workspace-aware container policy handling. |
| [`effigy-data`](../../crates/effigy-data/src/lib.rs) | `DataTargetRef`, `ResolvedDataTarget`, `DataSeedPlan`, `DataDumpPlan`, `DatabaseCommandPlan`, `ArtifactDataHandoff` | Data seed/dump planning, logical target resolution, database command rendering, and artifact handoff normalization. |
| [`effigy-artifacts`](../../crates/effigy-artifacts/src/lib.rs) | artifact refs, OCI refs, staging/capture/apply requests and reports | Artifact transport/staging substrate used by seed/dump and artifact commands. |

## Artifact Substrate Ownership Map

`effigy-artifacts` is now an internal module facade rather than a one-file
domain crate.

| Module | Responsibility |
| --- | --- |
| [`crates/effigy-artifacts/src/lib.rs`](../../crates/effigy-artifacts/src/lib.rs) | public compatibility facade and artifact crate tests |
| [`crates/effigy-artifacts/src/refs.rs`](../../crates/effigy-artifacts/src/refs.rs) | local/OCI source refs, source types, artifact kinds, and reference parsing |
| [`crates/effigy-artifacts/src/metadata.rs`](../../crates/effigy-artifacts/src/metadata.rs) | artifact metadata schema and metadata builder |
| [`crates/effigy-artifacts/src/staging.rs`](../../crates/effigy-artifacts/src/staging.rs) | local and pulled-OCI staging requests, reports, copy logic, and metadata writes |
| [`crates/effigy-artifacts/src/oci.rs`](../../crates/effigy-artifacts/src/oci.rs) | OCI request/report models, ORAS adapter, descriptor parsing, and ORAS failure remediation |
| [`crates/effigy-artifacts/src/reports.rs`](../../crates/effigy-artifacts/src/reports.rs) | artifact operation report model and operation/result enums |
| [`crates/effigy-artifacts/src/errors.rs`](../../crates/effigy-artifacts/src/errors.rs) | artifact ref, staging, and OCI error families |
| [`crates/effigy-artifacts/src/util.rs`](../../crates/effigy-artifacts/src/util.rs) | private path, slug, digest, and redaction helpers |

Media/object-store work should build on this artifact substrate. It should not
reimplement OCI ref parsing, staging metadata, digest handling, or ORAS
redaction in runner/app code.

## Small Crate Boundary Posture

Small crates are not merge candidates by line count. They stay separate when
they own a stable public seam used by more than one shell surface or when they
protect dependency direction.

Current retained small-crate rationale:

| Crate | Keep / defer note |
| --- | --- |
| `effigy-core` | Keep. Bottom utility layer for build info, shell helpers, resolver helpers, and runtime-dir helpers. |
| `effigy-catalog` | Keep. Owns service-catalog fragment/schema/template assembly without pulling in runner, manifest, or CLI policy. |
| `effigy-changelog` | Keep. Owns changelog AST, parse, format, validate, and extract logic behind one reusable seam. |
| `effigy-exec` | Keep. Owns pure container-exec routing, cwd mapping, and alias logic without runtime side effects. |
| `effigy-routing` | Keep. Explicit catalog membership, selector routing, and catalog lookup order stay independent from CLI and runner orchestration. |
| `effigy-runtime-plan` | Keep. Pure activation request/plan/report model; small by design because side effects stay in runtime adapters. |
| `effigy-deps` | Keep. Shared dependency-link state and report owner consumed by command and doctor surfaces without importing either shell. |
| `effigy-process` | Keep. Host process primitives are reused across runner surfaces without importing container/runtime crates. |
| `effigy-gateway` | Keep. Local gateway registry and route primitives are consumed by runtime/container code without dragging command-shell behavior down. |
| `effigy-ui` | Keep. Renderer abstraction and output primitives keep domain crates out of top-level CLI rendering details. |
| `effigy-tui` | Keep. Thin TUI-only composition boundary; intentionally tiny because browser/demo terminal modules stay behind one crate-local seam. |

Current defer notes after the `g07.056` cleanup pass:

- `effigy-runtime` stays separate. It is a runtime-facing facade across read,
  write, session, shell, data, task-status, and signal flows, not a dead shim.
- `effigy-context` stays separate. It is still the typed captured-context owner
  used by several runner-facing paths.
- `effigy-doctor` stays separate. It is no longer a tiny library; it owns the
  doctor workflow and report model.
- `effigy-bootstrap`, `effigy-distribution`, and `effigy-release` remain
  cleanup targets for future internal decomposition, not crate-merge
  candidates.

No small crate is currently a merge candidate on ownership grounds. Future
merge proposals must prove the boundary has no useful API, not just that the
crate has few lines.

## Runtime/Container Hardening Deltas

The current runtime/container architecture is not the same shape described by
 older modularization-era docs.

The important live hardening seams are now:

- typed runtime/session context instead of bootstrap-only env steering
- captured `effigy-context` authority instead of direct cwd/root rediscovery in
  new runner code
- `effigy-containers` facade for runner-facing backend selection, operation
  planning, and operation reports
- `effigy-execution` request builder for direct and embedded task plan
  construction
- `effigy-runtime-plan` activation requests/plans for runtime prep identity,
  lease policy, and report shape
- `effigy-containers` operation plans for lifecycle, read, exec/shell, data,
  cache, and volume command intent
- `effigy-data` seed/dump planning for DB targets, artifact handoff, and
  database command rendering
- `effigy-artifacts` artifact transport/staging/capture substrate for OCI and
  local payloads
- typed generated-compose ownership instead of repeated YAML reparsing for the
  main generated policy seams
- explicit workspace session and provisioning owners instead of one mixed
  hotspot
- typed runtime/container error families instead of string-first translation as
  the dominant failure shape

Any architecture update that ignores those seams is stale on arrival.
