# 1074 - Delete Discovery And Align Diagnostics

Roadmap: [`../028-explicit-catalog-membership.md`](../028-explicit-catalog-membership.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/037-explicit-catalog-membership-contract.md`](../../../contracts/037-explicit-catalog-membership-contract.md)
Spec: [`../../../specs/archive/101-explicit-catalog-membership-strict-lane.md`](../../../specs/archive/101-explicit-catalog-membership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-08-10
Ready after: card 1073 and the no-ambient-consumer checkpoint

## Purpose

Delete the obsolete discovery implementation and make every diagnostic and
public command describe declared membership accurately.

## Owner And Seam

The catalog public-surface seam owns this card. `effigy-routing` deletes old
mechanisms, doctor consumes shared membership evidence, and `effigy-cli`
removes the now-empty catalog-cache command surface.

## Work

- delete descendant traversal, skip policy, symlink walk, cache stamps, empty
  subtree pruning, cache file helpers, and their tests
- remove `catalog.discovery` from manifest and doctor schemas
- remove the discovery-cache command; remove the `catalog` built-in when no
  contract-backed subcommand remains
- migrate catalog-specific doctor findings and human text from discovered to
  declared/effective terminology
- preserve JSON structures named generically for catalogs
- rename only evidence or identifiers explicitly tied to ambient discovery
- update help, completion, released-surface, JSON examples, and error fixtures
- prove old config and command forms fail as the documented breaking surface

## Acceptance

- [x] no runtime descendant catalog walk or discovery cache code remains
- [x] `catalog.discovery` is rejected as unsupported configuration
- [x] the obsolete cache CLI and empty built-in inventory entry are absent
- [x] doctor and tasks consume shared membership evidence
- [x] generic catalog JSON structure stays stable when shape is unchanged
- [x] discovery-specific identifiers are removed without compatibility aliases
- [x] old command/config failures point to explicit migration guidance

## Validation

- exact-token proof for deleted discovery/cache symbols and config keys
- focused routing, doctor, CLI parse/help/completion, and released-surface tests
- focused text/JSON error and evidence fixtures
- `effigy qa:json` when selected payload evidence changes
- `cargo fmt --all -- --check`
- focused Clippy on touched crates
- affected-test selection from the graph

## Evidence

Recorded in
[`2026-08/10-104558-delete-discovery-and-align-diagnostics.md`](../../../logs/archive/2026-08/10-104558-delete-discovery-and-align-diagnostics.md).

## Stop Conditions

Stop if deletion exposes an undeclared runtime consumer, requires a compatibility
resolver, changes generic JSON structure without a schema update, or reaches a
workflow/release mutation.

## Next Task

Execute ready card
[`1075`](./1075-prove-migration-and-close-explicit-membership-lane.md) for
consumer proof, public guidance, and lane closeout.
