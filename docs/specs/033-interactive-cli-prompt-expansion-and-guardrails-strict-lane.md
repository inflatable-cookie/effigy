# 033 - Interactive CLI Prompt Expansion and Guardrails Strict Lane

Roadmap: [`g03.027`](../roadmaps/g03/027-interactive-cli-prompt-expansion-and-guardrails.md)

Status: Active
Owner: Platform
Created: 2026-05-05

## Purpose

Turn the bootstrap DB-seed prompt from a one-off success into a bounded CLI
interaction contract that stays script-safe.

This lane exists to:

- define one prompt policy for Effigy CLI surfaces
- keep prompts limited to true TTYs and explicit operator flows
- add the next highest-value prompt seam first
- widen only into bounded destructive or missing-input cases

## Hard Boundaries

- prompts must never appear for `--json`, `--plan`, or non-TTY execution
- prompts must not alter the underlying command contract; they only complete or
  confirm input before normal execution
- release stays on its own dedicated interactive flow
- do not build a general wizard framework in this lane
- every destructive prompt surface must keep an explicit non-interactive escape
  hatch such as `--yes`, `--force`, or `--no-prompt`

## Execution Order

1. Land the shared prompt policy plus bootstrap existing-path reuse
   confirmation.
2. Widen into destructive container/data actions.
3. Widen into broad unlock confirmation.
4. Decide whether optional `init` starter selection is still warranted, or
   whether the lane can close without it.

## Exit Condition

This lane closes when:

- the prompt policy is explicit and documented
- bootstrap existing-path reuse follows it
- `container data pull-production`, `container data import`, and broad `unlock`
  follow it
- the non-interactive suppression rules are proven

## Current Ready Card

- [`batch-cards/368-promote-prompt-policy-for-builtin-unlock.md`](./batch-cards/368-promote-prompt-policy-for-builtin-unlock.md)

## Next Task

Execute `368-promote-prompt-policy-for-builtin-unlock.md`.
