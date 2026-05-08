# 229 Implement Effigy CLI Help Extraction

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the root-owned CLI help topic surface into `effigy-cli` so help text,
topic registration, and shared help rendering stop living in `src/cli_help/**`.

## In Scope

- inspect `src/cli_help/**`
- move command help topics and shared topic helpers into `crates/effigy-cli/**`
- move any help-topic registry or shared rendering helpers that belong with the
  CLI contract rather than the root crate
- leave root-crate callers limited to final dispatch into the CLI/help layer

## Out Of Scope

- release execution
- demo/docs/container parallel cleanup
- broader CLI parse/model work already owned by `effigy-cli`
- speculative UI extraction outside the help surface

## Acceptance Criteria

- `src/cli_help/**` gets materially smaller or becomes a thin compatibility
  shell
- reusable help-topic ownership moves into `effigy-cli`
- the next move is a boundary decision, not another guessed CLI-help slice

## Validation

- `cargo test -p effigy-cli`
- `cargo test help_and_flag_tests --lib`
- `cargo test --test cli_output_tests help_and_flags_tests`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`230-decide-post-cli-help-extraction-boundary.md`](./230-decide-post-cli-help-extraction-boundary.md)
to classify the remaining `src/cli_help.rs` shell after the topic surface moved
into `effigy-cli`.
