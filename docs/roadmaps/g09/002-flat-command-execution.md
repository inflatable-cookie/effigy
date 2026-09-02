# g09.002 Flat Command Execution

Status: Active
Created: 2026-09-02
Spec: [`117`](../../specs/117-flat-command-execution-strict-lane.md)
Architecture: [`026`](../../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md)

## Purpose

Keep the useful help taxonomy from `g09.001`, but remove its executable
namespace aliases and restore direct built-in invocation as the single canonical
operator grammar.

## Sequence

1. [`1110`](./batch-cards/1110-remove-executable-command-namespaces.md) —
   **Ready**: remove executable aliases and migration diagnostics; restore
   direct help, completion, current documentation, generated references, and
   managed-skill guidance without changing genuine subcommands.

The lane is serial because parser, dispatch, warning-envelope, help,
completion, docs, skill parity, and closeout share one command-route authority.

## Acceptance

- direct built-in spellings are canonical and warning-free
- `local`, `repo`, `deliver`, `extend`, and `admin` no longer route as built-in
  namespaces or reserve manifest task names
- general help remains grouped by operator job and `help <group>` remains
  available, with direct spellings in each inventory
- completion and current guidance teach direct spellings
- command-owned subcommands and existing selector precedence stay unchanged
- no release, consumer rewrite, or unrelated feature-boundary work enters the
  lane

## Non-Goals

- renaming help groups
- flattening genuine subcommands
- adding a new built-in escape for shadowed direct commands
- release execution or a version decision
- consumer-repository edits
- S3 extraction or extension transport

## Next Task

Execute ready card
[`1110`](./batch-cards/1110-remove-executable-command-namespaces.md).
