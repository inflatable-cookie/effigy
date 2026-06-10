# Env Schema Public API Integration

Status: complete
Created: 2026-03-10
Roadmap: g01.025
Batch: module-integration-api

## Summary

- Rounded out the public env-schema library API with explicit load/resolve/validate helpers.
- Added default `.env.schema` autodetection helpers at the library level.
- Added `ResolvedEnv` export helpers that return `HashMap<String, EnvValue>` and reconciled the section-5 roadmap items against Effigy's actual module layout.

## Changes

- Extended `src/env_schema.rs` with `detect_schema_path`, `load_env_schema`, `load_env_schema_if_present`, `resolve_env`, `validate_env`, and `resolve_and_validate_env`.
- Kept the existing `load_and_resolve` entry point but refactored it to use the new public helpers instead of duplicating the pipeline inline.
- Added `EnvValue` as the public alias for resolved env values and export helpers on `ResolvedEnv` in `src/env_schema/resolver.rs`.
- Added clone support for secret/plain resolved values so exported maps preserve sensitivity information cleanly.
- Expanded `tests/env_schema_tests.rs` with autodetection, explicit resolve/validate flow, and `HashMap<String, EnvValue>` export coverage.
- Marked the module-integration roadmap checklist complete, noting that the shipped public surface lives in `src/env_schema.rs` rather than the older roadmap sketch path.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`
- Movement: baseline `env-schema features existed but the public Rust API was still a thin wrapper around internal module functions` -> current `the crate exposes a coherent library surface for loading, autodetecting, resolving, validating, and exporting env-schema values`
- Remaining gap: `the main unresolved roadmap areas are config/checklist reconciliation and a few still-open security/resolution verification targets`

## Validation Performed

- command: `cargo fmt --all -- --check`
  - result: pass
- command: `cargo test --test env_schema_tests -- --nocapture`
  - result: pass
- command: `cargo test public_resolve_and_validate_env_surfaces_validation_errors_without_reloading -- --nocapture`
  - result: pass
- command: `cargo test resolved_env_exports_hash_map_of_env_values -- --nocapture`
  - result: pass
- command: `git diff --check`
  - result: pass

## Risks

- The roadmap still uses some earlier naming (`src/env/mod.rs`, `EnvValue`) that now maps onto the real shipped surface (`src/env_schema.rs`, `EnvValue` alias over `ResolvedValue`); future roadmap updates should keep that translation explicit to avoid confusion.
- Export helpers clone secret values when producing `HashMap<String, EnvValue>`; that is correct for API ergonomics, but callers should avoid holding unnecessary copies of secret values longer than needed.

## Next Task

- Implement the next `g01.025` configuration-alignment batch: add focused tests and cleanup around `[env_schema]` manifest behavior (`enabled`, `schema`, `exec_timeout`), then reconcile the remaining section-7 roadmap items against what Effigy already ships versus what still needs explicit validation.
