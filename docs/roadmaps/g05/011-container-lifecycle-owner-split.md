# g05.011 - Container Lifecycle Owner Split

Status: Complete
Depends on: `g05.010`
Contract: [`023-container-command-decomposition-contract.md`](../../contracts/023-container-command-decomposition-contract.md)

## Goal

Finish the real `container_command` decomposition by breaking
`src/runner/container_command/lifecycle.rs` into bounded subowners instead of
keeping the new change surface in one large file.

## Evidence

- `src/runner/container_command/lifecycle.rs` is still 1699 lines and warning-level
- the file mixes startup/shutdown, shell/exec, secret injection, cleanup,
  capability probing, working-dir policy, and warning emission
- the earlier decomposition lane mostly thinned `mod.rs`, not the underlying
  lifecycle ownership surface

## Scope

- split lifecycle handling into clearer startup/shutdown, shell/exec, secrets,
  and cleanup owners
- keep `mod.rs` thin
- move reusable request/planning logic downward only where a durable owner
  exists in `effigy-containers` or `effigy-runtime`
- keep user-visible container behavior stable

## Non-Goals

- no new container features
- no backend selection redesign
- no CLI grammar change
- no opportunistic output wording churn

## Acceptance Criteria

- `lifecycle.rs` is no longer a warning-level god file
- container lifecycle subareas have obvious owners
- current container behavior and reports remain compatible

## Suggested Validation

- targeted container command tests
- `cargo test -p effigy-containers`
- `effigy scan god-files --json`
- `effigy scan duplicate-blocks --json`

## Next Task

Open a card for the first split boundary after the shared secrets-vault support
work lands: isolate lifecycle-owned secret injection and adjacent shell prep.
