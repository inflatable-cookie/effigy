# 011 - Runtime Context Contract

Status: Active
Owner: Platform
Created: 2026-05-05

## Purpose

Effigy must resolve invocation and runtime facts once at process entry, then
pass those facts through command execution instead of rediscovering them in
caller-local code.

This contract covers the boot-time context needed by local execution,
container-backed work, embedded command replay, and bootstrap handoff.
Rhai scripts are part of that surface: a script must be able to ask Effigy for
the same context the runner is using instead of guessing from cwd, env vars, or
container-local paths.

## Context Owner

The canonical context type is `effigy_context::EffigyRuntimeContext`.

It owns:

- invocation cwd
- resolved command root
- effective repo override
- resolved target evidence
- host OS and architecture
- selected process env facts used as context, including `HOME`, `PATH`,
  `SHELL`, `NO_COLOR`, and `CI`
- whether the process is already inside Effigy's container handoff marker

The runner may still keep surface-specific request state, but it must not
invent a second boot-time context model.

## Path Rules

- resolve cwd once at CLI entry
- resolve repo target once for the parsed command
- carry invocation cwd and command root through dispatch
- command modules should consume the context or wrappers backed by it
- embedded callers must preserve their explicit repo target instead of falling
  back to process cwd

## Handoff Rules

The container handoff marker remains:

- env var: `EFFIGY_INTERNAL_CONTAINER_HANDOFF`

`EffigyRuntimeContext` captures whether it is present. Downstream execution may
use that fact to avoid recursive container dispatch, but should not repeatedly
probe the env var when a context is available.

## Migration Boundary

The first migration keeps old runner helpers as compatibility wrappers while
new entrypoint dispatch passes `EffigyRuntimeContext`.

Future migration must move these callers behind the context:

- command-local `current_working_dir()` plus `resolve_repo_root()` pairs
- task preflight discovery
- bootstrap target handoff
- Rhai and run-array embedded dispatch
- demo task re-entry
- container/runtime prep callers that re-probe cwd or handoff state

## Rhai Exposure Rules

The Rhai host surface must expose a read-only runtime context view before
first-party scripts depend on context-sensitive path or container behavior.

Minimum Rhai context fields:

- invocation cwd
- command root / repo root
- effective repo override, when present
- invocation mode: host or container handoff
- inside-container handoff flag
- host OS and architecture

Rhai scripts must not decide host-versus-container execution by probing cwd,
`env(...)`, or container marker variables directly. When a script needs a
command to run in a service container, it should submit a typed execution
request with `run_in = "container"` and service/container intent. The execution
builder owns whether that becomes host `process::run(...)`, container
`exec(...)`, or an inside-container direct process execution.

## Drift Triggers

Update this contract when Effigy changes:

- repo target resolution rules
- embedded repo override propagation
- container handoff marker semantics
- boot-time host/context facts
- public context inspectability, if added later

## Validation Direction

Minimum proof:

- direct CLI and `--repo` preserve current root resolution
- nested command replay keeps the parent repo target
- bootstrap task dispatch targets the cloned repo, not invocation cwd
- container handoff state is captured once and is visible through the context
- Rhai can read the captured context and run container-targeted execution
  without host/container path guessing
- runner code has a drift guard against new direct cwd discovery
