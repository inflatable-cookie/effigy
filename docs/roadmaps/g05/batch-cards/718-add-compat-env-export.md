# 718 - Add Compatibility Env Export

Roadmap: [`../005-container-secret-injection.md`](../005-container-secret-injection.md)
Strict lane: [`../../../specs/080-container-secret-injection-strict-lane.md`](../../../specs/080-container-secret-injection-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Add an explicit plaintext export bridge for tools that cannot yet consume
Effigy-managed secrets directly.

## Scope

- add command:

```sh
effigy secrets export --format env --output <PATH> --yes
```

- require `--yes`
- reject stdout export
- reject unsafe source-controlled default destinations where practical
- unlock the local vault for export
- export only declared secrets requested by the manifest model
- never print values in command output or JSON reports
- document export as an unsafe compatibility bridge

## Non-Goals

- no default `.env` generation
- no automatic container use of exported files
- no team sync
- no provider secret creation

## Acceptance

- export writes a dotenv-compatible file only when explicitly confirmed
- command output names the destination and keys but never values
- missing required values block before writing
- unsafe destinations are rejected or require an explicit documented override
- command reference and JSON examples document the bridge clearly

## Completed

- Added `effigy secrets export --format env --output <PATH> --yes`.
- Required `--yes` for plaintext export.
- Rejected stdout export and repo-root `.env`.
- Exported declared vault values as dotenv-compatible `KEY=VALUE` lines.
- Blocked missing required secrets before writing.
- Kept command output and JSON value-free.
- Updated help, command reference, and JSON examples.

## Validation

- CLI parser tests
- export write tests
- missing required blocker tests
- value redaction tests
- docs checks
- `cargo check --all-targets`
- `git diff --check`

## Next Task

Close `g05.005` or continue to the Underlay/Example App proof in `g05.006`,
depending on container injection outcome.
