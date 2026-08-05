# 099 - Local Dependency Management Strict Lane

Roadmap: [`g08.018`](../roadmaps/g08/018-local-dependency-management-suite.md)
Related planning:

- [`g08.019`](../roadmaps/g08/019-dependency-inventory-and-command-foundation.md)
- [`g08.020`](../roadmaps/g08/020-cargo-local-dependency-linking.md)
- [`g08.021`](../roadmaps/g08/021-bun-local-dependency-linking.md)
- [`g08.022`](../roadmaps/g08/022-dependency-link-doctor-and-hygiene.md)
- [`g08.023`](../roadmaps/g08/023-dependency-link-portfolio-proof-and-closeout.md)

Durable authority:

- [`architecture/023`](../architecture/023-local-dependency-linking-architecture.md)
- [`contract/034`](../contracts/034-local-dependency-linking-contract.md)
- [`working rules/001`](../contracts/001-working-rules.md)

Status: Complete
Owner: Platform
Created: 2026-08-05

## Purpose

Execute the `effigy deps` suite without weakening committed dependency truth or
mixing Cargo, Bun, CLI, doctor, and persistence ownership in the runner shell.

The lane begins with a shared read-only foundation. Manager mutation starts
only after package inventory, desired state, status, and JSON shapes are typed
and proven.

## Lane Posture

Posture: `strict-complete`

Current ready card:

- none

## Owner And Seam

`effigy-deps` is the dependency-domain owner. It owns manager-neutral models,
canonical identities, repo-local desired state, machine-local Bun registration
ownership, read-only inventory, plans, and verification reports.

Dependency direction:

- `effigy-deps` must not depend on `effigy-cli`, `effigy-doctor`, or the root
  runner
- `effigy-cli` owns command grammar only
- the root runner owns command dispatch and operator rendering
- `effigy-doctor` consumes a read-only inspection API from `effigy-deps`
- Cargo/Bun process mutation remains behind manager adapters added in
  `g08.020` and `g08.021`

## Hard Boundaries

- never edit `Cargo.toml` or `package.json` in consumer repos
- never invoke Bun link behavior with `--save`
- never link a partial matching closure
- never discard lockfile changes through Git restore commands
- never overwrite foreign Cargo patches or Bun registrations
- never mutate manager state from status, doctor, or `--dry-run`
- no package managers beyond Cargo and Bun in this suite
- no release mutation or workflow edit

## Execution Order

### g08.019 foundation

1. `1051` — establish the `effigy-deps` domain and state stores
2. `1052` — add read-only Cargo/Bun inventory and status inspection
3. `1053` — wire CLI/help/completion, JSON contracts, and foundation closeout

### Later milestones

4. `1054` — plan Cargo full closure and managed config safely
5. `1055` — apply Cargo links and verify the full local closure
6. `1056` — apply unlink, prove committed-source recovery, and close Cargo
7. `g08.021` — Bun link/unlink
8. `1060` through `1061` — doctor and do-not-commit hygiene
9. `1062` — Signal proof against flat and nested consumers
10. `1063` — Bun closure, drift, peer, and repair proof
11. `1064` — operator guidance and suite closeout

## Ready Chain

- `1051` is complete with its ownership/state tests green
- `1052` is complete with deterministic read-only Cargo/Bun inventory and
  observed-status tests green
- `1053` is complete; `g08.019` is closed
- `1054` is complete with full-closure and safety fixtures green
- `1055` is complete with Cargo apply, metadata/tree verification, rollback,
  CLI, and JSON proofs green
- `1056` is complete; `g08.020` is closed with reversible Cargo link/unlink,
  exact Git recovery, and owned lock-drift guards green
- `1057` is complete with deterministic Bun closure, immutable-file,
  process-intent, and registration-ownership plans green
- `1058` is complete with exact-precondition Bun link application, rollback,
  immutable manifest/lock guards, full symlink verification, CLI/JSON, and
  real Bun `1.3.14` proofs green
- `1059` is complete; `g08.021` is closed with exact Bun unlink, safe
  registration release, peer diagnostics, and real round-trip proof green
- `1060` is complete with shared Cargo/Bun health findings, exact evidence,
  remediation, peer diagnostics, and status text/JSON parity green
- `1061` is complete; `g08.022` is closed with doctor/status parity green
- `1062` is complete with flat/nested Signal closure, edit propagation,
  read-only observation, and exact unlink recovery proved in disposable clones
- `1063` is complete with save-less closure, real install drift, managed
  repair, exact peer-path diagnostics, and owned-registration cleanup proved
- `1064` is complete with operator/agent guidance, JSON examples, full QA, and
  suite/front-door closeout

## Stop Conditions

Stop and replan if:

- `effigy-deps` would need an upward dependency on CLI, doctor, or runner code
- save-less Bun behavior mutates a manifest or lockfile in supported-version
  proof
- Cargo cannot apply one repo-root config across the supported nested workspace
  shape
- full direct/transitive closure cannot be established deterministically
- safe Bun registration ownership requires overwriting or claiming foreign
  state
- the JSON payload cannot remain additive under `effigy.command.v1`

## Acceptance

This lane is complete when:

- [x] `g08.019` through `g08.023` are complete
- [x] Cargo and Bun links are full-closure, reversible, and dry-runnable
- [x] status and doctor share one observed-state model
- [x] active local links never require committed manifest changes
- [x] Cargo lock and Bun manifest/lock hygiene failures are actionable
- [x] Signal flat/nested consumer proof and Bun closure/drift proof are recorded
- [x] no active ready card remains

## Next Task

Select the next substantial g08 scope separately. Do not infer a release or
generation rollover.
