# Effigy Release Review And Text Projection Follow-up Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Card: `183`

## Summary

Moved the remaining release review/menu/text-projection layer into
`effigy-release` and reduced `src/runner/release_command.rs` to a thinner
interactive adapter shell.

The batch extracted:

- release review state types
- prepare/execute/resume review menu rendering
- blocked-preflight and stale/drift review shaping
- prepare/simulate/prepared/resume/execute/gates/verify-install text
  projections

The batch also closed two real runtime regressions exposed by the extraction:

- the interactive prepare apply path was incorrectly treating any selected
  version as a custom override
- that same path was dropping the caller's `check_gates` intent when applying
  the prepared release

## Validation

- `cargo test -p effigy-release`
- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

Before this batch, the release seam was still split between a promoted release
crate and one large runner-owned review/text layer. After this batch, the
release-specific review and text-projection contract is crate-owned, and the
remaining runner shell is much closer to a final honest adapter boundary.

## Next Task

Execute
[`184-decide-post-release-review-and-text-projection-follow-up-boundary.md`](../../../specs/batch-cards/184-decide-post-release-review-and-text-projection-follow-up-boundary.md)
to decide whether `g02.010` can finally pause for `g02.007` release resumption.
