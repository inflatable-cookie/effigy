# g08.040 Catalog-Pack Acquisition Prototype

Status: Ready
Created: 2026-09-01
Architecture: [`026`](../../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md)
Spec: [`113`](../../specs/113-catalog-pack-acquisition-prototype-strict-lane.md)
Card: [`1095`](./batch-cards/1095-prototype-catalog-pack-acquisition.md)

## Purpose

Prove that Effigy can select, install, validate, activate, inspect, roll back,
and reset an independently versioned default catalog pack without making any
existing catalog-backed workflow harder or network-dependent.

## Decisions

- Keep today's embedded catalog permanently as the compiled baseline and
  automatic offline floor.
- Preserve resolution order: project override, user override, active installed
  default pack, compiled baseline.
- Install explicit immutable OCI or local-path candidates into a versioned
  user-state store and activate only after validation succeeds.
- Reuse the existing OCI artifact adapter; add no bespoke network client.
- Fall back visibly to the compiled baseline when an active installed pack later
  becomes unreadable or incompatible.
- Nest the management surface under `effigy service pack`.
- Test fixed official-channel planning, but defer public no-argument `update`
  until an official artifact exists.

## Scope

- versioned catalog-pack manifest, identity, compatibility, and source reports
- installed-pack store, atomic activation, previous-version rollback, and reset
- explicit digest-addressed OCI and local-path installation
- baseline/installed selection integrated with existing catalog resolution
- `status`, `install`, `rollback`, and `reset` text and JSON behavior
- `doctor` health and one-step repair findings
- command/help/docs/changelog/contracts/evidence parity
- representative service, workspace, extraction, and assembly regression proof

## Boundary

- no concrete catalog asset movement or separate pack repository
- no official OCI publication or live default coordinate
- no public `effigy service pack update` command
- no `.github/workflows/`, Homebrew, release archive, installer, or release
  mutation
- no implicit network access during ordinary command execution
- no change to project/user override precedence
- no S3, Rhai-provider, extension-transport, or command-removal work

## Cards

- [ ] [`1095`](./batch-cards/1095-prototype-catalog-pack-acquisition.md) — ready

## Acceptance

- baseline-only installs preserve current catalog behavior with no local store,
  OCI tool, or network requirement
- valid explicit packs install transactionally and report identity, version,
  compatibility, source, and immutable content identity
- failed candidates never replace the active installed pack
- later unhealthy active state selects the baseline visibly and diagnostically
- rollback and reset are deterministic and preserve override precedence
- no-argument official-channel planning is testable without exposing a dead
  public command
- focused and full repository validation pass

## Next Task

Execute ready card
[`1095`](./batch-cards/1095-prototype-catalog-pack-acquisition.md). After merge,
return to planning for official pack publication and concrete-asset cutover;
that follow-up requires a real OCI coordinate and explicit workflow-edit
authority.
