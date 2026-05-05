# Container Manager Lane Closeout

Date: 2026-05-05

## Change

Closed `g03.031` and lane `038`.

The manager facade now owns runner-level backend selection for lifecycle,
exec, copy, data, shared compose, runtime volume, and generated image removal
paths. The remaining `effigy-containers` backend helpers are documented as
temporary lower-level compatibility wrappers.

## Validation

- `rg "resolve_compose_backend|ComposeBackend" src/runner/exec_command src/runner/container_command crates/effigy-runtime/src/write.rs -n`
- `git diff --check`

## Next Task

Choose the next queued roadmap deliberately. The likely next roadmap is
`g03.033`.
