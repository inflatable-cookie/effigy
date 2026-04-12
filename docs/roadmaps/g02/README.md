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
- [`003-demo-harness-model-and-runner-contract.md`](./003-demo-harness-model-and-runner-contract.md) (in progress; defines first-class demo proof, runner semantics, coverage/gap model, and TUI browser contract, with registry/inspection, run foundation, lifecycle control, browser-state/query polish, self-hosted proof demos, browser list/detail foundation, browser artifact-opening affordances, bounded browser live log visibility, in-browser query controls, detail-pane navigation, metadata-query parity, first-browser cleanup, bounded persisted attempt history, a dedicated `demo history` query surface, historical-attempt drilldown, bounded history-query controls, an integrated one-demo browser history view, a runner-owned active demo terminal/session handoff, a bounded browser terminal view, a bounded active-terminal input contract, direct attached terminal sessions for human-launched interactive demos, PTY-backed demo terminal/session semantics, bounded live browser terminal consumption, bounded demo-scoped browser tabs, panel-first browser navigation, embedded browser terminal emulation with real input handoff, runner-owned active terminal resize semantics, and bounded runtime backend capability reporting now shipped; the next slice is concurrent-runner demo session projection)

Active strict planning lane:

- [`../specs/003-demo-harness-model-and-runner-strict-lane.md`](../specs/003-demo-harness-model-and-runner-strict-lane.md)
- [`../specs/batch-cards/066-implement-demo-concurrent-runner-session-projection.md`](../specs/batch-cards/066-implement-demo-concurrent-runner-session-projection.md)

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items that represent a fresh product cycle should start in `g02`
- continue numbering in `g02/` until another manual rollover is justified

## Next Task

Use the active `g02.003` ready card to project concurrent-runner-backed demos
through the demo session contract.
