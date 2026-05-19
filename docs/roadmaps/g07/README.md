# g07 Roadmaps

Status: Active
Theme: Agent-facing repo intelligence and setup front doors

## Purpose

`g07` plans Effigy's agent-facing repo intelligence and setup front doors.

The goal is a deterministic, local, queryable interpretation of a repo that
agents can use before falling back to broad file scans. The graph should make
Effigy better at answering "where is this behavior owned?", "what calls this?",
"what files matter for this task?", and "what context should I read first?"

Later `g07` lanes also widen into the adjacent setup front door work needed to
make that intelligence usable from first contact with a repo, especially
through `effigy init`.

The final planned `g07` lane turns the reusable codebase sweep audit into
bounded cleanup work. It keeps graph and init maintainable before the next
feature generation starts.

The CLI remains the protocol. This generation does not introduce an MCP server,
a background daemon, external language plugins, or a JavaScript runtime
dependency.

## Roadmap Sequence

- [`001-code-graph-intelligence-suite.md`](./001-code-graph-intelligence-suite.md)
- [`002-graph-storage-and-json-contracts.md`](./002-graph-storage-and-json-contracts.md)
- [`003-graph-index-command-and-freshness-model.md`](./003-graph-index-command-and-freshness-model.md)
- [`004-first-party-language-extractor-framework.md`](./004-first-party-language-extractor-framework.md)
- [`005-rust-extractor.md`](./005-rust-extractor.md)
- [`006-effigy-manifest-toml-and-task-graph-indexer.md`](./006-effigy-manifest-toml-and-task-graph-indexer.md)
- [`007-markdown-docs-and-anchor-indexer.md`](./007-markdown-docs-and-anchor-indexer.md)
- [`008-php-extractor.md`](./008-php-extractor.md)
- [`009-javascript-typescript-extractor.md`](./009-javascript-typescript-extractor.md)
- [`010-query-commands.md`](./010-query-commands.md)
- [`011-agent-context-packs.md`](./011-agent-context-packs.md)
- [`012-performance-cache-and-regression-proof.md`](./012-performance-cache-and-regression-proof.md)
- [`013-graph-follow-up-performance-and-fixture-reliability.md`](./013-graph-follow-up-performance-and-fixture-reliability.md)
- [`014-incremental-indexing-and-cache-reuse.md`](./014-incremental-indexing-and-cache-reuse.md)
- [`015-query-speed-and-projection-reduction.md`](./015-query-speed-and-projection-reduction.md)
- [`016-failed-graph-fixture-path-reliability.md`](./016-failed-graph-fixture-path-reliability.md)
- [`017-graph-scan-cost-reduction-suite.md`](./017-graph-scan-cost-reduction-suite.md)
- [`018-file-walk-and-scan-metadata-baseline.md`](./018-file-walk-and-scan-metadata-baseline.md)
- [`019-safe-scan-metadata-reuse.md`](./019-safe-scan-metadata-reuse.md)
- [`020-scan-cost-closeout-proof.md`](./020-scan-cost-closeout-proof.md)
- [`021-graph-watch-mode-suite.md`](./021-graph-watch-mode-suite.md)
- [`022-watch-backend-and-debounce-rules.md`](./022-watch-backend-and-debounce-rules.md)
- [`023-dirty-reconcile-and-overflow-fallback.md`](./023-dirty-reconcile-and-overflow-fallback.md)
- [`024-graph-watch-closeout-proof.md`](./024-graph-watch-closeout-proof.md)
- [`025-graph-context-ranking-quality-suite.md`](./025-graph-context-ranking-quality-suite.md)
- [`026-context-ranking-baseline-and-gold-tasks.md`](./026-context-ranking-baseline-and-gold-tasks.md)
- [`027-role-aware-context-ranker.md`](./027-role-aware-context-ranker.md)
- [`028-search-and-snippet-usefulness.md`](./028-search-and-snippet-usefulness.md)
- [`029-graph-navigation-quality-closeout.md`](./029-graph-navigation-quality-closeout.md)
- [`030-graph-explore-agent-call-suite.md`](./030-graph-explore-agent-call-suite.md)
- [`031-explore-contract-and-benchmark-baseline.md`](./031-explore-contract-and-benchmark-baseline.md)
- [`032-explore-context-assembly-command.md`](./032-explore-context-assembly-command.md)
- [`033-agent-guidance-and-skill-update.md`](./033-agent-guidance-and-skill-update.md)
- [`034-explore-benchmark-closeout.md`](./034-explore-benchmark-closeout.md)
- [`035-codegraph-parity-suite.md`](./035-codegraph-parity-suite.md)
- [`036-parity-benchmark-harness-and-claim-discipline.md`](./036-parity-benchmark-harness-and-claim-discipline.md)
- [`037-fts-backed-source-evidence-and-ranking.md`](./037-fts-backed-source-evidence-and-ranking.md)
- [`038-traversal-aware-explore-assembly.md`](./038-traversal-aware-explore-assembly.md)
- [`039-richer-language-extractor-coverage.md`](./039-richer-language-extractor-coverage.md)
- [`040-framework-route-and-entrypoint-edges.md`](./040-framework-route-and-entrypoint-edges.md)
- [`041-source-section-packets-and-no-reread-workflow.md`](./041-source-section-packets-and-no-reread-workflow.md)
- [`042-affected-test-and-impact-workflow.md`](./042-affected-test-and-impact-workflow.md)
- [`043-large-repo-scale-and-storage-hardening.md`](./043-large-repo-scale-and-storage-hardening.md)
- [`044-agent-adoption-and-cli-workflow-polish.md`](./044-agent-adoption-and-cli-workflow-polish.md)
- [`045-codegraph-parity-closeout.md`](./045-codegraph-parity-closeout.md)
- [`046-codegraph-parity-follow-up-suite.md`](./046-codegraph-parity-follow-up-suite.md)
- [`047-warm-query-latency-and-release-ranking.md`](./047-warm-query-latency-and-release-ranking.md)
- [`048-fixture-backed-parity-proof.md`](./048-fixture-backed-parity-proof.md)
- [`049-codegraph-parity-follow-up-closeout.md`](./049-codegraph-parity-follow-up-closeout.md)
- [`050-init-setup-wizard-suite.md`](./050-init-setup-wizard-suite.md)
- [`051-init-context-inventory-and-checklist-contract.md`](./051-init-context-inventory-and-checklist-contract.md)
- [`052-tty-init-wizard-engine-and-prompt-flow.md`](./052-tty-init-wizard-engine-and-prompt-flow.md)
- [`053-setup-job-adapters-and-mutation-boundaries.md`](./053-setup-job-adapters-and-mutation-boundaries.md)
- [`054-noninteractive-init-action-execution-and-migration-paths.md`](./054-noninteractive-init-action-execution-and-migration-paths.md)
- [`055-init-wizard-proof-docs-and-closeout.md`](./055-init-wizard-proof-docs-and-closeout.md)
- [`056-codebase-leanness-and-boundary-hardening-suite.md`](./056-codebase-leanness-and-boundary-hardening-suite.md)
- [`057-codegraph-language-emitter-deduplication.md`](./057-codegraph-language-emitter-deduplication.md)
- [`058-codegraph-manifest-query-module-decomposition.md`](./058-codegraph-manifest-query-module-decomposition.md)
- [`059-init-setup-module-boundary-cleanup.md`](./059-init-setup-module-boundary-cleanup.md)
- [`060-json-help-contract-consistency-cleanup.md`](./060-json-help-contract-consistency-cleanup.md)
- [`061-runner-domain-boundary-and-test-fixture-cleanup.md`](./061-runner-domain-boundary-and-test-fixture-cleanup.md)
- [`062-crate-boundary-rejustification-and-planning-hygiene.md`](./062-crate-boundary-rejustification-and-planning-hygiene.md)
- [`063-codebase-leanness-closeout.md`](./063-codebase-leanness-closeout.md)

