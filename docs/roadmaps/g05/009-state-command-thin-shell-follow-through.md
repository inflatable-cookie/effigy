# g05.009 - State Command Thin-Shell Follow-Through

Status: Complete
Depends on: `g05.008`
Contract: [`027-state-domain-extraction-contract.md`](../../contracts/027-state-domain-extraction-contract.md)

## Goal

Finish the highest-confidence state ownership moves so `src/runner/state_command.rs`
becomes mostly orchestration and side effects over `effigy-state`.

## Evidence

- `src/runner/state_command.rs` remains a 2575-line warning-level god file
- `docs/contracts/027-state-domain-extraction-contract.md` already identifies
  runner-owned pure-domain candidates still left behind
- the latest audit found report models, context models, renderers, and manifest
  decode helpers still concentrated in one runner file

## Scope

- move stable state report/context models into `effigy-state` where they are not
  runner-specific
- move additional pure planning or enum-codec helpers out of the runner where
  ownership is durable
- keep file writes, task execution, hook execution, SQL import, and artifact
  staging in the runner
- shrink `state_command.rs` materially without changing command behavior

## Non-Goals

- no state CLI grammar changes
- no public JSON schema change unless a later card scopes it explicitly
- no media or object-store implementation
- no deploy/provider behavior changes

## Acceptance Criteria

- `state_command.rs` is materially smaller and easier to navigate
- moved logic is tested at the `effigy-state` layer where practical
- runner tests prove orchestration rather than duplicated domain logic
- `effigy state` command behavior remains compatible

## Suggested Validation

- `cargo test -p effigy-state`
- targeted state command tests
- `effigy state plan --json`
- `effigy state history --json`
- `effigy scan god-files --json`

## Next Task

Open a bounded card for the first extracted slice: stable state report/context
models and any adjacent pure codecs that still live in `state_command.rs`.
