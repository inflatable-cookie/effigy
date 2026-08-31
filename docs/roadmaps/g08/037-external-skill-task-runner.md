# g08.037 External Skill Task Runner

Status: Active
Created: 2026-08-31
Spec: [`110`](../../specs/110-external-skill-task-runner-strict-lane.md)
Architecture: [`025`](../../architecture/025-external-skill-task-execution.md)
Contract: [`042`](../../contracts/042-external-skill-task-runner-contract.md)

## Purpose

Let installed skill projects own reusable Effigy tasks while a consuming
repository remains the invocation and runtime target.

## Context

Northstar currently teaches commands such as:

```text
effigy --repo <installed-northstar> northstar/rust-quality:setup apply <target> ...
```

That makes the skill path look like the repository and pushes the real target
through task arguments. The resulting CWD workarounds and instruction burden
hide the source/target distinction from agents and operators.

## Scope

- add explicit isolated `effigy skill tasks` and `effigy skill run` surfaces
- split task-definition source from consumer target in typed execution context
- preserve source/target identity through nested tasks and Rhai
- expose deterministic text/JSON evidence
- prove current ordinary task behavior is unchanged
- publish user and agent guidance and close with Northstar skill evidence

## Boundary

- no automatic skill discovery, install, update, registry, or trust system
- no implicit consumer manifest/runtime inheritance
- no skill catalog members or container-bound skill execution in v1
- no edits to the Northstar repository
- no work on paused documentation-context cards `1089` or `1090`

## Cards

- [`1092`](./batch-cards/1092-add-external-skill-task-runner.md) — complete
  command, runtime, proof, docs, and closeout

## Acceptance

- one command selects an explicit installed skill source and current/explicit
  consumer target
- task assets and runtime paths follow contract `042`
- source and target cannot merge through ambient discovery or inheritance
- ordinary selector and `--repo` behavior remains unchanged
- Northstar's installed skill catalog lists and runs through the new surface
  without modifying Northstar or the consumer manifest
- full QA passes and card `1089` returns as the next ready task

## Next Task

Execute ready card
[`1092`](./batch-cards/1092-add-external-skill-task-runner.md).
