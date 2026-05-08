# 022 Implement Demo Browser Artifact Affordances

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Ship the next bounded browser slice by making artifact references usable from
inside the demo browser without widening into live log streaming or embedded
terminal behavior.

## In Scope

- expose the selected demo's artifact references as first-class browser
  affordances
- support a bounded operator action to open or reveal one artifact path from
  the browser
- keep the implementation grounded in the shipped artifact references already
  surfaced by `demo inspect`
- make failure/reporting honest when an artifact path is missing or cannot be
  opened in the current environment

## Out Of Scope

- live log streaming or tailing
- embedded terminal output
- broader runtime cancellation
- multi-attempt history or artifact galleries
- desktop-client decisions

## Acceptance Criteria

- the browser can surface and act on artifact references without leaving the UI
- the action model remains bounded and platform-honest
- the batch improves proof inspection on the shipped self-hosted demos

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch turns into live log streaming or terminal emulation work
- the batch invents browser-local artifact metadata instead of using runner
  state
- the batch starts solving generic OS integration beyond one bounded open or
  reveal action

## Next Task

Decide whether live log visibility is still the next honest browser follow-up
now that artifact-opening affordances are shipped.
