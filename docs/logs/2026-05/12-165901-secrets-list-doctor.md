# Secrets List Doctor

Implemented ready card `704` for `g05.002`.

## Vision Target Delta

Effigy now has a read-only secrets command surface:

- `effigy secrets list`
- `effigy secrets doctor`

The commands inspect only declaration metadata from `[secrets]`. They do not
create vault files, unlock vaults, read values, inject values, or modify
runtime/container state.

## Evidence

- added CLI parsing and help for `effigy secrets`
- added runner dispatch for read-only list/doctor reports
- added `effigy.secrets.v1` JSON payload output
- added parser and runner tests proving missing-section behavior, blocker
  diagnostics, and value-free output
- updated command reference with the new command surface

## Validation

- `cargo test secrets_option_tests`
- `cargo test secrets_tests`
- `cargo test command_kind_and_name_maps_command_variants`
- `cargo test help_topic_label_maps_all_topics`
- `cargo check --all-targets`

## Next Task

Execute `705` to document JSON examples and close `g05.002`.
