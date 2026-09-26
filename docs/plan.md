# Plan

Updated: 2026-09-26

## Now

**Isolate container-backed worker worktrees.** A worker must run tests against
its own checkout, Compose project, mutable volumes, ports and gateway names.
Effigy owns runtime identity, route ownership, effective host discovery and
verifiable cleanup; Queue owns the retirement trigger and durable retry. Keep
the main checkout's existing hostnames. Worker stacks use distinct base domains
such as `<repo>-w<scope>.test` so their app and helper hosts remain together.
Prove two live worktrees stay independent when one starts, tests and retires.
The [worktree isolation lead](triage/20260917-230600-per-worktree-container-identity-default.md)
records the failures and open runtime observations. Implement through Queue
tasks, with Effigy's runtime contract first and consumer adoption after it.

## Candidates

- Bounded doctor cache pruning — reduce stale cache growth without changing the fast structural tier.
- Optional provider and S3 retirement — settle remaining feature placement questions before removing a surface.
- Release gate diagnosability and hosted evidence — make failures and exact-commit proof easier to inspect.
- Consumer adoption cohort expansion — test the current command surface against more real repositories.

The [triage index](triage/README.md) owns unresolved leads.
