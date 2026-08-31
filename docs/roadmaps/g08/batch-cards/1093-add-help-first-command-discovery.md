# 1093 - Add Help-First Command Discovery

Roadmap: [`../038-help-first-command-discovery.md`](../038-help-first-command-discovery.md)
Architecture: [`../../../architecture/026-feature-placement-and-command-surface.md`](../../../architecture/026-feature-placement-and-command-surface.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/043-feature-placement-and-surface-migration-contract.md`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)
Spec: [`../../../specs/111-help-first-command-discovery-strict-lane.md`](../../../specs/111-help-first-command-discovery-strict-lane.md)

Status: Ready
Owner: CLI command inventory, help parser/rendering, and public documentation
Created: 2026-08-31
Ready since: 2026-08-31 operator approval of help-first scope and exact topics

## Purpose

Ship grouped command discovery without adding grouped execution grammar or
changing existing task and built-in routing.

## Work

- add typed primary help-group ownership to the existing command/help inventory
- group `effigy --help` and `effigy help` under the six contract-043 topics
- add `effigy help <group>` inventories
- add `effigy help <command>` through the current detailed-help owner
- make unknown help topics fail with deterministic valid-path guidance
- preserve deferred-built-in filtering on every relevant help surface
- prove group names do not become top-level built-ins or steal manifest tasks
- add focused parser, help, output, inventory, and fixture coverage
- update user guidance, generated help/reference coverage, agent guidance where
  affected, and `CHANGELOG.md`
- close the card, roadmap, and strict spec with one evidence log; return the
  feature-boundary queue to planning for catalog-pack acquisition design

## Acceptance

- [ ] general help has `work`, `local`, `repo`, `deliver`, `extend`, and `admin`
- [ ] every general-help entry has one and only one primary group
- [ ] each group inventory exactly matches contract `043`
- [ ] `help <command>` and `<command> --help` have fact parity
- [ ] unknown help topics fail deterministically with useful guidance
- [ ] deferred built-ins remain absent from general, group, and direct help
- [ ] manifest selectors named after groups retain current routing
- [ ] no grouped execution route or new top-level built-in exists
- [ ] current direct-command behavior and output contracts are unchanged
- [ ] public, generated, and agent-facing guidance is consistent
- [ ] focused and full validation pass
- [ ] closeout leaves no stale ready card and returns the queue to planning

## Review Oracle

Falsify these counterexamples before PR creation:

1. `effigy help repo` lists exactly `graph`, `scan`, `docs`, `contracts`, and
   `papercuts`; it includes no local-runtime or delivery command.
2. `effigy help docs` and `effigy docs --help` contain the same command facts.
3. In a fixture with `[tasks.repo]`, `effigy repo` still executes that task
   while `effigy help repo` renders repository-intelligence discovery.
4. When a manifest selector shadows a deferred built-in, that built-in is
   omitted from general help and its primary group help as current policy
   requires.
5. An inventory assertion finds no missing or duplicate primary-group owner.
6. `effigy repo docs` is not interpreted as a new built-in grouped route; it
   follows existing task routing or existing error behavior.
7. `effigy help not-a-topic` fails deterministically and points at valid groups
   and commands instead of silently rendering general help.

## Validation

- focused `effigy-cli`, parser, help, output, inventory, and deferral tests
- focused generated-reference and documentation-coverage checks
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Close with one dated log mapping all seven review-oracle cases to tests or
fixtures, recording direct-command regression proof, generated/public docs
coverage, test counts, full QA, and the return to planning.

## Stop Conditions

Stop if the implementation needs executable aliases, new top-level group
names, direct execution or output-contract changes, alias deprecation/removal,
weaker deferred-built-in behavior, a new taxonomy decision, or work on release,
catalog packs, S3, or provider extraction.

## Next Task

Implement this card. On closeout, return to planning for the catalog-pack
acquisition prototype; do not infer an implementation lane or release work.
