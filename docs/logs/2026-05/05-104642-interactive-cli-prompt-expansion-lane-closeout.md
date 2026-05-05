# Interactive CLI Prompt Expansion Lane Closeout

Date: 2026-05-05
Roadmap: `g03.027`
Batch Card: `371-close-interactive-cli-prompt-expansion-lane.md`

## Summary

`g03.027` is complete. The prompt guardrail lane now has a shared prompt policy
and bounded destructive prompts for bootstrap destination reuse,
generated-compose data imports, production-data pulls, and broad unlocks.

## Closed Scope

- shared `PromptPolicy` / `PromptDecision`
- bootstrap existing non-empty destination confirmation
- `container data pull-production` confirmation
- `container data import` confirmation
- broad `unlock` confirmation
- documented explicit automation bypasses
- optional `init` starter selection deferred out of this lane

## Validation

- `git diff --check`

## Vision Target Delta

Primary tags: `CONTRACT`, `OPERATE`, `MAINT`

Baseline: the CLI prompt policy was inconsistent outside a one-off bootstrap
DB-seed prompt.

Current state: prompt policy is explicit, shared, and applied to the lane's
destructive guardrail surfaces while preserving script-safe suppression.

Remaining open: none for `g03.027`. Next roadmap selection should happen
deliberately from planning.
