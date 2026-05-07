# 368 - Promote Prompt Policy for Builtin Unlock

Lane: [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](../033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Move the shared prompt policy to a crate surface that `effigy-builtin` can use
before implementing broad `unlock` confirmation.

## Context

`unlock` is implemented in `crates/effigy-builtin`, but the shared prompt
policy currently lives at `src/runner/prompt_policy.rs`. The lane requires one
prompt policy, so the built-in layer should not copy the same decision logic.

## Scope

- promote `PromptPolicy` and `PromptDecision` into an appropriate shared crate
  already available to both runner and built-ins
- update bootstrap and container prompt call sites to use the promoted policy
- keep behavior unchanged for existing prompt surfaces
- preserve the current prompt suppression rules:
  - no prompt for `--json`
  - no prompt for `--plan`
  - no prompt for explicit non-interactive bypasses
  - prompt only when stdin and stdout are TTYs
- add or move targeted tests so the policy remains covered after promotion

## Non-Goals

- implementing `unlock` confirmation
- changing current bootstrap or container prompt behavior
- adding another prompt policy abstraction

## Exit Condition

This card is complete when `effigy-builtin` can use the shared prompt policy
without depending on runner internals, and all existing prompt-policy tests
still pass.

## Next Card

[`369-implement-broad-unlock-confirmation.md`](./369-implement-broad-unlock-confirmation.md)
implements broad `unlock` confirmation using the promoted policy and `--yes`
as the explicit automation bypass.

## Closeout

`PromptPolicy` and `PromptDecision` now live in `effigy-builtin`, where the
runner and built-in `unlock` implementation can both use the same decision
rules. Existing bootstrap and container prompt call sites now import the
promoted policy.

Validation:

- `cargo check -p effigy-builtin`
- `cargo check -p effigy`
- `cargo test -p effigy-builtin prompt_policy -- --nocapture`
- `cargo test -p effigy --lib prompt_bootstrap_path_reuse -- --nocapture`
- `cargo test -p effigy --lib container_data_pull_production_prompt -- --nocapture`
- `cargo test -p effigy --lib container_data_import_prompt -- --nocapture`

## Next Task

Execute `369-implement-broad-unlock-confirmation.md`.
