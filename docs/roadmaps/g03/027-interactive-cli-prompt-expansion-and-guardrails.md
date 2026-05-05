# 027 - Interactive CLI Prompt Expansion and Guardrails

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05
Depends on: [`018-v1-runtime-hardening-proof-and-stress-matrix.md`](./018-v1-runtime-hardening-proof-and-stress-matrix.md), [`019-v0.3.x-release-foundation-and-v1.0-readiness-assessment.md`](./019-v0.3.x-release-foundation-and-v1.0-readiness-assessment.md)

## Problem

The new bootstrap DB-seed prompt proved there is real value in bounded,
TTY-only interactive completion for missing operator input. Effigy still has
other seams where the CLI either:

- makes the operator fully spell out obvious missing input even when the
  runtime already knows the safe bounded choices, or
- performs broad or destructive actions without a final interactive check when
  the command is launched from a real terminal

Right now those cases are inconsistent. Some surfaces already have dedicated
interaction models:

- release has its own interactive review flow
- gateway elevation already prompts when host privilege is needed

But the rest of the CLI has no clear prompt policy. If this grows ad hoc, the
result will be noisy, hard to script, and hard to reason about.

## Goal

Define and ship one bounded interactive prompt policy for Effigy CLI surfaces,
then apply it to the next highest-value cases after bootstrap DB seeding.

## Scope

- document the prompt decision rules:
  - prompt only on real stdin + stdout TTY
  - never prompt for `--json`, `--plan`, or explicit non-interactive paths
  - every prompted destructive surface must expose an escape hatch such as
    `--yes`, `--force`, or `--no-prompt`
- treat prompts as one of two things only:
  - bounded completion of missing required operator input
  - bounded confirmation before destructive or broad-impact actions
- add the next prompt surfaces in this order:
  - bootstrap existing-path reuse confirmation when `--path` targets a
    non-empty directory
  - `container data pull-production`
  - `container data import` when the target import is broad or overwriting
  - `unlock` for `--all`, broad shared scopes, or multiple scopes at once
  - optional `init` starter selection was evaluated and deferred out of this
    guardrail lane
- keep prompt behavior front-end only; the underlying command contract should
  still be driven by the same parsed arguments and execution path
- add targeted proof for TTY-only behavior and non-interactive suppression

## Non-Goals

- adding prompts to normal task execution, `dev`, `exec`, `qa`, `doctor`, or
  other script-first read/report commands
- replacing the dedicated release interactive flow
- building a broad wizard framework or multi-step setup assistant
- introducing prompt requirements into JSON contracts

## Exit Condition

This milestone is complete when:

- Effigy has one explicit prompt policy for CLI surfaces
- bootstrap path reuse, production/data import, and broad unlock actions follow
  that policy
- interactive prompt behavior is suppressed cleanly for non-TTY, `--json`,
  `--plan`, and explicit non-interactive modes
- the docs and help text explain when prompts appear and how to bypass them

## Next Task

No active ready card. Stop in planning and choose the next live roadmap
deliberately.

## Closeout

`g03.027` is complete. Effigy now has one shared prompt policy and applies it
to bootstrap destination reuse, generated-compose data imports, production-data
pulls, and broad unlock operations. Prompting remains TTY-only and script-safe:
`--json`, `--plan`, redirected I/O, and explicit automation bypasses never ask
for input.

Optional `init` starter selection is deferred because it is convenience UX, not
part of the destructive or missing-input guardrail contract.
