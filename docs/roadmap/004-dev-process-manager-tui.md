# 004 - Dev Process Manager TUI

Status: In Progress
Owner: Platform
Created: 2026-02-26
Depends on: 001, 003

## 1) Problem

Effigy can launch commands, but local multi-process development still requires manual terminal splitting and ad-hoc process coordination. For common flows (API + frontend + admin), we need a first-class interactive process manager so `effigy dev` can orchestrate and monitor all services in one terminal instance.

## 2) Goals

- [x] Add a process-manager TUI mode for task commands such as `effigy dev`.
- [x] Spawn multiple predefined child processes from task catalog configuration.
- [x] Support named TOML-driven profiles per managed task (for example `default`, `admin`).
- [x] Provide one tab per process with live output stream.
- [x] Support stdin passthrough to focused process tab (for flows like Vite `r + Enter` restart).
- [ ] Keep process-manager scope focused on managed processes only (no embedded shell terminal).
- [ ] Keep process lifecycle deterministic (start order, shutdown order, signal handling, exit propagation).

## 3) Non-Goals

- [ ] No remote/cluster process orchestration.
- [ ] No long-term daemon mode in phase 004.
- [ ] No process auto-healing policy beyond explicit restart controls.
- [ ] No replacement of normal non-TUI command output paths.

## 4) UX Contract

Default invocation:
- `effigy dev`
- `effigy dev <profile>`

Expected behavior:
- Effigy resolves `dev` task as a managed process group task.
- Effigy chooses process set by profile:
  - no profile arg uses default profile.
  - profile arg (for example `admin`) selects configured subset for that profile.
- Alternate-screen TUI opens (ratatui).
- Tabs shown for each configured process:
- API, frontend, admin (or project-defined equivalents).
- No embedded shell terminal; use managed process tabs only.
- Focused tab receives stdin input.
- Global keys support tab switching, help, and graceful shutdown.

## 5) Config Model (Target)

Roadmap target for catalog schema extensions:

```toml
[tasks.dev]
mode = "tui"

[tasks.dev.profiles.default]
processes = ["api", "front", "admin"]

[tasks.dev.profiles.admin]
processes = ["api", "admin"]

[tasks.dev.processes.api]
run = "cargo run -p farmyard-api"

[tasks.dev.processes.front]
run = "vite dev"

[tasks.dev.processes.admin]
run = "vite dev --config admin.vite.config.ts"


```

Notes:
- This roadmap defines the direction; exact schema can evolve during phase 4.1 if a cleaner manifest shape is identified.
- `effigy dev` resolves `profiles.default`.
- `effigy dev admin` resolves `profiles.admin`.
- Unknown profiles fail with a validation error listing available profiles.

## 6) Execution Plan

### Phase 4.1 - Schema and Runner Wiring
- [x] Define managed-task schema for process groups (`mode=tui`, process map, profile map).
- [x] Define CLI profile binding for task passthrough (`effigy dev <profile>`).
- [x] Extend parser/runner selection path to route managed tasks to TUI runtime.
- [x] Add validation errors for malformed process definitions.
- [x] Add validation errors for missing/default/unknown profile references.
- [x] Add compatibility behavior when task is missing/invalid.

### Phase 4.2 - Process Runtime Engine
- [x] Build async process supervisor (spawn, stdout/stderr capture, stdin pipe).
- [x] Expand profile-selected process IDs to runnable process specs.
- [ ] Implement lifecycle policy: startup ordering, cancellation, and graceful shutdown.
- [x] Implement focus-based stdin dispatch to active process.

### Phase 4.3 - Ratatui Interface
- [x] Add alternate-screen ratatui app host.
- [x] Implement tab bar + per-tab scrollback view.
- [x] Implement status line (running/exited/error, pid where useful).
- [ ] Add keymap help overlay and deterministic key handling.

### Phase 4.4 - Dev Ergonomics and Safety
- [ ] Add restart controls (focused process restart, full stack restart).
- [x] Add bounded scrollback ring buffer per process.
- [ ] Add exit summary and non-zero propagation rules.
- [x] Add no-color / CI fallback behavior (non-interactive fallback to managed plan output).

### Phase 4.5 - Validation and Docs
- [ ] Add integration tests for process spawn/stop and stdin passthrough.
- [ ] Add docs for authoring `mode=tui` task manifests.
- [ ] Add migration examples for existing multi-process dev scripts.
- [ ] Publish a report with real-project smoke validation (`acowtancy` as first adopter).

## 7) Acceptance Criteria

- [x] `effigy dev` can launch and manage at least 3 configured processes concurrently.
- [x] `effigy dev <profile>` launches only the processes configured for that profile.
- [x] Each process has dedicated tabbed output view with preserved recent logs.
- [x] Focused tab receives stdin correctly (including interactive restart commands).
- [x] Shell tab supports ad-hoc commands without leaving the TUI.
- [ ] Graceful shutdown works from one key chord and cleans child processes reliably.

## 8) Risks and Mitigations

- [ ] Risk: terminal-state corruption on panic/forced exit.
  - Mitigation: strict teardown guards and integration tests around alternate-screen recovery.
- [ ] Risk: mixed stdout/stderr ordering ambiguity.
  - Mitigation: timestamped line buffering at capture boundary.
- [ ] Risk: stdin contention across tabs.
  - Mitigation: single-focus input routing model with explicit active-tab indicator.

## 9) Deliverables

- [x] Managed-task schema support in runner.
- [x] Process supervisor runtime.
- [x] Ratatui tabbed process manager.
- [ ] Docs + migration examples + validation report.
