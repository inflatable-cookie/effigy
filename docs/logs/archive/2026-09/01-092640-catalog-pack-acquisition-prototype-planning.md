# Catalog-Pack Acquisition Prototype Planning

Status: complete
Created: 2026-09-01
Roadmap: g08.040
Batch: 1095 planning

## Summary

- resolved the catalog-pack acquisition decisions left open by architecture
  `026` and contract `043` through an operator-led intent round
- promoted a permanent compiled baseline, explicit OCI/local installation,
  transactional activation, visible fallback, rollback/reset, and fixed-channel
  trust boundaries
- separated the in-repository acquisition prototype from official publication,
  concrete-asset movement, and release/install wiring
- made card `1095` ready with eight adversarial counterexamples and one bounded
  worker runway
- moved unresolved S3, Rhai-provider, extension-transport, and future-provider
  evidence questions into a residual triage note

## Operator Decisions

- The compiled baseline remains permanently for zero-ceremony offline behavior,
  including source installs.
- Independent updates use OCI through the existing artifact transport; local
  path remains available for development and recovery.
- Normal commands never fetch or check for updates.
- Failed candidates preserve the prior active pack. Later-unhealthy active state
  falls back visibly to baseline with structured diagnostics and doctor repair.
- The baseline owns the fixed official repository/channel and installed content
  cannot redirect it.
- The first lane stops at the in-repository prototype.
- Public no-argument `service pack update` waits for the official artifact so it
  is not released as a guaranteed failure.
- Review returned the previously reserved retention choice to the operator. The
  prototype retains every successfully installed pack entry and performs no
  automatic pruning; garbage collection or bounded retention waits for a later
  explicit decision.

## Readiness

- owner and seams: catalog model/resolver, artifact adapter, service routing,
  user-state activation, and doctor health
- public shapes: `status`, explicit OCI/local `install`, `rollback`, `reset`
- acceptance: baseline parity, precedence, transactional failure, visible
  fallback, deterministic recovery, fixed-source trust, and no runtime network
- stop conditions: publication/release, implicit networking, general extension
  transport, signing-policy expansion, asset movement, and S3
- parallel safety: no open Effigy PR and no separate active Effigy worker owns
  the catalog/service/artifact seam

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`, `SCALE`
- Movement: unresolved asset-acquisition policy -> one bounded, falsifiable
  acquisition prototype with unchanged common-path usage
- Remaining gap: official OCI publication, concrete catalog ownership movement,
  install/release wiring, and the public no-argument update command remain a
  separate planning lane

## Validation Performed

- current `main` and open PR state inspected
- architecture `026`, contract `043`, catalog resolver/layering, service command,
  existing OCI artifact adapter, distribution paths, and current strict-lane
  authority inspected
- no open Effigy PR found
- `git diff --check` and `effigy qa:docs` required before dispatch

## Next Task

Dispatch card `1095` through the committed worker handoff. After accepted merge,
return to planning for official pack publication and concrete-asset cutover;
do not infer workflow-edit authority.
