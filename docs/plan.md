# Plan

Updated: 2026-09-26

## Candidates

- Bounded doctor cache pruning — reduce stale cache growth without changing the fast structural tier.
- Optional provider and S3 retirement — settle remaining feature placement questions before removing a surface.
- Release gate diagnosability and hosted evidence — make failures and exact-commit proof easier to inspect.
- Consumer adoption cohort expansion — test the current command surface against more real repositories.
- Ephemeral clone runtime scopes — apply worktree isolation and retirement to clones marked `effigy.runtimeScope = ephemeral` in local Git config; preserve unmarked checkout behavior. See the [runtime contract](knowledge/contracts/005-container-runtime-contract.md).

The [triage index](triage/README.md) owns unresolved leads.
