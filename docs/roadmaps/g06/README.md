# g06 Roadmaps

Status: Complete
Theme: Codebase lean-down and ownership simplification

## Purpose

`g06` exists to make Effigy smaller by making it clearer.

The target is not cosmetic line reduction. The target is less duplicated
ownership, fewer parallel codepaths, fewer runner-private domain surfaces, and
less contract drift to defend.

Effigy is now large enough that code volume is a product risk on its own.
`g06` treats that as a maintainability and release-safety problem.

## Roadmap Sequence

- [`001-codebase-lean-down-suite.md`](./001-codebase-lean-down-suite.md)
- [`002-state-command-domain-split-and-shell-trim.md`](./002-state-command-domain-split-and-shell-trim.md)
- [`003-release-domain-split-and-lib-reduction.md`](./003-release-domain-split-and-lib-reduction.md)
- [`004-shared-fixture-and-test-support-convergence.md`](./004-shared-fixture-and-test-support-convergence.md)
- [`005-cli-help-and-rendering-deduplication.md`](./005-cli-help-and-rendering-deduplication.md)
- [`006-typed-contract-shape-reuse-and-json-builder-reduction.md`](./006-typed-contract-shape-reuse-and-json-builder-reduction.md)
- [`007-compatibility-branch-audit-and-deletion.md`](./007-compatibility-branch-audit-and-deletion.md)
- [`008-runner-private-domain-logic-reduction.md`](./008-runner-private-domain-logic-reduction.md)

## Execution Rule

Open strict batch cards only when implementation starts.

Do not optimize for raw line count. Every deletion must preserve or improve:

- contract clarity
- test confidence
- operator predictability
- domain ownership boundaries

## Batch Card Shape

Recommended first cards:

```text
800-open-codebase-lean-down-lane.md
801-baseline-size-duplication-and-god-file-metrics.md
802-trim-state-command-domain-ownership.md
803-trim-release-lib-domain-ownership.md
804-converge-demo-release-and-bootstrap-test-fixtures.md
805-converge-cli-help-topic-layout-machinery.md
806-type-shared-json-contract-shapes.md
807-audit-and-delete-dead-compat-branches.md
808-reduce-runner-private-domain-logic.md
809-close-g06-proof-and-residual-risk.md
```

## Current State

`g05` is closed through `g05.027`.

Accepted starting evidence for `g06`:

- Rust-only code size is roughly 233k lines
- `src/runner/state_command.rs` remains a warning-level god file
- `crates/effigy-release/src/lib.rs` remains a warning-level god file
- duplicate-block findings still remain after the `g05` cleanup tranche
- several concurrent-runner demo CLI tests recently needed stronger shared
  serialization discipline, which is a sign that test support should own more
  of that lifecycle
- provider/export/state/release/demo surfaces still show repeated JSON shape
  construction and runner-private domain logic

## Next Task

`g06.001` is closed. No active `g06` execution card remains.
