# Roadmaps

Roadmaps are executable milestone plans derived from Effigy vision and architecture.

## Generation model

- Use generation folders: `g01`, `g02`, `g03`.
- Use milestone files inside each generation: `NNN-<slug>.md`.
- Reference milestones as `gNN.NNN`.
- Trigger generation rollover manually; do not use automatic file-count limits.
- Treat generations as substantial sequencing eras, not one-or-two-file
  buckets. As a healthy default, expect roughly 20 to 40 roadmap files in one
  generation before rollover is even worth discussing.
- Treat rollover as full generation closeout, not a convenience reset:
  close, supersede, or rehome every roadmap in the current generation first,
  then purge stale generation-specific specs and batch cards from
  `docs/specs/` before opening the next generation.

## Layout

- `g03/` current roadmap generation
- `g02/` previous release and local-runtime expansion generation
- `g01/` original implementation and consolidation generation
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
- `g01/020-research-phase-1-core-execution.md` is complete.
- `g01/021-research-phase-2-developer-experience.md` is complete.
- `g01/022-research-phase-3-scale-and-integration.md` is complete.
- `g01/023-builtin-test-suite-lifecycle-and-env.md` is complete.
- `g01/024-release-pipeline-validation-and-consumer-ci.md` is complete.
- `g01/027-release-orchestration-system.md` is complete.
- `g01/029-northstar-effigy-consumer-adoption-kit.md` is complete.
- `g01/028-script-surface-reduction-and-builtins.md` reduces repo shell logic into Effigy-native command surfaces and is complete.
- `g02/001-bootstrap-command-and-clone-contract.md` started the new generation with a stateless bootstrap built-in for repo acquisition and environment bring-up, and is now complete: released plus live-pilot validated on `loophole` and `songsprout`.
- `g02/002-manifest-composition-and-override-contract.md` is complete; the general split-manifest model, override contract, and inspectability surface are now shipped.
- `g02/003-demo-harness-model-and-runner-contract.md` defines first-class demo proof and the runner/browser semantics that should sit inside Effigy; the lane shipped and released in `v0.2.13`, including registry loading, inspection, lifecycle control, one-demo history, attached and PTY-backed terminal runner semantics, browser demo tabs, browser terminal replay/input/resize consumption, concurrent-runner session plus interaction projection, browser-owned live attached terminal sessions for browser-launched run-backed interactive demos, bounded single-process concurrent-runner browser live-session parity, runner-owned concurrent-runtime projection-shape truth, projected-runtime process summary truth, and projected-output provenance truth.
- `g02/004-rust-native-scripting-surface-contract.md` is paused after the shipped Rhai foundation, Effigy dogfooding, and native distribution cutover reached a clean internal boundary.
- `g02/005-optional-distribution-surface-contract.md` is paused after one real consumer proof plus bounded widening made the optional distribution boundary trustworthy for metadata validation, artifact validation, and closeout evidence reuse.
- `g02/006-colima-container-environment-contract.md` is now paused after the real-machine `colima nerdctl` live-stop and closeout path was hardened strongly enough to stop carrying a deferred warning.
- `g02/020-multi-project-gateway-expansion-and-service-dns.md` is complete.
- `g02/007-distribution-release-and-consumer-rollout.md` is paused; the `v0.3.x` release work is done and any broader rollout follow-up should be re-sequenced deliberately instead of pretending the old release lane is still active.
- `g02/022-v0-3-pre-release-hardening-and-contract-cleanup.md` is complete; the bounded pre-cut hardening queue from the final `v0.3` audit landed before the `v0.3.0` cut.
- `g02/010-effigy-modularization-and-crate-boundaries.md` is complete.
- `g02/008-demo-and-manifest-import-rollout.md` remains planned, but stays outside the current `v0.3` release-prep queue.
- `g02/009-vault-backed-varlock-rollout.md` remains planned, but stays outside the current `v0.3` release-prep queue.
- `g02/017-remaining-shell-cleanup-and-crate-extraction-program.md` queues the substantial parallel cleanup jobs for the remaining heavy `/src` seams and any justified final crate splits.
- `g03/001-production-deployment-model-and-export-contract.md` is the new active milestone; it defines the provider-neutral deployment model and export contract for managed production generation.
- `g03/002-underlay-managed-deployment-export.md` is planned behind `g03.001`; Underlay is the first real target for provider export.
- `g03/003-decodelabs-production-strategy-scope.md` is planned behind `g03.002`; it scopes the future Decodelabs managed-host story without forcing premature automation now.
- `g03/007` through `g03/012` are now queued behind the current provider-export lane; they form the full common-path convergence program for execution-surface parity, repo targeting, runtime activation, interactive ownership, embedded re-entry, and drift guards without reopening `g02`.
- research carry-forward now lives in [`docs/research/carry-forward-intake.md`](../research/carry-forward-intake.md), not as an active `g02` roadmap.

## Active Strict Lane

- `g03.001` is the active strict lane
- `311` is complete
- `313` is complete
- `315` is complete
- `316` is complete
- `317` is complete
- `318` is complete
- `319` is complete
- `320` is complete
- `321` is complete
- `322` is complete
- `323` is the current ready batch: decide the next widening seam now that
  Render and Railway both have first bounded exports

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

## Rollover guardrail

Do not open `gNN+1` while the current generation still has live roadmap files
or stale strict-lane debris in the active specs tree.

Before rollover:

- every roadmap in the closing generation must be explicitly closed, paused,
  superseded, or moved to backlog
- the roadmap front doors must agree that the old generation is no longer the
  live queue
- `docs/specs/` must be purged so only live or near-live planning artifacts
  remain in the active tree

## Next Task

Execute the next `g03.001` post-Railway widening decision batch.


## Historical language boundary

- New roadmaps and actively maintained roadmap updates must use roadmap IDs and batch language.
- Older imported roadmap bodies may retain internal `Phase X.Y` execution headings as historical record.
- Leave those historical headings alone unless that roadmap is reopened for active work, then normalize it in the same batch.

## Historical command boundary

- older roadmap bodies may retain wrapper-script names or superseded command
  spellings when they describe the implementation path that existed at the time
- treat those references as historical planning evidence, not current operator
  guidance
- active release/runtime usage should be taken from the guides, contracts, and
  current roadmap front matter rather than old roadmap body details
