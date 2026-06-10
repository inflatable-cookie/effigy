# g08.014 - Gateway Route-Table Trust Model

Status: Complete
Depends on: `g08.013`
Completed: 2026-06-10

## Goal

Give the gateway a defined trust boundary for its route table. Remediates
assessment finding 2.

Today `routes.json` ([routes.rs](../../../crates/effigy-gateway/src/routes.rs))
is a plain user-writable JSON file. The gateway daemon — which runs elevated to
bind :80/:443 and to write macOS resolver files — reads that file and proxies to
whatever `target` host:port each route names. Any local process that can write
the file can redirect a root-trusted reverse proxy to an arbitrary upstream.
Only `uninstall_direct` currently checks an "effigy-managed" marker; the read
and proxy path trusts the file wholesale. Scope is localhost dev, so severity is
moderate, but there is no provenance or integrity model and no documented threat
model.

## Planning Gap (CLOSED)

The governing contract
[`033-gateway-route-table-trust-contract.md`](../../contracts/033-gateway-route-table-trust-contract.md)
is authored and promoted (indexed in the contracts README artifacts list,
active-posture anchors, and ownership/drift table). It fixes the threat model
(single-user localhost tool; defends a non-owner local writer, not a same-UID or
root adversary), what the route table is trusted to assert (routing intent
only), the required read-path integrity mechanism (ownership/permission check +
Effigy-managed marker, mirroring the resolver-file and vault patterns), the
endorsed fail-closed behavior (refuse the untrusted table, keep last-known-good,
warn, surface in status/doctor), and out-of-scope items.

Batches B and C are now ready to compile against that contract.

## Scope (post-contract)

- enforce route-table file ownership and permission expectations before the
  elevated daemon trusts it (mirror the vault's `inspect_vault_permissions`
  posture)
- require/validate the effigy-managed provenance marker on the read path, not
  only on uninstall
- define and implement the behavior when the table fails the trust check:
  refuse to load, load read-only, or warn-and-degrade per the contract
- surface trust status in `effigy gateway` inspection output and `doctor`

## Guardrails

- do not over-engineer toward a hardened multi-tenant proxy; the contract sets
  the ceiling
- do not break the existing single-user localhost flow with friction that has no
  security payoff
- changes to load/trust behavior must match the promoted contract exactly
- no new privileged operations beyond what the daemon already performs

## Execution Plan

- [x] **Batch A — Author and promote the trust contract** (closes the planning
  gap). No code change. Landed
  [`033-gateway-route-table-trust-contract.md`](../../contracts/033-gateway-route-table-trust-contract.md)
  and indexed it in the contracts README.
- [x] **Batch B — Integrity enforcement on the read path.** Added an
  Effigy-managed provenance marker (`_managed_by`) to the route table envelope
  on `save`, plus owner-only (`0o600`) permissions. New
  [`trust`](../../../crates/effigy-gateway/src/trust.rs) module verifies
  permission (no group/other write) + marker; the daemon read path
  (`LiveRouteTable::new`/`reload` and the server watcher reload) fails closed —
  an untrusted table is refused and the last-known-good in-memory table is kept.
  Seven fixtures cover trusted/absent/missing-marker/foreign-marker/invalid-JSON/
  group-writable/owner-only-on-save.
- [x] **Batch C — Operator visibility.** `effigy gateway status` now reports
  `route_table_trust` (+ reason) in both human and JSON output, and an untrusted
  table emits a remediation warning. `effigy doctor` surfaces the same as a
  runtime diagnostic (evidence when trusted; remediation warning when not),
  injected via the runner's `DoctorRuntimePorts` so `effigy-doctor` stays
  decoupled from `effigy-gateway`. Added a route-table-trust note to the
  container-system guide.

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)
- [`033-gateway-route-table-trust-contract.md`](../../contracts/033-gateway-route-table-trust-contract.md)
  (authored and promoted in Batch A)

## Acceptance Criteria

- [x] trust contract 033 is authored, reviewed, and promoted before any code
  change
- [x] the elevated daemon validates ownership/permission and provenance before
  trusting the route table
- [x] a tampered or non-managed route table triggers the contract-defined
  failure mode (proven by fixture)
- [x] gateway inspection and `doctor` surface trust status
- [x] changelog `[Unreleased] > Added` records the integrity gate (with the
  one-time marker migration note)

## Evidence

- contract: [`033-gateway-route-table-trust-contract.md`](../../contracts/033-gateway-route-table-trust-contract.md)
- gate: `crates/effigy-gateway/src/trust.rs` (+ `trust/tests.rs`), route-table
  marker + `0o600` in `routes.rs`, trust gate wired into `LiveRouteTable` and
  the server watcher reload
- visibility: `route_table_trust` in `effigy gateway status` (human + JSON,
  `src/runner/gateway_command/mod.rs`); doctor runtime diagnostic in
  `src/runner/doctor_ports.rs`; guide note in
  [`063-container-system-guide.md`](../../guides/063-container-system-guide.md)
- validation: `cargo test -p effigy-gateway` green (126 tests, 7 new trust
  fixtures); `cargo test -p effigy gateway_command::` green (13); doctor tests
  green (52); fmt + clippy clean; `effigy` bin builds; live `gateway status`
  and `doctor` confirm the trust surfaces

## Next Task

Milestone complete. Open `g08.015` (Docs Spine Compaction) — the final milestone
of the g08.010 hardening suite.
