# 024 Implement Demo Browser Live Log Visibility

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Add bounded live log visibility to `effigy demo browser` so an operator can see
recent runner-owned output for the selected demo without leaving the browser.

## In Scope

- show recent log output for the selected demo inside the browser
- support both active attempts and latest terminal attempt output when available
- keep the surface bounded around recent proof output rather than terminal
  emulation
- keep the browser state honest when no logs are available

## Out Of Scope

- terminal emulation
- arbitrary process stdin interaction
- generic runtime cancellation expansion
- multi-attempt history browsing
- desktop-client work

## Acceptance Criteria

- the browser shows a bounded recent-output view for the selected demo
- active demos expose current runner-owned output when available
- completed demos expose latest known output when available
- missing-log cases are surfaced honestly instead of showing empty fake panes

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch turns into terminal emulation
- the batch reopens broader runtime cancellation questions
- the batch requires multi-attempt history to feel coherent

## Next Task

Decide the next bounded browser follow-up through
[`025-decide-demo-post-live-log-follow-up-boundary.md`](./025-decide-demo-post-live-log-follow-up-boundary.md).
