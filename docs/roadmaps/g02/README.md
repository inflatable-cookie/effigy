# Roadmap g02

`g02` is the current Effigy roadmap generation.

Generation theme:

- start the next product-shaping cycle from a clean sequence instead of
  extending `g01` indefinitely
- use `g02` for new command surfaces and architectural direction that are
  meaningfully beyond the original implementation and consolidation waves

Current milestones:

- [`001-bootstrap-command-and-clone-contract.md`](./001-bootstrap-command-and-clone-contract.md) (complete; built-in released and live-pilot validated on `loophole` and `songsprout`)
- [`002-manifest-composition-and-override-contract.md`](./002-manifest-composition-and-override-contract.md) (in progress; composition foundation and inspectability are now real product surface and no longer block downstream planning)
- [`003-demo-harness-model-and-runner-contract.md`](./003-demo-harness-model-and-runner-contract.md) (in progress; defines first-class demo proof, runner semantics, coverage/gap model, and TUI browser contract, with registry/inspection, run foundation, lifecycle control, browser-state/query polish, self-hosted proof demos, browser list/detail foundation, browser artifact-opening affordances, bounded browser live log visibility, and in-browser query controls now shipped; the next slice is bounded detail-pane navigation)

Active strict planning lane:

- [`../specs/003-demo-harness-model-and-runner-strict-lane.md`](../specs/003-demo-harness-model-and-runner-strict-lane.md)
- [`../specs/batch-cards/028-implement-demo-browser-detail-navigation.md`](../specs/batch-cards/028-implement-demo-browser-detail-navigation.md)

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items that represent a fresh product cycle should start in `g02`
- continue numbering in `g02/` until another manual rollover is justified

## Next Task

Use the active `g02.003` ready card to implement bounded detail-pane
navigation, while broader stoppability stays deferred behind a runtime-handle
boundary.
