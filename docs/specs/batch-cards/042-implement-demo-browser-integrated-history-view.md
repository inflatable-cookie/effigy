# 042 Implement Demo Browser Integrated History View

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Integrate one-demo retained history into the browser detail pane so operators
can review it in-place instead of leaving the browser.

## In Scope

- replace the shipped browser history handoff with an integrated detail-pane
  history mode for the selected demo
- keep the browser anchored on the settled one-demo `demo history` contract
  instead of inventing multi-demo or analytics semantics
- let detail-pane `↑` and `↓` navigate all visible actions/options in the
  pane, including actions, retained attempts, and artifacts
- leave the lane with one explicit ready card after the integrated browser
  history batch lands

## Out Of Scope

- widening into multi-demo history aggregation, analytics, or queueing
- adding `demo list` history density, badges, or grouped retained summaries
- broader runtime cancellation or desktop-client work

## Acceptance Criteria

- the browser no longer leaves the TUI to view one demo's retained history
- the selected demo can enter a retained-history detail mode from the action
  menu
- detail-pane `↑` and `↓` navigation covers all visible interactive entries in
  the pane rather than only artifacts
- the retained-history pane consumes the settled `demo history` contract rather
  than inventing a second history source of truth

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch widens into multi-demo history density or generic timeline tooling
- the browser change depends on `demo list` history summaries to feel coherent
- the integrated view drifts away from the settled one-demo `demo history`
  contract

## Next Task

Execute [`043-decide-demo-post-integrated-browser-history-boundary.md`](./043-decide-demo-post-integrated-browser-history-boundary.md)
to decide whether the next bounded history/browser follow-up should deepen
browser-side activation from retained attempts or return to runner/query work.
