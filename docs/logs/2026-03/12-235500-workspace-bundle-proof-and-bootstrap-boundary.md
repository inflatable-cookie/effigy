# Workspace Bundle Proof and Bootstrap Boundary

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: workspace-bundle-proof-and-bootstrap-boundary

## Summary

Closed the last open Wave 3 proof and made the Wave 5 bootstrap decision.

The finished starter contract bundle is now proven in both of the main starter
shapes:

- neutral single-repo fixture
- thin workspace root with a nested docs-authority repo

That was enough to make the product-boundary call: bootstrap scaffolding should
stay in the `northstar-effigy` skill/templates for now, not move into an
Effigy-native `init` surface yet.

## Changes

- added a workspace-container CLI fixture that proves:
  - thin root-level `qa:docs` / `qa:northstar` delegation
  - nested docs-authority starter bundle
  - starter docs-policy config
  - root/docs-spine/agent/front-door drift checks
- updated the adoption roadmap to mark Wave 3 complete
- recorded the current bootstrap boundary explicitly:
  - Effigy owns reusable validation engines
  - Northstar owns starter scaffolding and template branching

## Decision

Do not productize bootstrap scaffolding into Effigy yet.

Reason:

- the current branching logic is still fundamentally repo-shape aware:
  - single repo
  - workspace root plus nested docs authority
  - releasable repo versus docs-only authority
- that branching is handled more cleanly in the `northstar-effigy` skill and
  template pack than it would be in a narrow built-in `init` surface right now
- the repeated friction we found was mostly about validation engines, and those
  have now been productized where justified:
  - docs-policy support
  - `check-next-action`
  - `check-forbidden`
  - `check-paths`

Reopen an Effigy-native bootstrap surface only if later adoption shows clear,
repeated pain that the current skill/templates cannot cover cleanly.

## Validation

Validated with focused coverage:

- `cargo test --test cli_output_tests cli_workspace_container_starter_bundle_passes_via_nested_docs_authority`
- `cargo run --bin effigy -- docs check-links docs/logs/README.md docs/logs/2026-03/12-235500-workspace-bundle-proof-and-bootstrap-boundary.md docs/roadmaps/g01/029-northstar-effigy-consumer-adoption-kit.md`
- `cargo run --bin effigy -- docs check-index --dir docs/logs --index docs/logs/README.md`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: baseline `starter bundle and validation were proven, but the
  completed workspace-container shape and bootstrap ownership boundary were
  still open` -> current `starter bundle proof now covers both main adoption
  shapes, and bootstrap scaffolding is explicitly kept in the skill/template
  layer rather than moved into Effigy product surface`
- Remaining gap: none in Wave 3; future work is only a later Wave 5
  reconsideration if new adoption friction appears

## Next Task

Treat the consumer adoption kit as complete for now and move future work to
observed friction only: do not build an Effigy-native bootstrap/init surface
unless later repo migrations show a repeatable gap that the current
`northstar-effigy` skill cannot cover.
