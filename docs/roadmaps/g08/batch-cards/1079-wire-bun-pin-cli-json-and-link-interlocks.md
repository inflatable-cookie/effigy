# 1079 - Wire Bun Pin CLI, JSON, And Link Interlocks

Roadmap: [`../031-bun-committed-dependency-pinning.md`](../031-bun-committed-dependency-pinning.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md),
[`../../../contracts/040-bun-committed-dependency-pinning-contract.md`](../../../contracts/040-bun-committed-dependency-pinning-contract.md)
Spec: [`../../../specs/archive/104-bun-committed-dependency-pinning.md`](../../../specs/archive/104-bun-committed-dependency-pinning.md)

Status: Complete
Owner: Dependency command surface
Created: 2026-08-11
Ready after: card `1078` closed with its domain contract green

## Purpose

Expose the domain transaction through the public CLI and prevent committed and
machine-local ownership from overlapping.

## Owner And Seam

`effigy-cli` owns grammar/help, `effigy-deps` owns overlap decisions, and the
runner owns root resolution, rendering, envelopes, and exit status. JSON schema
artifacts change with the command.

## Work

- add `deps pin bun` and `deps unpin bun` parsing with `--dry-run`, leading
  `--repo`, and global `--json`
- reject Cargo pin/unpin with a direct unsupported-manager diagnostic
- resolve relative library paths from the selected consumer repository
- render deterministic committed-state plans, outcomes, warnings, writes,
  verification, and `bun install` next actions
- add `effigy.deps.pin.v1`, schema-index registration, and selection proof
- refuse pin when an overlapping Effigy Bun link is active
- make link planning recognize a matching committed override and decline link
  mutation without changing the override
- preserve existing link/unlink/status schemas and save-less behavior
- update CLI help, completion/released-surface fixtures, and focused text/JSON
  contract tests

## Acceptance

- [x] grammar and root resolution match contract `040`
- [x] Cargo requests fail explicitly without invoking Cargo patch behavior
- [x] text and JSON outcomes have equivalent success/failure semantics
- [x] the new schema is versioned, indexed, selected, and example-validated
- [x] overlapping link/pin states are refused with exact remediation
- [x] link never writes an override and unpin never creates a link
- [x] existing plain relative docs-index links and dependency schemas remain
      green

## Validation

- focused CLI parse/help/completion tests
- focused runner text/JSON and exit-contract tests
- focused `effigy-deps` overlap fixtures
- `effigy qa:json`
- `effigy qa:docs`
- formatting, focused Clippy, affected analysis, and `git diff --check`

## Evidence Requirement

Close with one dated log containing grammar, root-resolution, overlap, text/JSON,
schema-selection, and regression evidence.

Evidence:
[`11-232228-bun-pin-cli-json-and-interlocks.md`](../../../logs/archive/2026-08/11-232228-bun-pin-cli-json-and-interlocks.md)

## Stop Conditions

Stop if runner code must own package matching or JSON text surgery, overlap
handling requires silent state conversion, an existing dependency schema must
break, or link would edit a manifest.

## Next Task

Execute ready card
[`1080`](./1080-prove-bun-pin-consumer-workflow-and-closeout.md).
