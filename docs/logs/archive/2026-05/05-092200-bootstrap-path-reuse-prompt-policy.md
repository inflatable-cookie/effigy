# Bootstrap Path Reuse Prompt Policy

Date: 2026-05-05
Roadmap: `g03.027`
Batch card: `362`

## Outcome

Card `362` is complete.

Effigy now has a shared runner prompt policy for normal CLI surfaces:

- real stdin and stdout TTY required
- no prompts for `--json`
- no prompts for `--plan`
- explicit `--no-prompt` remains the automation bypass

`effigy bootstrap` now uses that policy before reusing an existing non-empty
destination path. Interactive terminal use asks for confirmation before
clone/update work proceeds. Non-interactive execution fails clearly unless
`--no-prompt` makes reuse explicit.

## Validation

- `cargo fmt --all -- --check`
- `cargo test prompt_policy --lib`
- `cargo test -p effigy --lib existing_non_empty`
- `cargo test -p effigy --lib no_prompt_bypasses_existing_checkout_confirmation`
- `cargo test -p effigy --lib plan_skips_existing_destination_prompt`
- `cargo test -p effigy --lib prompt_bootstrap_path_reuse`

## Vision Target Delta

Primary tags: `OPERATE`, `CONTRACT`

Baseline: bootstrap had one DB-seed prompt seam but no reusable prompt policy
or destination-reuse confirmation.

Current: prompt policy is explicit in code and docs, and bootstrap path reuse
is guarded without changing script-safe modes.

Remaining: decide whether to widen directly into destructive container/data
prompts or add one helper hardening slice first.

## Next Task

Execute `363-decide-post-bootstrap-path-reuse-prompt-boundary.md`.
