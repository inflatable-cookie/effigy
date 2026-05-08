# 018 - OCI Artifact Closeout And Proof Matrix

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-08
Depends on:
- [`014-data-seed-dump-plan-consumption.md`](./014-data-seed-dump-plan-consumption.md)
- [`015-container-volume-operation-pipeline.md`](./015-container-volume-operation-pipeline.md)
- [`017-planning-crate-decomposition.md`](./017-planning-crate-decomposition.md)

## Goal

Move OCI artifact support from usable substrate to a finished,
operator-credible Effigy surface.

The missing work is not core parsing or transport anymore. It is proof,
failure handling, and contract closeout:

- prove the OCI path end to end across bootstrap, container data, and artifact
  commands
- make auth and push failures actionable instead of adapter-shaped
- decide what “artifact operation record” is actually shipped now versus
  explicitly deferred
- close the remaining wording and contract drift so OCI support reads as a
  stable product surface rather than an experimental side path

## Scope

- add a focused OCI proof matrix for:
  - `artifact inspect` local vs `oci://`
  - `artifact stage` local vs `oci://`
  - `artifact capture --push`
  - `bootstrap --db-seed ...=oci://...`
  - `container data seed --db-seed ...=oci://...`
  - `container data dump ...=oci://...`
  - `container data dump ...=oci://... --push`
- harden auth and push failure reporting around the live `oras` path
- decide and document the current artifact operation record/ledger boundary
- update guides/contracts/help so shipped OCI support is described with the
  same precision as the runtime/container surfaces

## Non-Goals

- no generic migration framework
- no registry credential management UI
- no background sync or automatic publish behavior
- no production deployment orchestration
- no `.github/workflows/` edits
- no release execution

## Definition Of Done

OCI support is considered done for this generation only when:

- local and OCI seed inputs resolve through the same staged artifact contract
- local and OCI dump destinations resolve through the same explicit capture
  contract
- push remains explicit and digest-reporting remains stable
- private-registry auth failures produce operator-actionable remediation
  guidance
- contract/docs/help no longer describe OCI as merely “first round” support
  unless a specific capability is still intentionally deferred
- the shipped artifact operation record boundary is explicit:
  - either the ledger/report surface is part of the supported contract now
  - or the contract explicitly says it is deferred and names the remaining gap

## Closed Gaps

This milestone closed the remaining OCI gaps that had kept the surface short of
done:

- command-level proof coverage now exists for the shipped artifact and
  dump-to-OCI paths, with the remaining shared-seed seam made explicit
- auth and push failures now produce operator-actionable remediation with
  contract-tested output
- dump-to-OCI and direct artifact capture now share one explicit push model
- the contract now says clearly that operation reports are shipped today while
  a durable persisted artifact ledger is deferred

## Acceptance Criteria

- OCI proof matrix exists and is green on focused command-level coverage
- fake-adapter tests and live-shaped runner tests both cover push and auth
  failure surfaces
- docs and help point operators at the real prerequisites and remediation path
- `014-artifact-substrate-contract.md` matches the actual shipped OCI support
  level
- the strict lane closes with no open ready card and no ambiguous OCI
  provisional language left in the current roadmap set

## Validation

- `cargo test -p effigy-artifacts`
- `cargo test -p effigy-data`
- `cargo test -p effigy bootstrap_option_tests --lib -- --nocapture`
- `cargo test -p effigy catalog_and_container_option_tests --lib -- --nocapture`
- targeted runner tests for artifact, bootstrap DB seed, and container data
  dump/seed OCI flows
- `./target/debug/effigy docs check-paths docs/contracts/014-artifact-substrate-contract.md docs/guides/063-container-system-guide.md docs/guides/072-artifact-commands-guide.md CHANGELOG.md`
- `git diff --check`

## Next Task

Planning stop.
