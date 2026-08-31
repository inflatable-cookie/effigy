# 110 External Skill Task Runner Strict Lane

Status: Complete
Created: 2026-08-31
Closed: 2026-08-31
Roadmap: [`g08.037`](../../roadmaps/g08/037-external-skill-task-runner.md)
Architecture: [`025`](../../architecture/025-external-skill-task-execution.md)
Contract: [`042`](../../contracts/042-external-skill-task-runner-contract.md)
Completed card: [`1092`](../../roadmaps/g08/batch-cards/1092-add-external-skill-task-runner.md)
Evidence: [`2026-08-31 closeout`](../../logs/2026-08/31-162015-external-skill-task-runner-closeout.md)

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
- lane closeout and restoration of documentation-context card `1089`

## Non-Goals

- resolving skills by name or through Codex installation metadata
- installing, updating, signing, or trusting skills
- merging skill selectors into consumer `effigy tasks`
- consumer container/system/default inheritance
- skill catalog members or multi-skill orchestration
- editing the Northstar repository in this lane

## Completion State

Card `1092` delivered the command, contract proofs, JSON/docs coverage,
read-only Northstar skill smoke, full QA, changelog, and evidence log. Strict
spec `108` resumed with card `1089` as the single ready task.

## Next Task

Execute ready card
[`1089`](../../roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md).
