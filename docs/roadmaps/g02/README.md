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
- [`004-rust-native-scripting-surface-contract.md`](./004-rust-native-scripting-surface-contract.md) (paused; the scripting policy split, Rhai v1 boundary, script-step foundation, long-running lifecycle support, release-wrapper convergence, and native distribution cutover are shipped strongly enough to pause while external pilots are deferred)
- [`005-optional-distribution-surface-contract.md`](./005-optional-distribution-surface-contract.md) (paused; the optional manifest-driven distribution surface is now proven strongly enough for cross-repo metadata validation, artifact validation, and closeout evidence reuse, while the fuller published-consumer `first-publish` question stays explicitly deferred)

Active strict planning lane:

- [`../specs/005-optional-distribution-surface-strict-lane.md`](../specs/005-optional-distribution-surface-strict-lane.md)
- no active ready card; `g02.005` is paused on its current boundary

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items that represent a fresh product cycle should start in `g02`
- continue numbering in `g02/` until another manual rollover is justified

## Next Task

Keep `g02.005` paused until a real published-consumer need justifies
reopening the deferred `distribution first-publish` question.
