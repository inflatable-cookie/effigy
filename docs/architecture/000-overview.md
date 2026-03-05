# Effigy Architecture Overview

Effigy is a Rust CLI task runner with two responsibility layers:

1. Runner infrastructure:
- CLI parsing and command routing,
- root resolution,
- catalog discovery,
- deterministic task selection,
- command execution.

2. Built-in tasks:
- task-specific collect/evaluate/render lifecycle,
- deterministic report output for operational tasks.

## Vision Alignment

- Primary tags: `MAINT`, `ROUTE`, `OPERATE`
- Target envelope: architecture framing remains clear enough that routing and operational behavior can evolve without boundary drift.
- Vision target delta: baseline architecture docs now explicitly map design intent to vision tags instead of relying on implied interpretation.

## Key design properties

- Catalogs are file-based (`effigy.toml`) so task ownership can live close to the code it operates on.
- Execution is cwd-aware but explicit override friendly via `--repo`.
- Root detection uses nearest marker semantics across `package.json`, `composer.json`, `Cargo.toml`, and `.git`.
- Unprefixed task resolution is deterministic and fails loudly on ambiguity.
- Task command payloads remain shell commands for incremental adoption.
