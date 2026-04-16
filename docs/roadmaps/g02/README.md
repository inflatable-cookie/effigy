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
- [`007-distribution-release-and-consumer-rollout.md`](./007-distribution-release-and-consumer-rollout.md) (in progress; release closure is queued again while the remaining TUI shell is still being reduced)
- [`008-demo-and-manifest-import-rollout.md`](./008-demo-and-manifest-import-rollout.md) (planned; complete manifest-import adoption and demo rollout across the intended repo cohort)
- [`009-vault-backed-varlock-rollout.md`](./009-vault-backed-varlock-rollout.md) (planned; turn the shipped env-schema/varlock foundation into a vault-backed consumer rollout program)
- [`010-effigy-modularization-and-crate-boundaries.md`](./010-effigy-modularization-and-crate-boundaries.md) (in progress; the backbone plus domain crates are real, the browser/TUI seam is paused on a clean adapter boundary, the demo runner seam is paused on an honest shell boundary, the release verify-install, git-execute, and version-preview slices are now extracted, the release seam is still open, and the next move is changelog workspace extraction and release adoption)
- [`011-service-catalog-and-compose-assembly.md`](./011-service-catalog-and-compose-assembly.md) (planned; eliminate compose boilerplate by assembling docker-compose.yml from a manifest-declared service catalog with bundled, overridable fragments)
- [`012-container-context-and-transparent-execution.md`](./012-container-context-and-transparent-execution.md) (planned; mark a container as the project's execution context so task routing implicitly goes through it)
- [`013-dev-front-door-and-managed-lifecycle.md`](./013-dev-front-door-and-managed-lifecycle.md) (planned; single-command `effigy dev` front door using the managed-process concurrent runtime with embedded terminal and health gate)
- [`014-rust-native-gateway.md`](./014-rust-native-gateway.md) (planned; Rust-native DNS resolver and reverse proxy for `.test` domains with optional HTTPS via mkcert)
- [`015-persistent-data-and-volume-lifecycle.md`](./015-persistent-data-and-volume-lifecycle.md) (planned; named volume lifecycle, export/import, task-based seeding, and Rhai hooks for production data pull)
- [`016-multi-project-coordination.md`](./016-multi-project-coordination.md) (planned; automatic port allocation, cross-project status, and resource visibility)

Container infrastructure design document:

- [`../architecture/020-container-infrastructure-design.md`](../architecture/020-container-infrastructure-design.md)

Active strict planning lane:

- [`../specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`](../specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md)
- active ready card:
  [`../specs/batch-cards/181-implement-effigy-changelog-workspace-extraction-and-release-adoption.md`](../specs/batch-cards/181-implement-effigy-changelog-workspace-extraction-and-release-adoption.md)

Queued release card:

- [`../specs/batch-cards/115-implement-effigy-distribution-release-closure.md`](../specs/batch-cards/115-implement-effigy-distribution-release-closure.md)

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items that represent a fresh product cycle should start in `g02`
- continue numbering in `g02/` until another manual rollover is justified

## Next Task

Execute `181` to extract the changelog surface into its own workspace crate and
reconnect release prep through that boundary.
