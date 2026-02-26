# 004 - Dev Process Manager TUI

Status: Not Started
Owner: Platform
Created: 2026-02-26
Depends on: 001, 003

## 1) Problem

Effigy can launch commands, but local multi-process development still requires manual terminal splitting and ad-hoc process coordination. For common flows (API + frontend + admin), we need a first-class interactive process manager so `effigy dev` can orchestrate and monitor all services in one terminal instance.

## 2) Goals

- [ ] Add a process-manager TUI mode for task commands such as `effigy dev`.
- [ ] Spawn multiple predefined child processes from task catalog configuration.
- [ ] Provide one tab per process with live output stream.
- [ ] Support stdin passthrough to focused process tab (for flows like Vite `r + Enter` restart).
- [ ] Provide an extra interactive shell tab to run ad-hoc commands while dev stack is running.
- [ ] Keep process lifecycle deterministic (start order, shutdown order, signal handling, exit propagation).

## 3) Non-Goals

- [ ] No remote/cluster process orchestration.
- [ ] No long-term daemon mode in phase 004.
- [ ] No process auto-healing policy beyond explicit restart controls.
- [ ] No replacement of normal non-TUI command output paths.

## 4) UX Contract

Default invocation:
- `effigy dev`

Expected behavior:
- Effigy resolves `dev` task as a managed process group task.
- Alternate-screen TUI opens (ratatui).
- Tabs shown for each configured process:
- API, frontend, admin (or project-defined equivalents).
- Dedicated "shell" tab available for ad-hoc command execution.
- Focused tab receives stdin input.
- Global keys support tab switching, help, and graceful shutdown.

## 5) Config Model (Target)

Roadmap target for catalog schema extensions:

```toml
[tasks.dev]
mode = "tui"

[[tasks.dev.processes]]
name = "api"
run = "cargo run -p farmyard-api"

[[tasks.dev.processes]]
name = "front"
run = "vite dev"

[[tasks.dev.processes]]
name = "admin"
run = "vite dev --config admin.vite.config.ts"

[tasks.dev.shell]
enabled = true
run = "$SHELL"
```

Notes:
- This roadmap defines the direction; exact schema can evolve during phase 4.1 if a cleaner manifest shape is identified.

## 6) Execution Plan

### Phase 4.1 - Schema and Runner Wiring
- [ ] Define managed-task schema for process groups (`mode=tui`, process list, shell tab options).
- [ ] Extend parser/runner selection path to route managed tasks to TUI runtime.
- [ ] Add validation errors for malformed process definitions.
- [ ] Add compatibility behavior when task is missing/invalid.

### Phase 4.2 - Process Runtime Engine
- [ ] Build async process supervisor (spawn, stdout/stderr capture, stdin pipe).
- [ ] Implement lifecycle policy: startup ordering, cancellation, and graceful shutdown.
- [ ] Implement focus-based stdin dispatch to active process.
- [ ] Implement shell-tab process with interactive stdin/stdout handling.

### Phase 4.3 - Ratatui Interface
- [ ] Add alternate-screen ratatui app shell.
- [ ] Implement tab bar + per-tab scrollback view.
- [ ] Implement status line (running/exited/error, pid where useful).
- [ ] Add keymap help overlay and deterministic key handling.

### Phase 4.4 - Dev Ergonomics and Safety
- [ ] Add restart controls (focused process restart, full stack restart).
- [ ] Add bounded scrollback ring buffer per process.
- [ ] Add exit summary and non-zero propagation rules.
- [ ] Add no-color / CI fallback behavior (non-TUI rejection with clear message).

### Phase 4.5 - Validation and Docs
- [ ] Add integration tests for process spawn/stop and stdin passthrough.
- [ ] Add docs for authoring `mode=tui` task manifests.
- [ ] Add migration examples for existing multi-process dev scripts.
- [ ] Publish a report with real-project smoke validation (`acowtancy` as first adopter).

## 7) Acceptance Criteria

- [ ] `effigy dev` can launch and manage at least 3 configured processes concurrently.
- [ ] Each process has dedicated tabbed output view with preserved recent logs.
- [ ] Focused tab receives stdin correctly (including interactive restart commands).
- [ ] Shell tab supports ad-hoc commands without leaving the TUI.
- [ ] Graceful shutdown works from one key chord and cleans child processes reliably.

## 8) Risks and Mitigations

- [ ] Risk: terminal-state corruption on panic/forced exit.
  - Mitigation: strict teardown guards and integration tests around alternate-screen recovery.
- [ ] Risk: mixed stdout/stderr ordering ambiguity.
  - Mitigation: timestamped line buffering at capture boundary.
- [ ] Risk: stdin contention across tabs.
  - Mitigation: single-focus input routing model with explicit active-tab indicator.

## 9) Deliverables

- [ ] Managed-task schema support in runner.
- [ ] Process supervisor runtime.
- [ ] Ratatui tabbed process manager.
- [ ] Docs + migration examples + validation report.
