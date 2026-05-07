# 261 Implement Service Catalog Integration Foundation

Status: landed
Updated: 2026-04-17
Roadmap: `g02.011`
Spec: `docs/specs/011-service-catalog-and-compose-assembly-strict-lane.md`

## Objective

Land the first runner/product integration slice for `effigy-catalog` so the
catalog is no longer only a crate-local proof.

## Scope

- add manifest-facing service declaration support needed for catalog-backed
  compose generation
- integrate generated compose output ownership into the container path without
  regressing direct `compose_file` ownership
- keep `src/` shell changes bounded and adapter-shaped

Primary write set:

- `crates/effigy-manifest/**`
- `crates/effigy-containers/**`
- bounded runner/container adapter points under `src/runner/**`

First proof boundary:

- one container config can declare services via catalog instead of
  `compose_file`
- Effigy can resolve that declaration into generated compose ownership through
  the integrated container path
- direct `compose_file` ownership still compiles and validates unchanged

## Acceptance

- one bounded catalog-backed container path compiles and validates
- focused integration tests prove generated compose ownership through the
  product surface
- the next batch can build on a real integrated boundary instead of a crate
  demo

## Outcome

This batch is landed.

What shipped:

- `effigy-manifest` now accepts catalog-backed container services under
  `[containers.<name>.services.<service>]` with flattened fragment params.
- `effigy-containers` now owns both compose-source modes:
  direct `compose_file` and generated catalog-backed compose under
  `.effigy/runtime/compose/.effigy-compose.generated.yml`, including override-file pickup.
- `effigy-doctor` schema validation now accepts the new service declaration
  shape.
- the root runner stayed adapter-light; only the container-error bridge needed
  a narrow update.

## Next Task

Compile the next bounded `g02.011` product-wiring batch on top of this landed
foundation. The obvious remaining work is product-facing behavior around
catalog inspection/eject and one real-project proof through the runner.
