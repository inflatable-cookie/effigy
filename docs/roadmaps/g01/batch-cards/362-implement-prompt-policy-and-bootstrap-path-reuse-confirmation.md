# 362 - Implement Prompt Policy and Bootstrap Path-Reuse Confirmation

Lane: [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](../033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05

## Goal

Land the shared prompt policy for CLI surfaces, then use it for the first new
prompt seam: bootstrap reuse of a non-empty destination path.

## Scope

- define the common prompt policy in code and docs:
  - real stdin + stdout TTY only
  - never for `--json`
  - never for `--plan`
  - never when an explicit non-interactive bypass is present
- add bootstrap confirmation when:
  - `effigy bootstrap --path <DIR>` targets an existing non-empty directory
  - or the default clone destination already exists and is non-empty
- keep the prompt bounded:
  - show the resolved destination path
  - confirm reuse before clone/update work proceeds
- preserve non-interactive behavior:
  - fail clearly instead of prompting
- add targeted tests for:
  - TTY prompt-eligible path reuse
  - non-TTY suppression
  - `--json` suppression
  - `--plan` suppression
  - explicit non-interactive bypass
- update help/docs/changelog

## Non-Goals

- container/data prompts
- unlock prompts
- `init` starter selection
- generic prompt framework beyond what this seam needs

## Exit Condition

This batch is complete when bootstrap path reuse follows the new prompt policy
and the policy is documented strongly enough to widen into destructive
container/data actions next.

## Closeout

Completed: 2026-05-05

- added a shared runner prompt policy for TTY, `--json`, `--plan`, and explicit
  non-interactive suppression rules
- gated bootstrap reuse of existing non-empty destinations before clone/update
  work proceeds
- kept `--no-prompt` as the explicit automation bypass for intentional reuse
- documented the policy in bootstrap help, the bootstrap guide, and the
  changelog

Validation:

- `cargo fmt --all -- --check`
- `cargo test prompt_policy --lib`
- `cargo test -p effigy --lib existing_non_empty`
- `cargo test -p effigy --lib no_prompt_bypasses_existing_checkout_confirmation`
- `cargo test -p effigy --lib plan_skips_existing_destination_prompt`
- `cargo test -p effigy --lib prompt_bootstrap_path_reuse`

## Next Card

- [`363-decide-post-bootstrap-path-reuse-prompt-boundary.md`](./363-decide-post-bootstrap-path-reuse-prompt-boundary.md)
