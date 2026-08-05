# g08.019 - Dependency Inventory And Command Foundation

Status: Complete
Depends on: `g08.018`
Completed: 2026-08-05

## Goals

- [x] establish the shared dependency-domain owner used by status, Cargo, Bun,
      and doctor
- [x] make desired and observed dependency-link state typed and inspectable
- [x] expose the agreed CLI/status/JSON foundation without manager mutation

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Target envelope: identical repo and manager inputs produce a deterministic
  package inventory, link plan, and JSON representation.
- Vision target delta: dependency source state becomes typed and inspectable
  rather than inferred independently by each manager shell.

## Scope

- settle the smallest reusable domain boundary consumable by command and doctor
- add `DepsArgs` grammar/help/completion for:
  - `effigy deps`
  - `effigy deps status [cargo|bun]`
  - `effigy deps link <manager> <path> [--dry-run]`
  - `effigy deps unlink <manager> <path> [--dry-run]`
- model manager, library, consumer root, package, source, desired state,
  observed state, drift, plan, verification, and report
- implement canonical library/repo path resolution
- implement versioned atomic
  `.effigy/local/dependency-links.json` read/write planning
- ensure `.effigy/` local state is ignored and expose any planned ignore delta
- define the locked, atomic `~/.effigy/deps/bun-registrations.json` ownership
  index consumed by the later Bun adapter
- provide read-only status over empty, healthy, missing-path, and drifted state
- add the versioned dependency JSON payload and contract fixtures
- add Cargo/Bun inventory fixtures without applying links yet

## Execution Plan

- [x] [`1051`](./batch-cards/1051-establish-dependency-domain-and-state-foundation.md)
      — establish the `effigy-deps` crate, canonical models, and state stores
- [x] [`1052`](./batch-cards/1052-add-read-only-dependency-inventory-and-status.md)
      — add deterministic read-only Cargo/Bun inventory and observed status
- [x] [`1053`](./batch-cards/1053-wire-deps-cli-json-and-foundation-closeout.md)
      — wire CLI/help/completion, JSON contracts, and milestone closeout

## Non-Goals

- no Cargo config writes
- no Bun registration or symlink writes
- no doctor findings yet
- no future update/audit/migrate command placeholders

## Acceptance Criteria

- [x] parser/help/completion cover the agreed grammar and reject unknown
      managers/flag combinations actionably
- [x] bare `effigy deps` and explicit status are equivalent
- [x] `--dry-run` cannot reach a mutating adapter
- [x] desired-state ledger round-trips deterministically and atomically
- [x] Bun registration ownership index round-trips under a concurrency lock
- [x] missing local-state ignore coverage is planned and reported
- [x] status distinguishes desired, observed, and drifted state
- [x] text and JSON share one report model
- [x] JSON contract checks select and validate the new payload
- [x] ownership is added to the live package map when code lands

## Evidence

- [`../../logs/2026-08/05-155727-dependency-domain-state-foundation.md`](../../logs/2026-08/05-155727-dependency-domain-state-foundation.md)
- [`../../logs/2026-08/05-162005-read-only-dependency-inventory-status.md`](../../logs/2026-08/05-162005-read-only-dependency-inventory-status.md)
- [`../../logs/2026-08/05-163456-deps-cli-json-foundation-closeout.md`](../../logs/2026-08/05-163456-deps-cli-json-foundation-closeout.md)

## Validation

- focused CLI parser/help/global-JSON tests
- focused dependency-domain inventory/state/report tests
- JSON example and selection-contract checks
- `effigy qa:ci:fast`

## Next Task

Execute ready batch card `1054` for `g08.020`.
