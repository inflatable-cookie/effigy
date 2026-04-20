# 300 Prove Managed Dev Front Door In One Real Project

Status: complete
Updated: 2026-04-18
Roadmap: `g02.013`
Spec: `docs/specs/013-dev-front-door-and-managed-lifecycle-strict-lane.md`

## Objective

Close `g02.013` on a trustworthy boundary by proving one repo-owned managed
dev task can replace the current multi-command startup routine in one real
project.

## In Scope

- configure one real project to use the shipped managed dev-task contract
- prove lifecycle ownership, shell access, readiness UX, and gateway auto-start
  through that real project path
- capture any bounded proof-exposed fixes needed to keep the product path
  honest
- refresh the lane front doors for clean closeout if the proof succeeds

## Out Of Scope

- new broad dev-front-door features beyond proof-exposed fixes
- widening into another roadmap lane

## Acceptance Criteria

- one real project can launch its dev environment through the shipped managed
  dev front door alone
- the proof exercises lifecycle, shell, readiness, and gateway behavior on the
  intended product path
- any proof-exposed fix stays bounded to making the shipped contract honest
- docs/tests/output surfaces leave `g02.013` ready for clean closeout

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Outcome

This batch landed through `/Users/tom/Dev/projects/underlay-reference`.

What the proof established:

- `underlay-reference` now uses one repo-owned managed `dev` front door with
  a workspace-backed container binding plus managed lifecycle, shell,
  readiness, and gateway behavior on the shipped Effigy contract
- `effigy dev` starts the resolved workspace container, auto-starts the gateway,
  waits for the container health gate, and exposes the managed shell/runtime
  tabs on the real product path
- while the TUI is live, `effigy gateway status --json` reports the managed
  route for `underlay-reference.test -> 127.0.0.1:8025` and
  `effigy container status --json` reports the container environment as
  `health: ready`
- on shutdown, every managed process exits cleanly and the runtime removes the
  route plus stops the named container environment

What the proof exposed and fixed in-batch:

- the consumer repo still carried a stale local `DATABASE_URL` for
  `acme-api`, which caused the real proof run to fail on Postgres auth even
  though the managed runtime path itself was behaving correctly
- the proof batch fixed that repo-owned drift by updating the local/example API
  database URL and README wiring to match the compose-owned Postgres service

`g02.013` is now complete on a trustworthy product boundary.

## Next Task

No further execution lives on this lane. Stop in planning and choose which
remaining `g02` lane should resume next.
