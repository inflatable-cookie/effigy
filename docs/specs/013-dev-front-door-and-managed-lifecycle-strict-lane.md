# 013 Dev Front Door And Managed Lifecycle Strict Lane

Status: complete
Updated: 2026-04-18
Roadmap: `g02.013`

## Context

The container, exec, gateway, persistent-data, and coordination lanes are now
shipped on bounded product surfaces. What is still missing is the repo-level
developer front door that composes those pieces into one honest daily-driver
loop.

This lane owns that final aggregator move. It should not reopen the lower
container or gateway contracts. It should turn the shipped substrate into one
repo-owned managed dev task path.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/013-dev-front-door-and-managed-lifecycle.md`
- `docs/architecture/020-container-infrastructure-design.md`

## Lane Focus

This lane owns:

- the repo-owned `effigy dev` task contract on top of the managed-process
  runtime
- `tasks.<name>.managed` metadata for dev-front-door behavior
- container lifecycle ownership inside a managed concurrent runtime
- bounded shell, health-gate, and gateway follow-through only after the
  lifecycle foundation is real
- one real-project proof that the shipped path can replace the current
  multi-command startup routine

## Current Posture

`complete`

Shipped substrate that this lane builds on:

- normal task dispatch already supports managed concurrent runtimes
- task routing already supports `container_session = "<name>"` on repo-owned
  tasks
- the container lane already owns attached session lifecycle, environment
  health checks, and primary-service shell execution
- the gateway lane already owns startup, route registration, status, and TLS
  on the bounded product path
- the coordination lane already owns shared route, port, and status surfaces

## Integration Constraint

This lane should start with the narrowest trustworthy dev-front-door slice:

- make managed dev-task metadata and container lifecycle ownership real before
  widening into shell embedding or gateway automation
- keep repo-owned task convention explicit; do not turn `dev` into a special
  built-in command
- treat embedded shell tabs, health-gate readiness messaging, and gateway
  auto-start as separate follow-up decisions unless the first batch proves they
  belong together

## Remaining Integration Work

The bounded continuation chain ran through:

1. `291` — plan the first `g02.013` execution batch on the shipped managed,
   container, and gateway substrate
2. `292` — first execution batch: managed dev-task metadata plus container
   lifecycle ownership foundation
3. `293` — decide the first post-lifecycle follow-up now that managed
   dev-task ownership is real
4. `294` — shell-role foundation through the shipped primary-service container
   shell path
5. `295` — decide the first post-shell-role follow-up now that the embedded
   shell path is real
6. `296` — readiness UX foundation through the shipped task-owned container
   health path
7. `297` — decide the first post-readiness follow-up now that readiness UX is
   real
8. `298` — gateway auto-start foundation on top of the shipped gateway lane
   and task-owned container session
9. `299` — decide the final post-gateway follow-up now that the bounded
   front-door contract is fully shipped
10. `300` — one real-project proof that the shipped managed dev front door can
   replace the current multi-command startup routine
11. lane closeout after the proof unless it exposes one bounded honest fix

What the first execution batch should make real:

- a manifest-owned `tasks.<name>.managed` section for repo-level dev-front-door
  policy
- explicit concurrent-entry roles for container lifecycle ownership instead of
  overloading generic task/process entries
- one honest product path where a repo-owned managed task can start the named
  container environment and shut it down on owner exit
- planning/render/schema coverage that makes the new dev-task contract visible
  before any shell-tab or gateway follow-up

What is now real in the product path:

- `[tasks.<name>.managed].container_lifecycle = true`
- `concurrent` lifecycle-role validation and plan rendering
- one bounded runtime path where a repo-owned managed task starts the named
  `container_session` through a managed lifecycle process and applies shutdown
  on managed-runtime exit

The final proof is now real:

- one bounded real-project proof landed through `underlay-reference`
- the proof exercised lifecycle ownership, shell embedding, readiness UX, and
  gateway auto-start together on the shipped managed dev-task contract
- the proof exposed one bounded consumer drift issue in local DB wiring, which
  was fixed in-batch without reopening the Effigy product contract

## Exit Condition

This strict lane is complete when:

- Effigy has one trustworthy repo-owned dev front door on the intended task
  contract
- the managed runtime can own the container lifecycle honestly
- any wider shell, health, gateway, or proof work is either shipped or
  explicitly deferred on a trustworthy boundary

## Next Task

No further execution lives on `g02.013`. Stop in planning and choose the next
remaining `g02` lane deliberately.
