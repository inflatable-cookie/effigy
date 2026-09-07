# 123 Cargo Link Version Transition Strict Lane

Status: Satisfied (2026-09-07); PR open for coordinator review
Owner: Effigy orchestrator
Created: 2026-09-07
Roadmap: [`g09.008`](../roadmaps/g09/008-cargo-link-version-transition.md)
Ready card: [`1116`](../roadmaps/g09/batch-cards/1116-cargo-link-version-transition.md)
Contract: [`034`](../contracts/034-local-dependency-linking-contract.md)
Guide: [`077`](../guides/077-local-dependency-linking.md)

## Outcome

A Cargo link across a local version bump applies, verifies from local paths,
and on failure leaves config and every affected lockfile exactly at baseline.

## Fixed Decisions

- Version transition is detected per matched package by comparing the local
  package version with the locked source version from locked metadata, and
  recorded in the plan and report.
- Refresh is targeted: only affected linked packages, inside the existing
  link transaction, after the patch is written and before verification. No
  `--workspace` update, no refresh of packages the link does not own.
- Every affected `Cargo.lock` is snapshotted before the first write and
  restored with the config on any failure, refusing only if the lock changed
  after Effigy's own write (the existing config rule).
- `[[patch.unused]]` for a linked package is a named verification failure,
  never a silent Git resolution.
- Contract `034` lockfile-safety gains two lines: transition refresh is
  link-owned drift; a failed link restores affected locks byte-for-byte.
- Not allowed: global lock rewrite, Bun changes, release or workflow
  mutation, consumer-repository edits, weakening the dirty-lock refusal or
  the unlink byte-for-byte rule.

## Whole-Lane Review Oracle

Reject the lane if any counterexample survives:

1. The `v0.4.3` to `v0.4.4` fixture still yields `[[patch.unused]]` or Git
   resolution for any linked package.
2. A forced verification failure leaves any affected `Cargo.lock` or config
   different from baseline.
3. An unrelated lockfile or an unlinked package's lock entry changes.
4. Refresh runs wider than the affected packages.
5. Unlink no longer returns the lock byte-for-byte, or the pre-link
   dirty-lock refusal weakens.
6. Contract `034` or guide `077` does not record the transition case.

## Validation And Evidence

Card `1116` maps every oracle row to proof: focused deps tests, the real
transition fixture, the rollback regression, `effigy qa` or targeted QA,
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
`git diff --check`. One dated evidence log.

## Stop Conditions

Stop and return facts if the fix needs a refresh wider than affected
packages, a change outside the existing link transaction, a new flag, or a
contract `034` change beyond the two lines above.

## Next Task

Card `1116` is executed and its evidence log
[`07-162725-cargo-link-version-transition-1116`](../logs/2026-09/07-162725-cargo-link-version-transition-1116.md)
answers every oracle row. The coordinator reviews and merges.
