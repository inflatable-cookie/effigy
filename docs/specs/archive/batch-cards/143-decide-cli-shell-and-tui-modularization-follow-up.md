# 143 Decide CLI Shell And TUI Modularization Follow-Up

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining pre-`v0.3` modularization work should continue
through a CLI-shell crate, a TUI/runtime crate, or both in sequence.

## In Scope

- assess the remaining `src/` hotspots after the current domain extractions
- classify the CLI parse/help/command-model cluster honestly
- classify the TUI/browser/runtime cluster honestly
- choose the next bounded extraction batch instead of pausing on a soft claim

## Out Of Scope

- executing the release lane in the same batch
- broad cleanup without a crate-boundary decision
- consumer rollout work

## Acceptance Criteria

- the next remaining modularization seam is explicit
- the release lane posture is honest
- one clear next batch is left ready

## Validation

- docs/state surfaces updated honestly
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

The next real seam is CLI shell extraction first.

The command model and parse grammar are still bounded, reusable shell-facing
contracts. By contrast, the remaining TUI/runtime surface is larger and should
follow after the CLI shell no longer inflates `src/lib.rs` and
`src/cli/parse/command_parsing.rs`.

## Next Task

Execute [`144-implement-effigy-cli-foundation-extraction.md`](./144-implement-effigy-cli-foundation-extraction.md).
