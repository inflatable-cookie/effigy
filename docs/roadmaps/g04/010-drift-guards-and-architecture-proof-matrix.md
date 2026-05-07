# 010 - Drift Guards And Architecture Proof Matrix

Generation: `g04`

Status: Queued
Owner: Platform
Created: 2026-05-07
Depends on: [`009-cli-parser-modularisation-for-runtime-surfaces.md`](./009-cli-parser-modularisation-for-runtime-surfaces.md)

## Goal

Prevent runtime/container logic soup from returning.

## Scope

- add lightweight drift guards for forbidden direct calls
- document allowed adapter modules and suppression process
- prove direct CLI, bootstrap, Rhai, run-array, demo, container data, exec,
  workspace, and managed dev paths
- keep guards focused and explainable

## Guard Targets

- no direct `std::env::current_dir()` in `src/runner/**`
- no runner-local `Command::new("docker"|"colima"|"nerdctl")`
- no runner calls to `resolve_compose_backend`
- no runner calls to `compose_args` outside allowed adapter modules
- no runner calls to `run_docker_capture` outside allowed adapter modules
- no embedded task dispatch bypassing `TaskExecutionRequestBuilder`
- no Rhai container-sensitive helpers bypassing execution/container operation
  requests
- no new god file over threshold without explicit suppression

## Acceptance Criteria

- focused guard command exists
- docs explain suppression process
- proof matrix covers all critical runtime/container paths

## Validation

- guard command
- focused proof matrix tests
- `git diff --check`

## Next Task

Do not start until `g04.009` closes.
