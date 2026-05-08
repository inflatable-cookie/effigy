# 2026-04-11 - Composition Explainability Closeout And g02.003 Activation

Roadmap: `g02.003`

## Summary

Closed the `g02.002` explainability follow-up batch and activated `g02.003` as
the active strict lane.

Composition is now usable enough that downstream planning can rely on it:

- source-aware conflict diagnostics are real product surface
- `effigy config --inspect --path <dotted.path>` gives one bounded focused
  query surface
- text and JSON inspection now expose override/source facts more legibly

That means the next real blocker is no longer manifest composition. It is the
demo-harness model itself.

## Changes

- completed the `006` composition explainability batch
- promoted `g02.003` from planned to in-progress
- opened the new active strict lane and ready card for the first demo-model
  planning batch
- refreshed the front-door planning surfaces to make `g02.003` the active lane

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `ROUTE`, `MAINT`
- Movement: baseline `composition was still the active blocker before demo
  planning could start honestly` -> current `composition is legible enough for
  downstream planning, and the demo model is now the active product question`
- Remaining gap: `the demo object model, registry boundary, runner semantics,
  and coverage model are still planning work`

## Validation Performed

- command: `cargo test`
  - result: passed
- command: `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
  - result: passed
- command: `effigy qa:docs`
  - result: passed

## Next Task

Execute the active ready card in
`docs/roadmaps/g02/batch-cards/007-decide-demo-model-boundaries-and-registry-shape.md`,
then leave the next move explicit as either runner/lifecycle semantics or
coverage/gap modeling for `g02.003`.
