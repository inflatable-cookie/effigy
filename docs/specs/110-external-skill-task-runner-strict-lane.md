# 110 External Skill Task Runner Strict Lane

Status: Active
Created: 2026-08-31
Roadmap: [`g08.037`](../roadmaps/g08/037-external-skill-task-runner.md)
Architecture: [`025`](../architecture/025-external-skill-task-execution.md)
Contract: [`042`](../contracts/042-external-skill-task-runner-contract.md)
Current ready card: [`1092`](../roadmaps/g08/batch-cards/1092-add-external-skill-task-runner.md)

## Outcome

An agent in a consumer repository can explicitly load tasks from an installed
skill and run them against the consumer without changing CWD, pointing
`--repo` at the skill, registering the skill in the consumer manifest, or
copying task definitions.

## Decisions

- Public domain: `effigy skill`.
- Required source: `--path <SKILL_DIR|EFFIGY_TOML>`.
- Consumer target: invocation-CWD root resolution or explicit `--repo`.
- V1 posture: isolated host/Rhai catalog; no consumer runtime inheritance.
- Discovery: explicit source only; no installed-skill registry or global scan.
- Delivery: one complete implementation and adoption-proof card.

## Scope

- CLI grammar/help for `skill tasks` and `skill run`
- typed source/target execution context
- isolated source composition and selector routing
- source-relative assets and target-relative runtime paths
- nested task/Rhai target preservation
- text/JSON diagnostics and schemas
- focused fixtures, docs, changelog, and Northstar skill smoke
- lane closeout and restoration of paused documentation-context card `1089`

## Non-Goals

- resolving skills by name or through Codex installation metadata
- installing, updating, signing, or trusting skills
- merging skill selectors into consumer `effigy tasks`
- consumer container/system/default inheritance
- skill catalog members or multi-skill orchestration
- editing the Northstar repository in this lane

## Runway

1. [`1092`](../roadmaps/g08/batch-cards/1092-add-external-skill-task-runner.md)
   implements, proves, documents, and closes the surface.

Card `1092` is ready.

## Completion State

Close only when the command, contract proofs, JSON/docs coverage, read-only
Northstar skill smoke, full QA, changelog, evidence log, and front-door
restoration are complete. Resume card `1089` as the single next task.

## Next Task

Execute ready card
[`1092`](../roadmaps/g08/batch-cards/1092-add-external-skill-task-runner.md).
