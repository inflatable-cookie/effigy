# g07.052 - TTY Init Wizard Engine And Prompt Flow

Status: Complete
Depends on: `g07.051`

## Goal

Make plain `effigy init` in a real TTY an interactive setup wizard that walks
the user through relevant setup jobs with short, bounded yes/no prompts.

## Scope

- detect whether init is running in a TTY with no conflicting flags
- keep current non-interactive behavior for:
  - `--json`
  - `--check`
  - `--checklist`
  - named starters
  - piped/non-TTY invocation
- add a prompt engine that consumes the shared setup-job inventory
- group prompts into short setup phases
- render a final summary of:
  - completed actions
  - skipped actions
  - deferred actions
  - follow-up commands

## Prompt Phases

1. Baseline repo files
2. Agent setup
3. Task migration and cleanup
4. Repo health and test inspection
5. Graph setup
6. Secrets
7. Containers and bundles
8. Advanced inspection-only setup checks
9. Final validation

## Guardrails

- prompts must stay yes/no and bounded; no open-ended wizard maze
- do not ask about jobs that are not applicable in the current repo
- do not ask mutation questions for release/deploy/state apply paths
- keep plain `effigy init` fast in non-TTY contexts
- let the user skip a phase without losing the rest of the session

## Acceptance Criteria

- plain TTY `effigy init` enters wizard mode
- non-TTY `effigy init` keeps deterministic apply semantics
- the prompt flow is driven by the shared checklist model
- prompt output remains concise and predictable

## Evidence

- [`2026-05/19-122703-tty-init-wizard-engine.md`](../../logs/2026-05/19-122703-tty-init-wizard-engine.md)

## Next Task

Execute `1003`.
