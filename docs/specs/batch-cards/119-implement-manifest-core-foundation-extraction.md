# 119 Implement Manifest Core Foundation Extraction

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move shared manifest loading, composition, and task-manifest contracts out of
`runner` so deeper domain extraction can stop depending on `runner` as the
default owner of repo policy.

## In Scope

- create the next shared manifest-facing foundation on top of `effigy-core`
- move the first trustworthy manifest contracts out of `runner`:
  - task manifest root types where they move cleanly
  - manifest composition and inspection contracts
  - shared config-section contracts where other domains depend on them
- reconnect current runtime paths without changing user-facing behavior
- leave the next extraction batch explicit

## Out Of Scope

- full domain extraction of release, distribution, containers, or demos
- release execution
- consumer rollout work

## Acceptance Criteria

- shared manifest ownership no longer sits entirely inside `runner`
- the extraction unblocks deeper task and domain movement honestly
- the next extraction batch is explicit

## Validation

- targeted Rust validation for the moved manifest contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open the next extraction batch using the new manifest/core boundary, likely
deeper task extraction or the release-blocking container/distribution/release
cluster.
