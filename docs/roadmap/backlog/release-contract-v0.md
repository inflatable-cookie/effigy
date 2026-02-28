# Release Contract (v0.x)

Status: Draft
Owner: Platform
Created: 2026-02-27
Related: [Distribution Channels](./distribution-channels.md)

## 1) Purpose

Define the minimum release contract required before promoting Effigy to stable distribution channels (crates + Homebrew).

This contract is intentionally scoped to `v0.x` while feature shape is still converging.

## 2) Versioning Policy (v0.x)

- Version format: `0.MINOR.PATCH`.
- `PATCH`: bug fixes and output polish with no intentional breaking behavior.
- `MINOR`: may include breaking changes, but must include migration notes.
- Public references should use exact versions in automation and CI during `v0.x`.

## 3) Compatibility Guarantees

Within a given `PATCH` line:

- CLI invocation forms must remain stable:
  - `effigy <task>`
  - `effigy <catalog>/<task>`
  - `effigy tasks`
  - `effigy doctor`
  - `effigy test`
- Config parsing must remain backward compatible for existing supported keys.
- Built-in task names remain reserved and stable (`help`, `config`, `doctor`, `tasks`, `test`).

Across `MINOR` bumps:

- Breaking changes are allowed, but must include:
  - migration notes,
  - before/after examples,
  - explicit mention in release notes.

## 4) Output Stability Policy

- Human-readable output can evolve for readability.
- Machine-readable assumptions must not rely on plain CLI text layout.
- If/when machine integration is required, add explicit structured output mode and version it.

## 5) Release Gating Checklist

A version can be tagged for channel publication only if all are true:

- [ ] `cargo test` passes on release branch.
- [ ] Smoke checks pass on active workspace(s):
  - `help`
  - `tasks`
  - prefixed built-ins (`farmyard/tasks`, `farmyard/test`)
  - `test --plan`
- [ ] Release notes drafted with change summary and migration notes (if needed).
- [ ] Rollback candidate tag identified (previous known-good).
- [ ] Distribution metadata validated (crate metadata, install docs, checksum path).

Operational template:
- [Release Checklist Template](../../guides/014-release-checklist-template.md)

## 6) Rollback and Hotfix Policy

Rollback trigger examples:

- broad task-resolution regression,
- built-in task routing failures,
- install/upgrade failures in primary channel.

Rollback procedure:

1. Pause new channel publishes.
2. Communicate affected versions and impact.
3. Re-point install guidance to previous known-good version.
4. Ship hotfix patch or yank broken crate version if needed.

Hotfix expectations:

- Use `PATCH` bump within same `MINOR` line.
- Include focused regression test for root cause.
- Add short checkpoint report under `docs/reports/`.

## 7) Support Window (Initial)

- Maintain latest `MINOR` line only during early `v0.x`.
- No long-term support branch until `v1` planning.

## 8) Promotion Criteria to v1 Planning

Start `v1` contract planning when:

- distribution channels are stable for at least two release cycles,
- no major migration pain reported across active workspaces,
- CLI/config surface is mostly additive for one full `MINOR` cycle.
