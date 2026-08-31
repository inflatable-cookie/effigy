# 1092 - Add External Skill Task Runner

Roadmap: [`../037-external-skill-task-runner.md`](../037-external-skill-task-runner.md)
Architecture: [`../../../architecture/025-external-skill-task-execution.md`](../../../architecture/025-external-skill-task-execution.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/011-runtime-context-contract.md`](../../../contracts/011-runtime-context-contract.md),
[`../../../contracts/013-task-execution-request-contract.md`](../../../contracts/013-task-execution-request-contract.md),
[`../../../contracts/037-explicit-catalog-membership-contract.md`](../../../contracts/037-explicit-catalog-membership-contract.md),
[`../../../contracts/042-external-skill-task-runner-contract.md`](../../../contracts/042-external-skill-task-runner-contract.md)
Spec: [`../../../specs/archive/110-external-skill-task-runner-strict-lane.md`](../../../specs/archive/110-external-skill-task-runner-strict-lane.md)

Status: Complete
Owner: CLI, task routing, runtime context, and execution pipeline
Created: 2026-08-31

## Purpose

Ship the complete explicit runner for tasks stored in installed skills while
the consuming repository remains the runtime target.

## Work

- add `skill tasks` and `skill run` parser, help, dispatch, text, and JSON
- resolve a required skill directory or manifest without ambient discovery
- carry source manifest/root separately from invocation CWD, target root, and
  execution CWD
- load one isolated composed catalog and reject members or container/runtime
  inheritance before side effects
- make includes, bundles, Rhai/script steps, and `{skill}` source-relative
- make host process CWD, `{repo}`/`{project}`, env files, cache inputs/outputs,
  state, artifacts, and built-ins target-relative
- preserve source and target through nested skill task and Rhai dispatch
- keep ordinary selector routing and existing `--repo` behavior unchanged
- add focused fixtures for every contract-042 failure and review-oracle case
- add versioned JSON schema/example/selection coverage
- update command reference, cookbook/troubleshooting, agent skill guidance,
  generated reference/help coverage, architecture/package map, and changelog
- run a read-only smoke against
  `/Users/tom/Dev/projects/northstar/skills/northstar/effigy.toml` when present;
  fixture proof remains authoritative when the checkout is absent
- close the card/lane/spec with one evidence log, then restore card `1089` as
  the single ready/next task across front doors

## Acceptance

- [x] `skill tasks` lists only the explicit source catalog
- [x] `skill run` targets the CWD-resolved or `--repo` consumer
- [x] source/target/invocation/execution paths match contract `042`
- [x] no consumer selector, defaults, container, member, or ambient path leaks
      into skill execution
- [x] nested task and Rhai execution preserve both identities
- [x] all contract-042 rejection cases stop before side effects
- [x] text and JSON expose matching resolution evidence
- [x] ordinary task and `--repo` regression tests stay green
- [x] public/agent docs can replace Northstar's CWD gymnastics with one command
- [x] focused and full QA pass
- [x] closeout leaves card `1089` ready and no stale skill-runner card

## Evidence

[`2026-08-31 external skill task runner closeout`](../../../logs/2026-08/31-162015-external-skill-task-runner-closeout.md)

## Review Oracle

Use contract `042` `## Review Oracle`. Falsify each of its six adversarial
counterexamples and map the result to test or smoke evidence before PR creation.

## Validation

- focused `effigy-cli`, `effigy-context`, `effigy-manifest`, `effigy-routing`,
  `effigy-execution`, `effigy-rhai`, runner, and CLI-output tests
- JSON schema/example/selection checks
- focused documentation coverage and generated help/reference tests
- read-only Northstar skill smoke when the local source is present
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Close with one dated log containing the source/target matrix, six review-oracle
proofs, no-side-effect rejection evidence, JSON validation, Northstar smoke or
explicit absence, test counts, full QA, and the transition back to card `1089`.

## Stop Conditions

Stop if implementation needs implicit skill discovery, consumer config
inheritance, container-bound execution, external catalog members, a breaking
change to ordinary task routing, or a product/API choice not settled by contract
`042`.

## Next Task

Execute ready documentation-context card
[`1089`](./1089-add-bounded-documentation-context-query.md).
