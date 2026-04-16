# Roadmaps

Roadmaps are executable milestone plans derived from Effigy vision and architecture.

## Generation model

- Use generation folders: `g01`, `g02`, `g03`.
- Use milestone files inside each generation: `NNN-<slug>.md`.
- Reference milestones as `gNN.NNN`.
- Trigger generation rollover manually; do not use automatic file-count limits.

## Layout

- `g02/` current roadmap generation
- `g01/` previous implementation and consolidation generation
- `generation-index.md` active generation and rollover history
- `backlog/` deferred scope with promotion criteria

## Current queue

- `g01/001` through `g01/012` capture the delivered Effigy implementation baseline.
- `g01/013-effigy-northstar-doctrine-alignment.md` records the docs structure migration and is complete.
- `g01/014-attention-marker-scan-and-doctor-integration.md` is complete.
- `g01/015-effigy-self-hosting-and-agent-first-adoption.md` is complete.
- `g01/016-duplicate-blocks-scan-and-doctor-integration.md` is complete.
- `g01/017-comment-ratio-scan-and-doctor-integration.md` is complete.
- `g01/018-generated-in-src-scan-and-doctor-integration.md` is complete.
- `g01/019-stale-suppressions-scan-and-doctor-integration.md` is complete.
- `g01/020-research-phase-1-core-execution.md` is the first research roadmap (planned).
- `g01/021-research-phase-2-developer-experience.md` is the second research roadmap (planned).
- `g01/022-research-phase-3-scale-and-integration.md` is the third research roadmap (planned).
- `g01/023-builtin-test-suite-lifecycle-and-env.md` is the next implementation roadmap (planned).
- `g01/024-release-pipeline-validation-and-consumer-ci.md` validates the release pipeline and delivers consumer CI integration (complete).
- `g01/029-northstar-effigy-consumer-adoption-kit.md` is the consumer-adoption and product-boundary consolidation roadmap.
- `g01/028-script-surface-reduction-and-builtins.md` reduces repo shell logic into Effigy-native command surfaces and is complete.
- `g02/001-bootstrap-command-and-clone-contract.md` started the new generation with a stateless bootstrap built-in for repo acquisition and environment bring-up, and is now complete: released plus live-pilot validated on `loophole` and `songsprout`.
- `g02/002-manifest-composition-and-override-contract.md` defines the general split-manifest model so features do not invent their own file-loading semantics, and its foundation plus inspectability are already shipped.
- `g02/003-demo-harness-model-and-runner-contract.md` defines first-class demo proof and the runner/browser semantics that should sit inside Effigy; the lane shipped and released in `v0.2.13`, including registry loading, inspection, lifecycle control, one-demo history, attached and PTY-backed terminal runner semantics, browser demo tabs, browser terminal replay/input/resize consumption, concurrent-runner session plus interaction projection, browser-owned live attached terminal sessions for browser-launched run-backed interactive demos, bounded single-process concurrent-runner browser live-session parity, runner-owned concurrent-runtime projection-shape truth, projected-runtime process summary truth, and projected-output provenance truth.
- `g02/004-rust-native-scripting-surface-contract.md` is paused after the shipped Rhai foundation, Effigy dogfooding, and native distribution cutover reached a clean internal boundary.
- `g02/005-optional-distribution-surface-contract.md` is paused after one real consumer proof plus bounded widening made the optional distribution boundary trustworthy for metadata validation, artifact validation, and closeout evidence reuse.
- `g02/006-colima-container-environment-contract.md` is now paused after the real-machine `colima nerdctl` live-stop and closeout path was hardened strongly enough to stop carrying a deferred warning.
- `g02/007-distribution-release-and-consumer-rollout.md` remains in progress, but release closure is queued again while the remaining TUI shell is still being reduced.
- `g02/010-effigy-modularization-and-crate-boundaries.md` is active again because the remaining shell seams are still not clean enough for release; the CLI shell slice, TUI/browser slices, the demo seam, and the changelog workspace seam are now extracted, the release seam is narrower again, and the next move is the post-changelog boundary decision.
- `g02/008-demo-and-manifest-import-rollout.md` queues the remaining demo and manifest-import adoption work across the intended cohort.
- `g02/009-vault-backed-varlock-rollout.md` queues the vault-backed rollout for the shipped env-schema / varlock foundation.

## Active Strict Lane

- `g02.010`
- active ready card: `182-decide-post-changelog-workspace-extraction-boundary.md`
- queued release card: `115-implement-effigy-distribution-release-closure.md`

## Research Program

Three-phase comparative research program:
- **Phase 1 (020)**: Core Execution — Configuration, caching, watch mode, DAG, TUI
- **Phase 2 (021)**: Developer Experience — Completions, errors, workspaces, portability
- **Phase 3 (022)**: Scale & Integration — Remote execution, CI/CD, IDE, plugins, telemetry

See `docs/research/README.md` for the research operating model.

## Backlog

Deferred roadmap items live in [backlog/README.md](./backlog/README.md).

## Batch and logging rule

- Execute milestones in meaningful batches.
- Create logs per completed batch or update cycle, not per individual task.

## Next Task

Execute `182` to decide whether the remaining release shell is now honest
enough to pause or still needs one final bounded extraction.


## Historical language boundary

- New roadmaps and actively maintained roadmap updates must use roadmap IDs and batch language.
- Older imported roadmap bodies may retain internal `Phase X.Y` execution headings as historical record.
- Leave those historical headings alone unless that roadmap is reopened for active work, then normalize it in the same batch.
