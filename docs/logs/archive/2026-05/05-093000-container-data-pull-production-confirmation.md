# Container Data Pull-Production Confirmation

Date: 2026-05-05
Roadmap: `g03.027`
Batch card: `364`

## Outcome

Card `364` is implemented.

`effigy container data pull-production` now uses the shared prompt policy before
production data is pulled into the local generated-compose environment:

- real TTY runs prompt and default to no
- `--json` and non-TTY runs fail clearly instead of prompting
- automation can pass `--yes`
- Rhai pull-production calls use the explicit bypass internally

## Validation

- `cargo check -p effigy-cli`
- `cargo check -p effigy`
- `cargo test -p effigy --lib parse_container_data_pull_production -- --nocapture`
- `cargo test -p effigy --lib container_data_pull_production_prompt -- --nocapture`
- `cargo test -p effigy --lib run_container_data_pull_production_rejects_direct_compose_ownership -- --nocapture`
- `cargo test -p effigy --lib prompt_container_data_pull_production -- --nocapture`

`cargo fmt --all -- --check` is currently blocked by unrelated formatting drift
in dirty files outside this card's change set.

## Vision Target Delta

Primary tags: `OPERATE`, `CONTRACT`

Baseline: production data pull had no final interactive confirmation.

Current: production data pull has a bounded confirmation contract and an
explicit automation bypass.

Remaining: decide whether `container data import` or broad `unlock` should be
the next prompt seam.

## Next Task

Execute `365-decide-post-container-data-pull-production-confirmation-boundary.md`.
