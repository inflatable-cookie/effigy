# 028 Container Assembly Model And Single-Pass Compose Emission Strict Lane

Status: complete
Updated: 2026-05-02
Roadmap: `g03.014`

## Context

Effigy's runtime/session ownership seams are now on a typed path strongly
 enough to stop treating bootstrap-only env flags as the governing control
 mechanism.

That closes the first `v1.0` hardening seam. The next honest weakness is the
 container assembly core itself: compose state is still built by generating
 YAML, reparsing it, and mutating it in policy passes.

That rewrite-heavy flow is now the clearest source of container brittleness.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/contracts/005-container-runtime-contract.md`
- `docs/roadmaps/g03/014-container-assembly-model-and-single-pass-compose-emission.md`
- `docs/roadmaps/g03/README.md`

## Lane Focus

This lane owns:

- one typed container/runtime assembly model inside `effigy-containers`
- migration of compose policy application off repeated YAML parse/rewrite
  passes
- one final YAML emission path from typed assembly truth
- enough proof to show the first assembly owner is real, not just another
  wrapper around string mutation

This lane does not own:

- broad workspace/runtime orchestrator splitting
- catalog surface redesign
- new container/runtime features
- deployment/provider work

## Current Posture

`strict-complete`

The correct implementation order is:

1. define one typed assembly model for container services, mounts, ports,
   aliases, and policy-owned metadata
2. move the first high-value compose policy paths onto that model
3. emit compose YAML once from typed truth instead of reparsing mutated YAML
4. prove the migrated path on the main generated-compose/runtime policy seams
5. move the remaining generated media/host mount attachment seam onto the
   typed model
6. decide whether another bounded assembly slice is needed before handing off
   to the workspace/runtime orchestrator split

## Integration Constraint

- keep the first batch centered on the assembly model itself
- do not pretend a typed wrapper around existing YAML rewrites is enough
- migrate one honest high-value policy seam fully instead of widening into
  every compose rewrite path at once
- preserve current catalog and bundle behavior while changing internal
  mechanics

## Continuation Chain

1. `336` — implement the typed container assembly foundation
2. `337` — decide whether another bounded assembly slice is needed
3. `338` — move media and host mount attachment onto the typed assembly model
4. `339` — decide whether the lane can hand off cleanly to `g03.015`

## Exit Condition

This strict lane is complete when:

- compose generation is driven by one typed assembly model
- the main generated-compose policy paths no longer depend on repeated YAML
  reparsing as their primary data model
- the first migrated assembly seams are proven from typed truth, not only
  emitted YAML snapshots

## Next Task

Promote `g03.015`.
