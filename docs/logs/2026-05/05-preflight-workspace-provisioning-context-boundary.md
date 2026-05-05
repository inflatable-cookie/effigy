# Preflight Workspace Provisioning Context Boundary

Date: 2026-05-05

## Change

Completed card `390`.

Preflight discovery now uses `resolve_command_context_from_cwd()`. Workspace
provisioning keeps its cwd probe as an explicit local Effigy checkout discovery
hint.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- focused execution, defer, and workspace-lock contract tests

## Next Task

Implement card `391`.
