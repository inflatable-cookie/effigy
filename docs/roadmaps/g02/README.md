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
- [`010-effigy-modularization-and-crate-boundaries.md`](./010-effigy-modularization-and-crate-boundaries.md) (in progress; the backbone plus domain crates are real, the browser/TUI seam is paused on a clean adapter boundary, the demo runner seam is paused on an honest shell boundary, the changelog workspace seam and final release review/text-projection seam are now extracted, `effigy-contracts` is now real, the contracts seam is now paused on an honest adapter boundary, distribution is now paused too, and the next local move is bootstrap runner-shell cleanup)
- [`011-service-catalog-and-compose-assembly.md`](./011-service-catalog-and-compose-assembly.md) (complete; `effigy-catalog` crate shipped with compose assembly, 6 bundled service fragments, production PHP Dockerfile, override system, volume lifecycle — 59 tests, awaiting runner integration)
- [`012-container-context-and-transparent-execution.md`](./012-container-context-and-transparent-execution.md) (in progress; `effigy-exec` crate shipped with routing engine, CWD mapping, exec aliases, container detection with handoff strategy — 53 tests, awaiting runner integration)
- [`013-dev-front-door-and-managed-lifecycle.md`](./013-dev-front-door-and-managed-lifecycle.md) (planned; single-command `effigy dev` front door using the managed-process concurrent runtime with embedded terminal and health gate)
- [`014-rust-native-gateway.md`](./014-rust-native-gateway.md) (in progress; `effigy-gateway` crate shipped with DNS resolver, streaming HTTP/HTTPS proxy, WebSocket upgrade, route table, macOS resolver, port registry — 62 tests, awaiting runner integration)
- [`015-persistent-data-and-volume-lifecycle.md`](./015-persistent-data-and-volume-lifecycle.md) (in progress; volume management shipped in `effigy-catalog::volumes`, seeding and Rhai hooks deferred to integration phase)
- [`016-multi-project-coordination.md`](./016-multi-project-coordination.md) (in progress; port allocation registry shipped in `effigy-gateway::ports`, cross-project status deferred to integration phase)
- [`017-remaining-shell-cleanup-and-crate-extraction-program.md`](./017-remaining-shell-cleanup-and-crate-extraction-program.md) (planned; queued substantial parallel cleanup jobs for the remaining heavy `/src` seams and possible final crate splits)

Container infrastructure design document:

- [`../architecture/020-container-infrastructure-design.md`](../architecture/020-container-infrastructure-design.md)

Active strict planning lane:

- [`../specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`](../specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md)
- active ready card: none — `g02.010` paused on a trustworthy full boundary
  after `237` confirmed no bounded cleanup target remains outside parallel
  thread ownership

Queued release card:

- [`../specs/batch-cards/115-implement-effigy-distribution-release-closure.md`](../specs/batch-cards/115-implement-effigy-distribution-release-closure.md)

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items that represent a fresh product cycle should start in `g02`
- continue numbering in `g02/` until another manual rollover is justified

## Next Task

`g02.010` is paused on a trustworthy full modularization boundary. Resume the
queued release card `115` when `v0.3` closure is intended, or open a new
product roadmap card against the stable crate boundary (e.g., `g02.013` dev
front door or `g02.008` demo/manifest import rollout).
