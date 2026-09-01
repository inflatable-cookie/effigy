# 1095 - Prototype Catalog-Pack Acquisition

Roadmap: [`../040-catalog-pack-acquisition-prototype.md`](../040-catalog-pack-acquisition-prototype.md)
Spec: [`../../../specs/113-catalog-pack-acquisition-prototype-strict-lane.md`](../../../specs/113-catalog-pack-acquisition-prototype-strict-lane.md)
Architecture: [`../../../architecture/026-feature-placement-and-command-surface.md`](../../../architecture/026-feature-placement-and-command-surface.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/043-feature-placement-and-surface-migration-contract.md`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Complete
Owner: catalog-pack model, installed state, `service` routing, and doctor health
Created: 2026-09-01
Ready since: 2026-09-01 operator-confirmed acquisition prototype
Closed: 2026-09-01
Evidence: [`2026-09-01 closeout`](../../../logs/2026-09/01-095641-catalog-pack-acquisition-prototype-1095.md)

## Purpose

Implement the smallest complete catalog-pack acquisition prototype that proves
independent installed assets can coexist with a permanent compiled baseline
without changing existing catalog-backed usage.

## Work

- define a typed, versioned pack manifest and compatibility model without
  duplicating catalog fragment schema or assembly
- add a versioned Effigy user-state store with atomic active-selection metadata
  and enough retained lineage for one deterministic rollback
- resolve layers in the approved order: project, user, active installed pack,
  compiled baseline
- implement explicit digest-addressed OCI and local-path installation through a
  shared transaction: acquire, validate, store, activate
- reuse the existing OCI artifact adapter and make it injectable for focused
  proof; do not add a second transport client
- add `service pack status`, `install`, `rollback`, and `reset` parsing, typed
  help, text output, standard JSON payloads, and deterministic exits
- model the fixed official repository/channel update request at the domain and
  adapter seam without exposing public no-argument `update`
- add visible baseline fallback and doctor repair behavior for later-unhealthy
  active state
- preserve current `service list`, extraction, workspace, service, compose, and
  override behavior with representative regression proof
- update the command reference, catalog/service guide, JSON example/contract
  coverage where required, agent guidance only if behavior is relevant there,
  `CHANGELOG.md`, one evidence log, and all closeout surfaces

## Acceptance

- [x] baseline-only catalog resolution works without a pack store, `oras`, or
      network access and preserves representative current fragment bytes
- [x] project and user overrides remain above an active installed pack
- [x] valid local and digest-addressed OCI packs record identity, version,
      compatibility, source, and immutable content identity before activation
- [x] invalid/incompatible/failed candidates leave active metadata and the
      previous validated pack unchanged
- [x] later-unreadable or incompatible active state falls back visibly to the
      baseline with equivalent text/JSON facts and one-step doctor repair
- [x] rollback selects the previous validated installed pack; reset selects the
      compiled baseline without deleting project/user overrides
- [x] installed content cannot redirect the fixed official channel model
- [x] no normal catalog-backed command invokes the OCI adapter or performs a
      network probe
- [x] no public `service pack update`, concrete asset move, release/workflow
      change, S3 change, or new general extension mechanism lands
- [x] focused catalog, artifact-adapter, CLI, runner, doctor, and representative
      assembly tests plus full Effigy QA pass
- [x] spec, roadmap, card, guides, contracts, changelog, evidence, and all
      current Next Task pointers close honestly

## Review Oracle

Falsified. Each row maps to exact tests in the
[closeout log](../../../logs/2026-09/01-095641-catalog-pack-acquisition-prototype-1095.md).

1. With an empty user-state root and no `oras` on `PATH`, `service list`,
   extraction, and representative compose assembly differ from the current
   compiled catalog or attempt network access.
2. Project or user override `postgres` exists beside an installed pack's
   `postgres`; the installed definition wins.
3. An OCI pull succeeds but the candidate manifest is incompatible or malformed;
   active selection changes anyway, or the prior installed directory is
   partially overwritten.
4. The active pack is deleted, corrupted, or becomes incompatible with the
   running Effigy; text silently uses baseline, JSON omits the reason, or
   `doctor` lacks a direct repair.
5. An installed pack declares a hostile alternate update source; the modeled
   official update request follows it instead of the compiled fixed channel.
6. Rollback after two successful installs selects baseline or the wrong digest;
   reset deletes project/user overrides or prevents later rollback.
7. A normal `service list`, container plan, system/workspace resolution, or task
   run invokes the OCI adapter merely to check freshness.
8. Help or JSON advertises `service pack update`, or the diff moves catalog
   assets or edits release/workflow surfaces before publication exists.

## Validation

- focused `cargo test -p effigy-catalog`
- focused `cargo test -p effigy-artifacts` where the shared adapter seam changes
- focused CLI parsing/help/output tests for `service pack`
- focused runner and doctor tests with isolated user-state roots and injected
  OCI adapters
- representative existing catalog service/workspace/assembly integration tests
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Write one dated closeout log mapping all eight oracle rows to exact tests or
commands. Record store/selection fixtures, baseline parity, transaction failure
proof, fallback diagnostics, the absence of normal-command adapter calls, full
validation, unresolved publication work, and the next planning checkpoint.

## Stop Conditions

Stop if the prototype needs a live official artifact, workflow/release edits,
implicit networking, silent fallback, destructive reset, a different override
order, a general extension store, new signing policy, concrete-asset movement,
or S3/Rhai-provider changes. Return any such choice to the orchestrator.

## Next Task

Complete. Return to planning for official pack publication and concrete-asset
cutover under contract `043`. That lane needs a real OCI coordinate and explicit
workflow-edit authority; it is not ready.
