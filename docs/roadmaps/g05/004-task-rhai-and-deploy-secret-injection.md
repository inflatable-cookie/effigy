# g05.004 - Task, Rhai, And Deploy Secret Injection

Status: Planned
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
- Missing required secrets block before side effects.
- Secret values are redacted from task status, deploy reports, provider reports,
  JSON envelopes, and errors.

## Test Strategy

- Task execution tests proving env injection and command redaction.
- Rhai host API tests for present, missing, undeclared, and redacted values.
- Deploy provider package fixture tests.
- Failure-stage tests ensuring missing secrets stop mutation.
- Snapshot/contract tests for JSON redaction.

## Next Task

Add container runtime secret injection in `g05.005`.

