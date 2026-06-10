# Prompt Policy Promotion for Builtin Unlock

Date: 2026-05-05
Roadmap: `g03.027`
Batch Card: `368-promote-prompt-policy-for-builtin-unlock.md`

## Summary

The shared prompt policy moved from runner-only code into `effigy-builtin`.
Runner prompt call sites now import the promoted policy, and built-in `unlock`
can use the same rules in the next implementation card.

## Changed

- added `PromptPolicy` and `PromptDecision` to `effigy-builtin`
- removed the runner-private prompt policy module
- updated bootstrap path-reuse and container data prompt call sites

## Validation

- `cargo check -p effigy-builtin`
- `cargo check -p effigy`
- `cargo test -p effigy-builtin prompt_policy -- --nocapture`
- `cargo test -p effigy --lib prompt_bootstrap_path_reuse -- --nocapture`
- `cargo test -p effigy --lib container_data_pull_production_prompt -- --nocapture`
- `cargo test -p effigy --lib container_data_import_prompt -- --nocapture`

## Vision Target Delta

Primary tags: `CONTRACT`, `OPERATE`, `MAINT`

Baseline: prompt policy was shared only inside runner code, blocking built-in
`unlock` from using it without duplication.

Current state: prompt policy is exported from `effigy-builtin`, so runner and
built-in prompt surfaces can share one decision rule.

Remaining open: implement broad `unlock` confirmation with `--yes`.
