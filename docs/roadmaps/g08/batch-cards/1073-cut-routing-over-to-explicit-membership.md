# 1073 - Cut Routing Over To Explicit Membership

Roadmap: [`../028-explicit-catalog-membership.md`](../028-explicit-catalog-membership.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/037-explicit-catalog-membership-contract.md`](../../../contracts/037-explicit-catalog-membership-contract.md)
Spec: [`../../../specs/archive/101-explicit-catalog-membership-strict-lane.md`](../../../specs/archive/101-explicit-catalog-membership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-08-10
Ready after: card 1072

## Purpose

Make explicit declarations the sole runtime catalog source and migrate every
catalog consumer to one routing-owned effective membership model.

## Owner And Seam

`effigy-routing` owns declaration collection, canonicalization,
deduplication, ordering, manifest loading, aliases, and origin evidence. Runner,
tasks, test, demo, execution, and status surfaces consume that result.

## Work

- add the normalized member and declaration-origin models from contract `037`
- collect root, named, and inline members from the complete composed manifest
- resolve member references and preserve all convergent declaration origins
- canonicalize, deduplicate, sort, load once, and retain alias-conflict behavior
- migrate routing, tasks, built-in test planning, execution preflight, demos,
  and task status to the shared effective set
- migrate the Effigy root manifest and catalog fixtures to explicit membership
- prove undeclared nested and ordinary-mounted manifests stay out of routing
- retain ancestor repo/workspace root resolution and `[manifest].root`
  boundaries

## Acceptance

- [x] root-only, descendant, symlink, sibling, named-mount, and inline-mount
      declarations normalize deterministically
- [x] physical and symlink declarations of one path load once with all origins
- [x] missing handles/paths/manifests and duplicate aliases carry contract
      evidence
- [x] selected system/workspace does not change effective membership
- [x] every runtime catalog consumer uses the routing-owned set
- [x] undeclared nested and ordinary-mounted manifests are ignored
- [x] selector precedence and unique-task routing remain unchanged
- [x] test planning fans out only across explicit membership

## Validation

- focused `effigy-routing` normalization and selector tests
- focused runner task/demo/status/preflight tests
- built-in test-plan fixtures across all declared member forms
- undeclared invalid nested-manifest sentinel fixture
- `cargo fmt --all -- --check`
- focused Clippy on touched crates
- affected-test selection from the graph

## Evidence

See
[`2026-08/10-101827-explicit-catalog-routing-cutover.md`](../../../logs/2026-08/10-101827-explicit-catalog-routing-cutover.md).

## Stop Conditions

Stop if a caller needs its own resolver, membership depends on runtime
selection, explicit declarations cannot preserve current selector rules, or an
ambient fallback is required.

## Next Task

Execute ready card
[`1074`](./1074-delete-discovery-and-align-diagnostics.md).
