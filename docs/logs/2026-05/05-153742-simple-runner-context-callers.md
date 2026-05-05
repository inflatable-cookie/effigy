# Simple Runner Context Callers

Date: 2026-05-05

## Change

Completed card `388`.

Added `command_context::resolve_active_repo_root()` and migrated simple command
entry modules away from local cwd/root wrapper pairs.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next Task

Implement card `389`.
