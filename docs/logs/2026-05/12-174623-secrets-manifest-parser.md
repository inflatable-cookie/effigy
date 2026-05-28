# Secrets Manifest Parser

Date: 2026-05-12

## Summary

Completed card `703`, the typed `[secrets]` manifest parser slice.

## Changes

- added `ManifestSecretsConfig`
- added typed backend, vault, external, unlock, identity, key, and target
  declaration models
- exposed `TaskManifest.secrets`
- added `secrets_config` parser tests
- advanced ready work to card `704`

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Baseline: repos could not declare secret names and targets in the manifest.
- Current state: manifests can parse declaration-only `[secrets]` config.
- Remaining open: read-only command surface, vault storage, unlock, injection,
  container startup integration, Underlay/Example App migration proof, and
  Varlock adapter decision.

## Validation

- `cargo test -p effigy-manifest --test secrets_config`
- `cargo test -p effigy-manifest secrets_config`
- `cargo check --all-targets`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute `704` to add read-only `secrets list` and `secrets doctor`.

