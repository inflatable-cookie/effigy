# Next Src Shell Cleanup Priority After Release Boundary Decision

Date: 2026-04-16
Roadmap: `g02.010`
Card: `185`

## Summary

The next `/src` cleanup seam is bootstrap.

I did not reopen the demo or release seams even though their runner files are
still large, because those seams were already paused on explicit shell-only
boundaries:

- demo runner: command entry/render wiring, task/run dispatch orchestration,
  raw process launch and supervisor integration, final runner adapter behavior
- release runner: interactive command flow, prompt/confirmation IO, runner-side
  dispatch/error routing, final adapter glue around `effigy-release`

Reopening either of those now would mostly be a line-count reaction, not a
stronger domain-boundary discovery.

Bootstrap is different:

- `src/runner/bootstrap_command.rs` is still fully root-crate owned
- it is still a bounded product surface
- it still contains real request/execution contracts, not only final CLI shell
  glue
- it has not yet been promoted into a workspace boundary at all

That makes bootstrap the next honest cleanup target for the user’s `/src is
clean` bar.

The strongest remaining root-crate pressure after that still includes:

- `src/runner/demo_command.rs`
- `src/runner/release_command.rs`
- `src/runner/distribution_command.rs`
- `src/runner/container_command.rs`
- `src/runner/bootstrap_command.rs`
- `src/tui/demo_browser.rs`
- `src/runner/docs_command.rs`

But bootstrap is the best next move because it is both bounded and still
unextracted, while the larger demo/release shells were already explicitly
classified as honest shell boundaries.

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

Before this decision, the post-release state only said that `/src` was still
not clean enough. After this decision, the next cleanup seam is explicit and
defensible: bootstrap is now the next bounded product surface to extract
without churning already-paused shell seams.

## Next Task

Execute
[`186-implement-effigy-bootstrap-foundation-extraction.md`](../../specs/batch-cards/186-implement-effigy-bootstrap-foundation-extraction.md)
to extract the first real bootstrap workspace boundary.
