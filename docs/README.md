# Effigy Docs

This is the docs front door: **goal-based links** into the guides (not the full
guide inventory—that lives in [`guides/README.md`](./guides/README.md)).

If you are new here, do not start by reading everything. Pick the job you are
trying to do, then follow one path.

**Need the binary first?** Install options (Homebrew, curl, `cargo install`) are
in the root [`README.md`](../README.md) under **Install**.

## Start Here

1. Use the root [`README.md`](../README.md) **Install** and **Start Fast** for
   the binary and the shortest first-run path.
2. Read [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
   for the first ten minutes.
3. Read [`guides/055-everyday-workflows.md`](./guides/055-everyday-workflows.md)
   when the basics are working and you want the normal day-to-day flow.

After that, choose one path below.

## Choose A Path

### I want to run tasks and get useful work done

**Start here (everyone):**
- [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
- [`guides/055-everyday-workflows.md`](./guides/055-everyday-workflows.md)

**Then go deeper:**
- [`guides/022-manifest-cookbook.md`](./guides/022-manifest-cookbook.md) — copy-paste patterns for `effigy.toml`
- [`guides/016-task-routing-precedence.md`](./guides/016-task-routing-precedence.md) — how task names resolve
- [`guides/023-troubleshooting-and-failure-recipes.md`](./guides/023-troubleshooting-and-failure-recipes.md) — fix common problems

### I want to run local dev environments with containers and services

Use this when a repo needs databases, caches, or language workspaces without
installing them directly on your machine.

**Start here:**
- [`guides/063-container-system-guide.md`](./guides/063-container-system-guide.md)
- [`guides/064-system-workspace-and-dev-contract.md`](./guides/064-system-workspace-and-dev-contract.md)

**Then go deeper:**
- [`guides/012-dev-process-manager-tui.md`](./guides/012-dev-process-manager-tui.md) — managed TUI/headless sessions, readiness, logs, and stop companions
- [`guides/065-external-bundle-adoption.md`](./guides/065-external-bundle-adoption.md) — typed external bundle source adoption
- [`guides/067-catalog-services-reference.md`](./guides/067-catalog-services-reference.md) — postgres, redis, and more
- [`guides/069-workspace-host-integration.md`](./guides/069-workspace-host-integration.md) — sibling repo mounts and Docker/Colima coexistence

**Use these when cleanup starts to matter:**
- [`guides/063-container-system-guide.md`](./guides/063-container-system-guide.md) — `container cache list/prune`, `container volume list/prune`
- [`guides/057-bootstrap-repo-bringup.md`](./guides/057-bootstrap-repo-bringup.md) — backend choice during repo bring-up

### I want to test a consumer against local library edits

- [`guides/077-local-dependency-linking.md`](./guides/077-local-dependency-linking.md) — choose machine-local Cargo/Bun links or committed root-consumer Bun pins

### I want to automate, integrate, or go deeper

**For CI and scripts:**
- [`guides/017-json-output-contracts.md`](./guides/017-json-output-contracts.md) — JSON output for automation
- [`guides/076-code-graph-and-agent-workflows.md`](./guides/076-code-graph-and-agent-workflows.md) — bounded repo context for agent code-understanding work
- [`guides/079-documentation-graph-profiles-and-context.md`](./guides/079-documentation-graph-profiles-and-context.md) — repository-owned documentation semantics and bounded `docs context` evidence
- [`architecture/024-repository-defined-documentation-graph.md`](./architecture/024-repository-defined-documentation-graph.md) — repository-owned Markdown graph architecture
- [`contracts/041-documentation-graph-profile-contract.md`](./contracts/041-documentation-graph-profile-contract.md) — `[docs_policy.graph]` profile grammar and semantics
- [`architecture/026-feature-placement-and-command-surface.md`](./architecture/026-feature-placement-and-command-surface.md) — core ownership, command grouping, and provider/extension placement
- [`contracts/043-feature-placement-and-surface-migration-contract.md`](./contracts/043-feature-placement-and-surface-migration-contract.md) — alias-stable command and feature migration gates
- [`architecture/027-catalog-scoped-code-graph.md`](./architecture/027-catalog-scoped-code-graph.md) — catalog-derived code-graph scope and storage ownership
- [`contracts/045-catalog-scoped-code-graph-contract.md`](./contracts/045-catalog-scoped-code-graph-contract.md) — segmented catalog grammar, selection, and no-sibling-work guarantees
- [`architecture/028-published-and-draft-task-surfaces.md`](./architecture/028-published-and-draft-task-surfaces.md) — maintained published selectors and provisional draft-task ownership
- [`contracts/046-published-and-draft-task-surface-contract.md`](./contracts/046-published-and-draft-task-surface-contract.md) — draft grammar, discovery isolation, expiry evidence, and execution compatibility
- [`architecture/029-bounded-doctor-and-scan-cache.md`](./architecture/029-bounded-doctor-and-scan-cache.md) — fast structural diagnosis and explicit incremental deep checks
- [`contracts/047-bounded-doctor-and-scan-cache-contract.md`](./contracts/047-bounded-doctor-and-scan-cache-contract.md) — doctor tiers, catalog scope, cache trust, deadlines, and cancellation
- [`guides/078-papercuts-discovery-and-capture.md`](./guides/078-papercuts-discovery-and-capture.md) — project and sibling-project friction inventory for humans and agents
- [`guides/024-ci-and-automation-recipes.md`](./guides/024-ci-and-automation-recipes.md) — copy-paste CI workflows
- [`guides/050-env-schema-integration.md`](./guides/050-env-schema-integration.md) — `--env-schema` overrides and validation when tasks need typed env

**For demos and proofs:**
- [`guides/058-demo-system-guide.md`](./guides/058-demo-system-guide.md)

**For data artifacts, OCI, and bootstrap seed flows:**
- [`guides/072-artifact-commands-guide.md`](./guides/072-artifact-commands-guide.md)
- [`guides/057-bootstrap-repo-bringup.md`](./guides/057-bootstrap-repo-bringup.md)

**For release workflows:**
- [`guides/051-release-orchestration.md`](./guides/051-release-orchestration.md)
- [`guides/062-distribution-system-guide.md`](./guides/062-distribution-system-guide.md)

**For contributing to docs:**
- [`guides/037-documentation-contribution-playbook.md`](./guides/037-documentation-contribution-playbook.md)

## Other Areas

- Architecture: [`architecture/`](./architecture/)
- Contracts and JSON surfaces: [`contracts/README.md`](./contracts/README.md)
- Roadmaps: [`roadmaps/README.md`](./roadmaps/README.md)
- Research: [`research/README.md`](./research/README.md)
- Vision: [`vision/README.md`](./vision/README.md)
- Strict planning lane: [`specs/README.md`](./specs/README.md)
<!-- northstar:lifecycle:begin schema=northstar.lifecycle.projection.v2 digest=sha256:7bde7ea06f11db008d01daaf8e983cf7e57a54b72ef166419fceb2bb5620a05c -->
| Generation | Disposition | Runway state |
| --- | --- | --- |
| g10 | open | planning_required |
| Task | Status | Stage | Revision | Record digest |
| --- | --- | --- | --- | --- |
| g10.002 | complete | none | 8 | sha256:acc40985e9f1420efa1189899daec76abc3be7d6f143bbc28dac2a8f66dcd833 |
| g10.003 | complete | none | 8 | sha256:f51ae9d2f79a84dfa1aa970575d37e10442574d5025015bf725258c72f2bd9a5 |
| g10.004 | complete | none | 8 | sha256:493fff197276ab92aab90e09838bed45ba1de970a3a5d11cbc5fa8091e719e5d |
| g10.005 | complete | none | 8 | sha256:bbdc3e4907f88ceb64be891fff87847854e123e4d552c565080fcda70a4d8881 |
| g10.006 | complete | none | 8 | sha256:b0134c685fb320d7a8acdcd4f0db4a6765270c3dd8b8bb61496cad0682e6dc49 |
| g10.007 | complete | none | 8 | sha256:04342987457e1bea973556297a4bef39e1592794e0dd3d52ebcb4457ae915995 |
| g10.008 | complete | none | 8 | sha256:6cfd213603aefd7367edc6d5cdc5eb9dae4b3ff09062defd6fa1cf80c736dc8b |
| g10.009 | complete | none | 8 | sha256:e9eec1299d0c8d71b4112224a5275f574100b411d9d7e690b937bf01f953a1f7 |
| g10.010 | complete | none | 8 | sha256:9b534c710ef6277f3996c4a3b0eee36cf128019480fd7e8c1b0b5f2dc710303b |
| g10.011 | complete | none | 8 | sha256:662faf219b8d797233e1c1fafd7ade3e3620ac1e6983de7eef1b9333feb3aa58 |
| g10.012 | complete | none | 8 | sha256:77c770dee88cc0c6e080d20ea7b19e6e00f68bbf4f73f8b9bdd4cb6272398ae9 |
| g10.013 | complete | none | 8 | sha256:1806cd3a5f2835288804585cb0c7541b52f17fe2dedff13d5547046ceb80192e |
<!-- northstar:lifecycle:end -->
