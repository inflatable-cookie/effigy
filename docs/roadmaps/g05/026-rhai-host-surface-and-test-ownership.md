# g05.026 - Rhai Host Surface And Test Ownership

Status: Planned
Depends on: `g05.021`

## Goal

Make the Rhai host surface easier to reason about by splitting tests by surface
owner and aligning docs with the actual provider-facing capability set.

This supports external provider packages without broadening core provider
behavior.

## Evidence

- `effigy scan god-files --json` flags `crates/effigy-rhai/src/tests.rs` at
  2120 lines
- `docs/guides/068-rhai-host-surface-audit.md` lists JSON and TOML structured
  data helpers but omits YAML
- YAML helpers exist in `crates/effigy-rhai/src/host_api/utility.rs`
- provider package contract docs say core must expose JSON/TOML/YAML helpers,
  deploy context/report helpers, HTTP, process helpers, and scoped paths

## Scope

- split `crates/effigy-rhai/src/tests.rs` into focused test modules
- suggested test groups: filesystem/data, deploy provider, secrets, process,
  HTTP, execution, config/tasks/catalog, scan/docs/system
- keep test names descriptive and preserve existing coverage
- update Rhai host-surface docs for YAML helpers and any provider-relevant
  helper names found during the split
- check whether provider scripts still need raw process/env access after the
  documented helper set

## Out Of Scope

- no removal of existing Rhai helpers
- no provider package implementation work
- no new command surface unless a missing helper is explicitly documented and
  accepted
- no recursive `effigy` calls from first-party Rhai scripts

## Guardrails For A Cheaper Model

- this is primarily structural; avoid behavior changes
- move tests in small groups and run focused tests after each group
- keep helper registration code untouched unless tests expose a real mismatch
- if a helper is undocumented, update docs; if a helper is missing, write a
  follow-up note rather than inventing it mid-refactor
- do not weaken secret redaction or first-party process allowlist tests

## Suggested Implementation Steps

1. Create test modules under `crates/effigy-rhai/src/tests/` or equivalent local
   module layout used by the crate.
2. Move provider context/report tests into a deploy-provider test module.
3. Move YAML/JSON/TOML tests into a structured-data test module.
4. Move process and first-party script allowlist tests together.
5. Update docs for YAML and any renamed group labels.
6. Rerun Rhai tests and god-file scan.

## Acceptance Criteria

- `crates/effigy-rhai/src/tests.rs` is no longer a god-file
- Rhai host-surface docs match exposed structured-data helpers
- provider-facing Rhai tests are easy to find
- all moved tests pass without weakened assertions

## Validation

Minimum focused validation:

```bash
cargo test -p effigy-rhai
effigy scan god-files --json
```

## Next Task

After Rhai ownership is clearer, move to `g05.027` for the process execution
boundary review.
