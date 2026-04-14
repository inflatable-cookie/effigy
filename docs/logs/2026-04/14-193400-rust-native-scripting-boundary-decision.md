# Rust-Native Scripting Boundary Decision

Date: 2026-04-14

## Outcome

Settled the scripting policy split and Rhai v1 boundary for Rust-first repos.

## Decision

- Rust-first repos should prefer Effigy-native scripting.
- Web-oriented repos should continue to prefer Bun + TypeScript.
- Rhai is the chosen Effigy-native scripting candidate for the Rust-first path.
- Jetstream is explicitly a full migration target, not a permanent Python
  exception.

## Rhai v1 Boundary

Rhai v1 is for repo automation glue, not general shell emulation.

### Include

- file-backed Rhai scripts referenced from the manifest
- logging helpers
- args access
- env read
- path helpers
- file read/write/exists/create-dir
- JSON/TOML parse and stringify helpers
- structured subprocess execution without shell parsing
- task invocation helpers where that keeps orchestration readable

### Exclude

- arbitrary shell-string execution as the core model
- network APIs
- daemon/process-supervisor semantics
- frontend/build-tool replacement
- Python/analysis replacement in the first implementation slice

## Repo Classification

### Migrate Early

- `effigy`
  - `scripts/install-local-bin-links.sh`
  - small docs/demo/report helpers

### Migrate After Effigy Proof

- `keepsake`
  - release-candidate packaging helper
  - REAPER smoke orchestration wrappers

### Needs Rust Helper Work But Still Targets Full Migration

- `jetstream`
  - bash QA/policy orchestration is a clear Rhai target
  - Python analysis tools are still migration targets, but likely need Rust
    host capability before Rhai can replace them cleanly

## Pilot Order

1. Effigy Rhai script-step foundation
2. Effigy shell-glue pilot migration
3. Keepsake orchestration pilot
4. Jetstream bash orchestration migration
5. Jetstream analysis-tool migration once Rust helper surfaces are explicit

## Next Task

Execute `087-implement-rhai-script-step-foundation.md`.
