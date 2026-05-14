# g06.001 - Codebase Lean-Down Suite

Status: Complete
Depends on: none

## Goal

Reduce Effigy's maintenance weight without weakening behavior, contracts, or
release safety.

This suite is the umbrella for codebase-size reduction work after `v0.7.0`.
The standard for success is not "smaller files". The standard is fewer real
owners for the same behavior.

## Evidence

- Effigy now carries roughly 233k lines of Rust
- recent `g05` cleanup removed real duplication, but major weight remains in
  state, release, CLI rendering, fixtures, compatibility branches, and
  runner-private domain logic
- the remaining large files are not random; they cluster around mixed
  responsibilities
- repeated JSON shape builders and repeated test scaffolds still increase both
  LOC and drift risk

## Ordered Follow-Up Lanes

1. [`002-state-command-domain-split-and-shell-trim.md`](./002-state-command-domain-split-and-shell-trim.md)
2. [`003-release-domain-split-and-lib-reduction.md`](./003-release-domain-split-and-lib-reduction.md)
3. [`004-shared-fixture-and-test-support-convergence.md`](./004-shared-fixture-and-test-support-convergence.md)
4. [`005-cli-help-and-rendering-deduplication.md`](./005-cli-help-and-rendering-deduplication.md)
5. [`006-typed-contract-shape-reuse-and-json-builder-reduction.md`](./006-typed-contract-shape-reuse-and-json-builder-reduction.md)
6. [`007-compatibility-branch-audit-and-deletion.md`](./007-compatibility-branch-audit-and-deletion.md)
7. [`008-runner-private-domain-logic-reduction.md`](./008-runner-private-domain-logic-reduction.md)

## Execution Guardrails

- do not pursue minified code or denser syntax as a size strategy
- prefer deletion over abstraction when behavior is truly dead
- prefer narrow shared helpers over broad new framework layers
- every moved or deleted branch must stay covered by focused tests or released
  surface proof
- do not break JSON contracts, CLI contracts, or release protocol for LOC
  wins
- do not reopen old provider/product-specific concepts in core

## Non-Goals

- no rewrite generation
- no "merge crates because crate count feels high" work without boundary proof
- no UI/UX redesign work
- no release protocol weakening
- no opportunistic cleanup in `external/`

## Acceptance Criteria

- the largest ownership seams have narrower and clearer module boundaries
- repeated fixture, rendering, and JSON-shape code is materially reduced
- dead compatibility codepaths are deleted where contracts permit
- residual large modules and retained duplication are explicitly justified
- closeout includes before/after measurements and accepted residual risk

## Outcome

- state and release god-file targets were reduced below the warning threshold
- deploy test-fixture duplication was converged under shared owners
- CLI help topic layout boilerplate was reduced
- release JSON wire models now have a typed owner
- dead compatibility branches were removed where proof allowed it
- state runner-private domain logic moved under `effigy-state`
- the tranche closed with `0` god-file findings, `93` duplicate-block
  findings, and `4` remaining high duplicate findings

## Suggested Batch Cards

- baseline current line counts, duplicate findings, and warning-level god files
- trim `state_command.rs` into thinner adapter/domain seams
- split `effigy-release/src/lib.rs` by durable release concepts
- converge demo/release/bootstrap test support
- reduce CLI help/render layout duplication
- replace repeated dynamic JSON assembly with typed reusable shapes
- inventory and delete dead compatibility branches
- move durable domain logic out of runner command modules
- close proof with metrics and retained-risk notes

## Next Task

Closed through `809`.
