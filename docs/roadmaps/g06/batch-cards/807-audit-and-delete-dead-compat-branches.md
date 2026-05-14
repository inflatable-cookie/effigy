# 807 - Audit And Delete Dead Compat Branches

Roadmap: [`../007-compatibility-branch-audit-and-deletion.md`](../007-compatibility-branch-audit-and-deletion.md)
Strict lane: [`../../../specs/084-codebase-lean-down-strict-lane.md`](../../../specs/084-codebase-lean-down-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Delete compatibility branches that survive only as internal debt and are no
longer required by current guides, contracts, or released-surface baselines.

## Scope

- inventory legacy flags, shims, and fallback branches
- classify each one as required, deferred, or deletable
- delete only branches with concrete proof

## Acceptance

- dead compatibility paths are removed
- retained compatibility has explicit rationale
- released-surface baselines stay green

## Outcome

- removed stale `catalogue` host-native routing so it now stays task-routed
  consistently
- removed the flat `docs check-*` parser shims that only served migration-error
  messaging for retired spellings
- retained compatibility still required by active guides, contracts, or
  released-surface proof:
  - `release resume`
  - `--dry-run`
  - `--allow-stale`
  - runtime and gateway migration compatibility not yet independently proved
    dead

## Suggested Validation

```bash
cargo run --bin effigy -- qa:released-surface --repo .
cargo test docs_and_contracts_option_tests
cargo test help_and_flag_tests
```

## Next Task

Execute `808`.
