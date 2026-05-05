# Post Surface Cleanup Boundary

Date: 2026-05-05

## Summary

Completed card `393`.

## Decision

The next cleanup target is container runtime inspection invocation branching in
`crates/effigy-containers/src/exec.rs`.

## Rationale

`exec.rs` still owns Docker-vs-Colima command shape for `ps`, `inspect`, and
`stats`. `ContainerManager::runtime_process_invocation(...)` already represents
that backend boundary, so this is a narrow migration that reduces backend
branching without changing parser or lifecycle behavior.

## Next

Implement card `394`.
