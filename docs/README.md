# Effigy — current state

Effigy runs manifest tasks and built-in workflows across monorepos. Linked worktrees and clones marked `effigy.runtimeScope = ephemeral` get distinct generated-Compose identities and effective gateway hosts by default, and gateway routes reject live foreign claims. `effigy container hosts` exposes the host map; `effigy container retire` tears down one scope's owned resources. Consumer applications still need to feed those names into public URLs and selectors; Queue lead `af3ab7c7-b631-4d4f-9da7-f2949ded82f3` tracks that wiring and an observed workspace recycle.

## By topic

- [Vision](knowledge/vision.md)
- [Architecture and ownership](knowledge/README.md)
- [Contracts and JSON schemas](knowledge/contracts/README.md)
- [Release procedure](knowledge/contracts/release.md)
- [User and consumer guides](guides/README.md), including [managed task sessions](guides/012-dev-process-manager-tui.md)
- [Bounded QA groups workflow](guides/081-bounded-qa-groups-workflow.md) (implemented through `tasks qa-group`; `stop`/`hard_timeout_ms` wait on owned-run supervision, contract 052)
- [Owned run supervision workflow](guides/082-owned-run-supervision-workflow.md) (proposed; QA-group `stop` and `hard_timeout_ms` remain unavailable under contract 052. Queue/Nucleus scheduling is implemented under contract 049; the former Effigy admission store and query commands are retired, as documented in [guide 080](guides/080-host-wide-validation-admission.md).)
- [v0.14.0 consumer migration](guides/083-v0.14.0-consumer-migration.md) (migration checklist and incremental first-parent change coverage)

## What's next

The project's plan is in Queue: its lanes, their documents and their order. Queue also holds leads, brief drafts, tasks, status and outcomes.
