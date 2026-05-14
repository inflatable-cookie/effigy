# g06.007 - Compatibility Branch Audit And Deletion

Status: Complete
Depends on: `g06.001`

## Goal

Delete behavior branches that only exist for old surfaces or migration paths
that Effigy no longer promises to support.

## Evidence

- Effigy carries multiple generations of release, routing, JSON, config, and
  task behavior
- some compatibility code is essential because released-surface baselines still
  require it
- some compatibility code likely persists only because nobody has proved it is
  dead yet

## Scope

- inventory branches, flags, fallback paths, and migration shims that exist
  only for older behavior
- classify each one as required, deferred, or deletable
- delete dead branches only where current guides/contracts/released-surface
  proof no longer require them
- record explicit reasons for retained compatibility

## Out Of Scope

- no speculative deletion
- no release-surface break unless a roadmap explicitly opens it
- no changelog/history scrubbing
- no removal of compatibility still required by current baselines

## Guardrails For A Cheaper Model

- require concrete evidence before deleting a compatibility branch
- use released-surface tests as authority for user-visible promises
- prefer full branch deletion over wrapping dead behavior in new abstraction
- document retained compatibility debt instead of hand-waving it

## Suggested Implementation Steps

1. Inventory legacy/fallback branches by command family.
2. Compare each one against guides, contracts, and released-surface tests.
3. Delete only the clearly dead paths.
4. Update tests and docs where the deletion removes stale internal assumptions.
5. Record retained debt for later batches.

## Acceptance Criteria

- dead compatibility branches are deleted with proof
- retained branches have explicit rationale
- released-surface baselines stay green
- net code volume and branch complexity decrease meaningfully

## Outcome

- deleted stale host-native routing for `catalogue`, which active parser tests
  and current docs no longer treat as a builtin command
- deleted flat `docs check-*` parser compatibility shims that only emitted
  migration errors for retired spellings
- retained active compatibility where proof still requires it:
  - `release resume`
  - `--dry-run`
  - `--allow-stale`
  - migration-sensitive runtime and gateway branches without released-surface
    proof of deadness

## Validation

Minimum focused validation:

```bash
cargo run --bin effigy -- qa:released-surface --repo .
cargo test
```

## Next Task

Completed. Next cleanup lane is `g06.008`.
