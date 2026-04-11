# Roadmap g02

`g02` is the current Effigy roadmap generation.

Generation theme:

- start the next product-shaping cycle from a clean sequence instead of
  extending `g01` indefinitely
- use `g02` for new command surfaces and architectural direction that are
  meaningfully beyond the original implementation and consolidation waves

Current milestones:

- [`001-bootstrap-command-and-clone-contract.md`](./001-bootstrap-command-and-clone-contract.md) (complete; built-in released and live-pilot validated on `loophole` and `songsprout`)
- [`002-manifest-composition-and-override-contract.md`](./002-manifest-composition-and-override-contract.md) (in progress; defines general include/require/import composition plus explicit override semantics for split manifest fragments)
- [`003-demo-harness-model-and-runner-contract.md`](./003-demo-harness-model-and-runner-contract.md) (planned; defines first-class demo proof, runner semantics, coverage/gap model, and TUI browser contract)

Active strict planning lane:

- [`../specs/002-manifest-composition-and-override-strict-lane.md`](../specs/002-manifest-composition-and-override-strict-lane.md)
- [`../specs/batch-cards/006-harden-composition-explainability-and-operator-ux.md`](../specs/batch-cards/006-harden-composition-explainability-and-operator-ux.md)

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items that represent a fresh product cycle should start in `g02`
- continue numbering in `g02/` until another manual rollover is justified

## Next Task

Use the active `g02.002` explainability batch to harden the operator-facing
composition surface next. Keep `g02.003` planned but inactive until split-config
and inspection are both real product surface instead of planning-only doctrine.