## Design Posture

- keep all v1 language support first-party and compiled into Effigy
- use internal extractor traits so language owners stay modular
- do not claim compiler-grade semantic analysis
- mark heuristic edges as heuristic
- store provenance and ranges for every emitted graph fact
- treat JSON CLI output as the public contract, not the DB layout
- keep graph artifacts local under `.effigy/graph/`

## Non-Goals

- no MCP server for graph v1
- no graph-specific daemon
- no JavaScript runtime dependency
- no external language package/plugin system
- no LLM-generated summaries as canonical graph data
- no editor-specific integration as core scope
- no "support every language" launch target

## Current State

`g06` is closed through `g06.008`.

`g07.001` is complete.

`g07.013` is complete.

`g07.017` is complete.

`g07.021` is complete.

`g07.022` is complete.

`g07.023` is complete.

`g07.024` is complete.

`g07.025` is complete.

`g07.030` is complete.

`g07.031` is complete.

`g07.032` is complete.

`g07.033` is complete.

`g07.034` is complete.

`g07.035` is complete.

`g07.036` is complete.

`g07.037` is complete.

`g07.038` is complete.

`g07.039` is complete.

`g07.040` is complete.

`g07.041` is complete.

`g07.042` through `g07.045` are the completed CodeGraph parity suite.

`g07.046` through `g07.049` are the bounded CodeGraph parity follow-up suite.

`901` through `905` are complete.

`906` through `910` are complete.

`911` is complete.

`912` is complete.

`931` is complete.

`932` is complete.

`933` is complete.

`935` is complete.

`950` through `954` are complete.

`960` is complete.

`961` is complete.

`962` is complete.

`963` is complete.

`964` is complete.

`970` is complete.

`971` is complete.

`972` is complete.

`973` is complete.

`974` is complete.

`980` is complete.

`981` is complete.

`982` is complete.

`983` is complete.

`984` is complete.

`985` is complete.

`986` is complete.

`987` is complete.

`988` is complete.

`989` is complete.

`990` is complete.

`991` is complete.

`992` is complete.

`993` is complete.

`994` is complete.

`995` is complete.

No active ready card remains in the CodeGraph parity lane.

`996` is complete in the follow-up lane.

`997` is complete.

`998` is complete.

`999` is complete.

No active ready card remains in the bounded CodeGraph parity follow-up lane.

`g07.050` through `g07.055` are the completed init setup-wizard suite.

`g07.056` through `g07.063` are the planned codebase leanness and boundary
hardening suite.

`1000` is complete.

`1001` is complete.

`1002` is complete.

`1003` is complete.

`1004` is complete.

`1005` is complete.

No active ready card remains in the init setup-wizard lane.

`1006` is complete.

`1007` is ready.

`1008` through `1013` are planned.

## Next Task

Start [`1007-deduplicate-codegraph-language-emitters.md`](./batch-cards/1007-deduplicate-codegraph-language-emitters.md).
