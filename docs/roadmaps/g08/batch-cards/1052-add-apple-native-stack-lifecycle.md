# 1052 - Add Apple Native Stack Lifecycle

Roadmap: [`../018-apple-containers-native-backend-prototype.md`](../018-apple-containers-native-backend-prototype.md)
Strict lane: [`../../../specs/099-apple-containers-native-backend-prototype.md`](../../../specs/099-apple-containers-native-backend-prototype.md)

Status: Complete
Owner: Platform / container backend seam
Created: 2026-08-01
Completed: 2026-08-01

## Purpose

Consume the effective stack plan through an explicitly selected Apple native
adapter and prove one representative stack lifecycle.

## Work

- add the candidate `apple-container` backend id without auto-detection
- plan native image, network, container, readiness, exec, log, inspect, stop,
  and cleanup operations
- implement deterministic project naming and project-local service discovery
- prove app/web/database/cache lifecycle with no global DNS dependency
- reject direct Compose input before runtime side effects

## Guardrails

- do not start until `1051` is complete
- prototype selection stays explicit and experimental
- create and delete only deterministic Effigy prototype resources
- no public support claim or default selection

## Acceptance

- representative stack starts, becomes ready, communicates by service name,
  accepts routed exec, exposes ports, emits logs, stops, and removes cleanly
- repeated start and interrupted recovery are deterministic
- Docker/Colima tests remain green

## Validation

- focused manager and adapter tests
- live representative-stack proof
- `cargo test -p effigy-containers`
- `effigy qa:architecture:runtime-container-drift`
- `git diff --check`

## Stop Conditions

- stop if service discovery requires machine-global DNS mutation
- stop if native lifecycle cannot preserve manager cleanup guarantees

## Evidence

- [`01-143249-apple-native-stack-lifecycle.md`](../../../logs/2026-08/01-143249-apple-native-stack-lifecycle.md)

## Next Task

Batch complete. Ready card `1053` is active.
