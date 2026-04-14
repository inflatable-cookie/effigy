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
- [`003-demo-harness-model-and-runner-contract.md`](./003-demo-harness-model-and-runner-contract.md) (complete; shipped and released in `v0.2.13`, including the demo registry, browser, live terminal, query/history surfaces, concurrent-runner projection, and real-project consumer proof)
- [`004-rust-native-scripting-surface-contract.md`](./004-rust-native-scripting-surface-contract.md) (in progress; the scripting policy split, Rhai v1 boundary, script-step foundation, and first Effigy dogfooding cluster are now shipped, with the next slice narrowed to bounded long-running lifecycle support)

Active strict planning lane:

- [`../specs/004-rust-native-scripting-strict-lane.md`](../specs/004-rust-native-scripting-strict-lane.md)
- [`../specs/batch-cards/091-implement-rhai-long-running-lifecycle-support.md`](../specs/batch-cards/091-implement-rhai-long-running-lifecycle-support.md)

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items that represent a fresh product cycle should start in `g02`
- continue numbering in `g02/` until another manual rollover is justified

## Next Task

Use the active `g02.004` ready card to add the bounded long-running Rhai
lifecycle support exposed by the shipped Effigy dogfooding results before any
cross-repo pilot.
