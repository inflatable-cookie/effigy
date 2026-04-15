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
- [`006-colima-container-environment-contract.md`](./006-colima-container-environment-contract.md) (paused; the first bounded container foundation, attached-session widening, repo-owned task composition, and real-machine live-stop hardening are now shipped strongly enough to pause on a trustworthy v1 boundary)
- [`007-distribution-release-and-consumer-rollout.md`](./007-distribution-release-and-consumer-rollout.md) (in progress; release closure is active again after the modularization prerequisite was met)
- [`008-demo-and-manifest-import-rollout.md`](./008-demo-and-manifest-import-rollout.md) (planned; complete manifest-import adoption and demo rollout across the intended repo cohort)
- [`009-vault-backed-varlock-rollout.md`](./009-vault-backed-varlock-rollout.md) (planned; turn the shipped env-schema/varlock foundation into a vault-backed consumer rollout program)
- [`010-effigy-modularization-and-crate-boundaries.md`](./010-effigy-modularization-and-crate-boundaries.md) (paused; the backbone plus major domain crate seams are now extracted strongly enough to stop blocking `v0.3` release closure)

Active strict planning lane:

- [`../specs/007-distribution-release-and-consumer-rollout-strict-lane.md`](../specs/007-distribution-release-and-consumer-rollout-strict-lane.md)
- active ready card:
  [`../specs/batch-cards/115-implement-effigy-distribution-release-closure.md`](../specs/batch-cards/115-implement-effigy-distribution-release-closure.md)

Paused modularization lane:

- [`../specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`](../specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md)

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items that represent a fresh product cycle should start in `g02`
- continue numbering in `g02/` until another manual rollover is justified

## Next Task

Execute `115` to carry the active release lane through bounded release
closure.
