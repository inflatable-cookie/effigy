# External Skill Task Runner Planning

Status: complete
Created: 2026-08-31
Roadmap: g08.037
Batch: external-skill-task-runner-planning

## Summary

- Captured the installed-skill task friction from Northstar's live command
  surface.
- Split external task-definition source from consumer runtime target.
- Promoted architecture `025`, contract `042`, strict spec `110`, roadmap
  `g08.037`, and ready card `1092`.
- Paused documentation-context card `1089`; it remains the next task after the
  skill-runner closeout.

## Decisions

- Public commands: `effigy skill tasks` and `effigy skill run`.
- Required explicit source: skill directory or manifest path.
- Consumer target: invocation-CWD root resolution or `--repo`.
- V1: one isolated host/Rhai catalog, no consumer runtime inheritance, members,
  automatic discovery, install, registry, or container execution.
- Delivery: one worker card covering runtime, CLI, JSON, docs, Northstar smoke,
  and closeout.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`
- Movement: task source and runtime target were coupled -> explicit isolated
  skill source with an independently resolved consumer target is ready for
  implementation
- Remaining gap: card `1092` implementation and proof; paused docs cards
  `1089` and `1090`

## Validation Performed

- `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`
  - planning base matched `origin/main` before edits
- planning authority and currentness review
  - one ready card: `1092`
  - one paused return card: `1089`
- `effigy qa:docs`
  - passed after staging the new indexed planning files

## Risks

- Source-relative assets and target-relative runtime paths cross existing
  catalog-root assumptions. Contract `042` supplies the rejection boundaries
  and six adversarial review cases.

## Next Task

Execute ready card
[`1092`](../../roadmaps/g08/batch-cards/1092-add-external-skill-task-runner.md).
