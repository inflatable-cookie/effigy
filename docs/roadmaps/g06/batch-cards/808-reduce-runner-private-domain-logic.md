# 808 - Reduce Runner-Private Domain Logic

Roadmap: [`../008-runner-private-domain-logic-reduction.md`](../008-runner-private-domain-logic-reduction.md)
Strict lane: [`../../../specs/084-codebase-lean-down-strict-lane.md`](../../../specs/084-codebase-lean-down-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Move durable non-CLI logic out of runner command modules where a clearer owner
already exists.

## Scope

- inventory mixed-responsibility runner command helpers
- move only domain-owned logic, not final rendering or CLI adaptation
- avoid broad new utility layers

## Acceptance

- runner command modules own less durable domain logic
- moved logic lands under clearer owners
- diagnostics stay readable

## Current Progress

- moved state report/context-file writing into `effigy-state`
- moved state apply/capture env construction into `effigy-state`
- moved state apply skip-layer validation and history-kind parsing into
  `effigy-state`
- moved standalone/composed state manifest loading and named capture-profile
  request expansion into `effigy-state`
- `src/runner/state_command.rs` dropped from `1918` lines at the start of
  `g06.008` to `1605`
- `cargo run --bin effigy -- scan god-files --json` now reports zero findings

## Suggested Validation

```bash
cargo test -p effigy-state
cargo test state_command
cargo run --bin effigy -- scan god-files --json
```

## Next Task

Execute `809`.
