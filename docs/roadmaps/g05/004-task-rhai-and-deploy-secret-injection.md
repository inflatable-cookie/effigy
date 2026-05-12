# g05.004 - Task, Rhai, And Deploy Secret Injection

Status: Complete
Depends on: `g05.003`

## Goal

Make declared, unlocked secrets available to task execution, Rhai scripts, and
deploy provider packages without writing plaintext repo files.

## Scope

- Extend the task execution request/plan path to carry secret requirements.
- Resolve secrets for targets:
  - `tasks`
  - `rhai`
  - `deploy`
  - `state`
  - `artifacts`
- Inject task secrets through process environment APIs, not shell command
  strings.
- Expose a controlled Rhai API for declared secrets.
- Pass deploy secrets into provider-package phase scripts through the deploy
  context.
- Ensure provider package reports cannot print secret values.
- Add missing-secret blockers before executing mutating commands.
- Preserve `.env.schema` behavior while making `[secrets]` the source of truth
  for true secret material.

## Rhai Surface

Target shape:

```rhai
let token = effigy.secret("render_api_key");
let has_token = effigy.has_secret("render_api_key");
```

Rhai access should be declaration-bound. Scripts should not enumerate secret
values unless a later roadmap explicitly justifies that surface.

## Non-Goals

- No container-service injection.
- No persistent compatibility `.env` files.
- No provider secret creation.
- No secret reads for undeclared keys.

## Acceptance Criteria

- Task commands can receive declared secrets without command-string leakage.
- Rhai scripts can request declared secrets through a small host API.
- Deploy provider packages can consume declared provider credentials.
- State apply hook tasks can consume declared state credentials.
- Missing required secrets block before side effects.
- Secret values are redacted from captured task JSON, Rhai errors, host logs,
  process result maps, and Effigy callback maps.

## Closeout

Completed by cards `712` through `716`.

Artifact-targeted secret scope is available to internal Rhai workflow callers,
but the current built-in artifact stage/capture commands do not execute Rhai
scripts. No artifact-specific caller was added in this roadmap.

## Test Strategy

- Task execution tests proving env injection and command redaction.
- Rhai host API tests for present, missing, undeclared, and redacted values.
- Deploy provider package fixture tests.
- Failure-stage tests ensuring missing secrets stop mutation.
- Snapshot/contract tests for JSON redaction.

## Next Task

Start container runtime secret injection in `g05.005`.
