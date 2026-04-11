# Effigy Docs

Use this page when you want the fastest route to the right document.

Effigy has a wide surface area, but most readers only need a small part of it
at a time: how to get started, how to run common workflows, how to shape the
manifest, or how to automate safely. Start with the goal below, then follow the
deeper links only when you need them.

## Start Here

1. Read [`../README.md`](../README.md) for the product promise and the shortest
   first-run path.
2. Read
   [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
   for the first useful tasks.
3. Read
   [`guides/055-everyday-workflows.md`](./guides/055-everyday-workflows.md)
   for the most common day-to-day flows.
4. Read [`guides/022-manifest-cookbook.md`](./guides/022-manifest-cookbook.md)
   when you are ready to shape `effigy.toml`.
5. Read [`specs/README.md`](./specs/README.md) when the active product lane
   needs strict planning or ready-card execution control.

## By Goal

### I want to run work without hunting through the repo

- Start with
  [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
- Then read
  [`guides/055-everyday-workflows.md`](./guides/055-everyday-workflows.md)
- Use
  [`guides/016-task-routing-precedence.md`](./guides/016-task-routing-precedence.md)
  when routing needs explaining

### I want tests, health checks, and watch mode to feel consistent

- Start with
  [`guides/055-everyday-workflows.md`](./guides/055-everyday-workflows.md)
- Then use
  [`guides/018-doctor-explain-mode.md`](./guides/018-doctor-explain-mode.md),
  [`guides/019-watch-init-migrate-foundation.md`](./guides/019-watch-init-migrate-foundation.md),
  and
  [`guides/048-built-in-test-suite-lifecycle-and-env.md`](./guides/048-built-in-test-suite-lifecycle-and-env.md)

### I want to make the manifest do more of the work

- Start with
  [`guides/022-manifest-cookbook.md`](./guides/022-manifest-cookbook.md)
- Then use
  [`guides/050-env-schema-integration.md`](./guides/050-env-schema-integration.md),
  [`guides/028-migration-quick-paths.md`](./guides/028-migration-quick-paths.md),
  and
  [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)

### I want automation, CI, or agents to consume Effigy safely

- Start with
  [`guides/017-json-output-contracts.md`](./guides/017-json-output-contracts.md)
- Then use
  [`guides/024-ci-and-automation-recipes.md`](./guides/024-ci-and-automation-recipes.md),
  [`guides/026-json-payload-examples.md`](./guides/026-json-payload-examples.md),
  [`guides/047-agent-and-cross-repo-adoption.md`](./guides/047-agent-and-cross-repo-adoption.md),
  [`guides/056-northstar-effigy-consumer-repo-contract.md`](./guides/056-northstar-effigy-consumer-repo-contract.md),
  and [`contracts/README.md`](./contracts/README.md)

For full repo adoption:
- use the `northstar-effigy` skill to scaffold the repo shape, starter files,
  and templates
- use Effigy built-ins to validate the resulting contract with `qa:docs`,
  `qa:northstar`, release gates, and JSON-safe output

### I want release and distribution flows on built-ins

- Start with
  [`guides/051-release-orchestration.md`](./guides/051-release-orchestration.md)
- Then use
  [`guides/052-changelog-workflows-and-northstar-profile.md`](./guides/052-changelog-workflows-and-northstar-profile.md),
  [`guides/049-ci-binary-distribution-and-release-protocol.md`](./guides/049-ci-binary-distribution-and-release-protocol.md),
  and
  [`guides/044-distribution-first-publish-execution-runbook.md`](./guides/044-distribution-first-publish-execution-runbook.md)

### I want to work on the docs themselves

- Start with
  [`guides/037-documentation-contribution-playbook.md`](./guides/037-documentation-contribution-playbook.md)
- Then use
  [`guides/029-docs-qa-checklist-and-validation.md`](./guides/029-docs-qa-checklist-and-validation.md),
  [`guides/033-style-and-terminology-guide.md`](./guides/033-style-and-terminology-guide.md),
  and [`guides/039-docs-drift-monitoring.md`](./guides/039-docs-drift-monitoring.md)

## Core Feature Guides

- Quick start:
  [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
- Everyday workflows:
  [`guides/055-everyday-workflows.md`](./guides/055-everyday-workflows.md)
- Manifest patterns:
  [`guides/022-manifest-cookbook.md`](./guides/022-manifest-cookbook.md)
- Command reference:
  [`guides/025-command-reference-matrix.md`](./guides/025-command-reference-matrix.md)
- Troubleshooting:
  [`guides/023-troubleshooting-and-failure-recipes.md`](./guides/023-troubleshooting-and-failure-recipes.md)
- Migration paths:
  [`guides/028-migration-quick-paths.md`](./guides/028-migration-quick-paths.md)

## Reference Areas

- Practical guide hub: [`guides/README.md`](./guides/README.md)
- Architecture notes: [`architecture/`](./architecture/)
- JSON contracts: [`contracts/README.md`](./contracts/README.md)
- Active strict planning lane: [`specs/README.md`](./specs/README.md)
- Roadmaps: [`roadmaps/README.md`](./roadmaps/README.md)
- Release and validation logs: [`logs/README.md`](./logs/README.md)
- Vision documents: [`vision/README.md`](./vision/README.md)
- Research notes and source maps: [`research/README.md`](./research/README.md)

## Terminology Canon

Use the glossary terms consistently:

- `selector`
- `routing`
- `deferral`

Reference:
[`guides/034-task-and-command-glossary.md`](./guides/034-task-and-command-glossary.md)

## Next Task

Use the active strict `g02.003` spec lane to choose the next bounded
follow-up after the shipped `demo history` query surface, then keep the docs
front doors aligned to that narrower slice.
