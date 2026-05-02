# 02 014 Typed Container Assembly Boundary Decision

Date: 2026-05-02
Roadmap: `g03.014`
Spec: `docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md`
Batch: `337`

## Decision

Keep `g03.014` open for one more bounded slice.

## Why

`336` landed a real typed owner for:

- shared-service env injection
- generated port publication

But one high-signal generated-compose seam still remains in
`crates/effigy-containers/src/policy_support.rs`:

- generated media mount attachment
- generated host mount attachment
- repo-root-attached service discovery for those paths

Those helpers still parse compose YAML again and mutate `volumes` through
caller-local YAML logic. That is still central enough to the container
assembly brittleness story that handing off to `g03.015` now would be early.

## Next Boundary

Open `338` as the final bounded assembly slice for generated mount
attachment, then decide lane closeout in `339`.
