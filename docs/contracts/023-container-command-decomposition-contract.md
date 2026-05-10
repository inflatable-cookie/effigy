# 023 - Container Command Decomposition Contract

Status: Active
Owner: Platform
Updated: 2026-05-10

## Purpose

Lock the internal module-boundary rules for splitting `src/runner/container_command/`
without widening user-facing behavior.

## Scope

This contract owns:

- the target module split for `container_command`
- the no-user-facing-change rule
- the extraction order for cache, volume, lifecycle, and shared scope helpers
- the slim-dispatcher rule for `mod.rs`

This contract does not own:

- container CLI surface changes
- new cache or volume behavior
- backend selection changes
- runtime activation or compose behavior changes

## Target Module Boundary

The target structure for this lane is:

- `mod.rs`
- `lifecycle.rs`
- `data.rs`
- `cache.rs`
- `volume.rs`
- `support.rs`
- existing `gateway_registration/`

`mod.rs` keeps only:

- top-level imports
- the top-level `run_container` dispatcher
- any genuinely tiny shared glue that does not justify another owner

## Extraction Rules

- lifecycle owns: `up`, `down`, `status`, `stats`, `logs`, `shell`, `reset`, `eject`
- data owns: `data list`, `data export`, `data dump`, `data import`, `data seed`, `data pull-production`
- cache owns: `cache list`, `cache prune`
- volume owns: `volume list`, `volume prune`
- support owns shared helpers that remain cross-domain after extraction

## Behavior Rule

This lane is structural only.

Allowed changes:

- move code
- rename internal helpers
- reduce `mod.rs` size
- extract shared scope-resolution helpers

Not allowed:

- CLI grammar changes
- JSON schema changes
- output wording churn unless a moved helper forces a tiny equivalent rewrite

## Scope Resolution Rule

The repeated repo-root versus cwd fallback pattern may be extracted into one
shared helper, but the observable behavior must stay the same.

The first shared helper target is the cache/volume/status/down repo-scope
fallback path.

## Acceptance

- `mod.rs` trends toward a thin dispatcher
- extracted modules own the expected command families
- focused container tests stay green without behavior diffs
- the lane does not widen into new container features
