# g07.077 - Agent Skill And Doc Query Guidance

Status: Complete
Depends on: `g07.076`

## Goal

Update the agent-facing guidance so agents actually choose graph when it is
useful, without making graph crowd out other Effigy features.

The skill should teach decision-making, not hype.

## Scope

- update `.agents/skills/effigy` and `skills/effigy` together
- keep the first-contact loop balanced:
  - `doctor`
  - `tasks`
  - `test --plan`
  - `graph explore` for code understanding
  - selectors/built-ins for execution
- include concrete query patterns that transfer across repos:
  - `where is <behavior> implemented`
  - `<domain> <action> <object>`
  - `<feature> edit target`
  - `<changed files> affected tests`
- include explicit fallback rules for `rg`
- update docs/guides only where they are active operator guidance

## Guardrails

- do not make graph the universal first command
- do not bury deploy, state, distribution, containers, docs, secrets, or task
  execution surfaces
- do not add agent guidance that assumes this repo's module names
- do not update historical roadmap text as if it were current guidance

## Acceptance Criteria

- skill guidance matches actual command behavior
- examples include non-Effigy-shaped wording
- graph is prominent for code understanding but not dominant for all work
- docs link and index checks pass

## Next Task

Execute `1027`.
