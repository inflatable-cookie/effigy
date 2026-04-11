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
- [`003-demo-harness-model-and-runner-contract.md`](./003-demo-harness-model-and-runner-contract.md) (in progress; defines first-class demo proof, runner semantics, coverage/gap model, and TUI browser contract, with registry/inspection, run foundation, lifecycle control, browser-state/query polish, self-hosted proof demos, browser list/detail foundation, browser artifact-opening affordances, bounded browser live log visibility, in-browser query controls, detail-pane navigation, metadata-query parity, first-browser cleanup, bounded persisted attempt history, a dedicated `demo history` query surface, and historical-attempt drilldown now shipped; the next slice is to add bounded history-query narrowing and selection ergonomics without widening list or browser density by default)

Active strict planning lane:

- [`../specs/003-demo-harness-model-and-runner-strict-lane.md`](../specs/003-demo-harness-model-and-runner-strict-lane.md)
- [`../specs/batch-cards/039-implement-demo-history-query-controls.md`](../specs/batch-cards/039-implement-demo-history-query-controls.md)

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items that represent a fresh product cycle should start in `g02`
- continue numbering in `g02/` until another manual rollover is justified

## Next Task

Use the active `g02.003` ready card to add bounded history-query narrowing and
selection ergonomics, while broader runtime expansion stays deferred.
