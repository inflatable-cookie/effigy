# 014 - Container Assembly Model And Single-Pass Compose Emission

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-02
Depends on: 004, 006, 013

## Problem

Effigy's container layer still assembles runtime state by generating compose
 YAML, reparsing it, and mutating it in multiple policy passes.

That creates order-sensitive behavior, hidden coupling, and a brittle
 maintenance surface for host integration, port policy, shared services, and
 workspace-specific rewrites.

## Goal

Introduce one typed container/runtime assembly model and emit compose YAML once
 at the end.

## Current Boundary

The first typed generated-compose owner is now landed for:

- shared-service env injection
- generated port publication

The remaining highest-signal generated-compose seam was:

- media mount attachment
- host mount attachment
- repo-root-attached service ownership for those two paths

That seam is now landed too. The remaining rewrite-heavy work lives in
workspace-specific runtime preparation, so the honest next handoff is
`g03.015`.

## Scope

- define a typed container assembly model for:
  - services
  - mounts
  - ports
  - aliases/routes
  - shared-service bindings
  - host-integration metadata
  - generated port policy
  - media and host mount policy
- move policy application in `effigy-containers` onto that model instead of
  repeated YAML parse/rewrite passes
- collapse or remove the current rewrite-heavy flow in:
  - `policy_support.rs`
  - workspace compose rewrite helpers where the same pattern still exists
- emit compose YAML once at the end from typed truth
- preserve current catalog and bundle behavior while changing internal
  assembly mechanics

## Non-Goals

- redesigning the public catalog file format
- changing bundle-owned app topology
- widening the runtime surface with new features

## Exit Condition

This milestone is complete when:

- compose generation is driven by one typed assembly model
- policy passes no longer depend on reparsing YAML strings as their main data
  model
- the main container policy tests can assert against typed assembly truth, not
  only emitted YAML snapshots

## Next Task

Promote `g03.015`.
