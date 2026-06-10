# Post-release Review And Text Projection Follow-up Boundary Decision

Date: 2026-04-16
Roadmap: `g02.010`
Card: `184`

## Summary

The release seam is no longer the blocker inside `g02.010`.

After `183`, the release-domain review/menu/text-projection surface is
crate-owned, the interactive release contract is revalidated, and the remaining
`src/runner/release_command.rs` mass is now mostly runner-shell orchestration:

- interactive command flow
- prompt/confirmation IO
- release-specific command dispatch and error routing
- final adapter glue around promoted `effigy-release` APIs

That is honest enough for the release seam itself to stop blocking on
modularization.

`g02.010` still does not pause, though, because `/src` is not yet clean at the
broader shell level. The remaining root-crate pressure is still real:

- `src/runner/demo_command.rs`
- `src/runner/release_command.rs`
- `src/runner/distribution_command.rs`
- `src/runner/container_command.rs`
- `src/runner/bootstrap_command.rs`
- `src/tui/demo_browser.rs`
- `src/runner/docs_command.rs`

So the next move is not release resumption. The next move is one broader shell
cleanup prioritization decision for the remaining `/src` seams.

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

Before this decision, the release seam and the broader `/src` cleanliness bar
were still conflated. After this decision, the release seam is explicitly
classified as good enough, while `g02.010` stays active because the remaining
shell-heavy root-crate seams still need one more cleanup program.

## Next Task

Execute
[`185-decide-next-src-shell-cleanup-priority-after-release-boundary.md`](../../../specs/batch-cards/185-decide-next-src-shell-cleanup-priority-after-release-boundary.md)
to choose the next meaningful `/src` cleanup seam.
