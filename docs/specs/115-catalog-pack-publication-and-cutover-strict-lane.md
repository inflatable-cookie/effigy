# 115 Catalog-Pack Publication And Cutover Strict Lane

Status: Active
Owner: Effigy orchestrator
Created: 2026-09-01
Roadmap: [`g08.048`](../roadmaps/g08/048-catalog-pack-publication-and-cutover.md)
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../contracts/043-feature-placement-and-surface-migration-contract.md)
Research: [`source map 002`](../research/source-hubs/002-catalog-pack-publication-source-map-v1.md)

## Outcome

Move concrete catalog assets to a dedicated independently versioned repository,
publish one verified official OCI pack, derive Effigy's permanent generated
baseline from it, and expose an explicit safe update path without weakening
offline operation or Effigy release authority.

## Fixed Decisions

- Source repository: `inflatable-cookie/effigy-catalog-pack`.
- Canonical asset root: `pack/`; first pack version/tag: `1.0.0` / `v1.0.0`.
- OCI repository: `ghcr.io/inflatable-cookie/effigy-catalog-pack`; channel:
  `stable`; OCI manifest digest is the immutable identity.
- Effigy keeps an exact generated baseline plus typed provenance lock.
- Ordinary commands never probe GHCR or activate pack content implicitly.
- Effigy owns the support floor. The pack repository consumes it by resolved
  commit and blob digest.
- First publication and every Effigy binary release remain separate explicit
  operator mutations.

## Dependency Runway

```text
1103 Effigy support-floor authority
  -> 1104 pack repository foundation + no-push rehearsal
    -> 1105 first publication (operator-gated external mutation)
      -> 1106 generated Effigy baseline + provenance proof
        -> 1107 official update cutover
        -> 1108 narrow baseline-PR proposal automation
```

Cards `1107` and `1108` may run in parallel only after `1106`: they have
different repository owners and write scopes. Same-repository PR review and
merge order remains serial. No other edge is ready until `1103` is on pushed
`main` and `1104` is promoted.

## Cards

- [`1103`](../roadmaps/g08/batch-cards/1103-establish-catalog-pack-support-floor.md)
  — Complete; Effigy-owned compatibility authority.
- [`1104`](../roadmaps/g08/batch-cards/1104-build-catalog-pack-repository-foundation.md)
  — Blocked on `1103` merge.
- [`1105`](../roadmaps/g08/batch-cards/1105-publish-first-official-catalog-pack.md)
  — Blocked on `1104` and explicit operator mutation authority.
- [`1106`](../roadmaps/g08/batch-cards/1106-cut-over-generated-catalog-baseline.md)
  — Blocked on accepted `1105` evidence.
- [`1107`](../roadmaps/g08/batch-cards/1107-expose-official-catalog-pack-update.md)
  — Blocked on `1106`.
- [`1108`](../roadmaps/g08/batch-cards/1108-propose-generated-baseline-updates.md)
  — Blocked on `1106`; parallel-safe with `1107` once ready.

## Lane Rules

- A card may move to Ready only when every predecessor and external gate is
  evidenced, its repository authority is explicit, and its review oracle is
  falsifiable without relying on a future card.
- Worker PRs do not publish, tag, change package visibility, move `stable`, or
  release Effigy unless the current card and operator instruction name that
  mutation exactly.
- Workflow edits are limited to the pack repository implementation lane.
  Effigy's `.github/workflows/` remains out of scope without separate authority.
- Preserve the compiled baseline and current selection transaction until card
  `1106` proves an exact generated replacement.
- Public `service pack update` remains absent until card `1107` has a public,
  anonymously readable, attested digest from card `1105`.
- No S3, retention, extension transport, command grouping, or release cleanup.

## Whole-Lane Review Oracle

Reject the lane if any of these counterexamples survives:

1. Pack source and Effigy snapshot can both be edited as authorities.
2. Offline drift passes when snapshot bytes, manifest facts, or content identity
   disagree with the provenance lock.
3. Publication can mutate a version pointer or `stable` before deterministic
   digest, compatibility, provenance, anonymous-pull, and exact-byte proof.
4. A stale or incompatible Effigy support input permits package mutation.
5. A partial retry overwrites a different digest or invents a new identity for
   unchanged source.
6. Ordinary install/bootstrap/catalog use contacts GHCR or requires user-state
   pack activation.
7. Failed update changes active, previous, or channel metadata.
8. Pack automation can approve, merge, release, or write unrelated Effigy code.
9. First publication or an Effigy release occurs without its explicit operator
   gate.

## Validation And Evidence

Each card maps its oracle to exact proof in one dated evidence log. Use focused
repository tests, deterministic no-network proof where specified, exact-byte
and identity evidence, repository-owned docs QA, and full Effigy QA for Effigy
runtime changes. Publication evidence must record immutable source/artifact
identities, workflow run, support-input commit/blob, attestation, anonymous
pull, channel result, and rollback target.

## Stop Conditions

Stop and return to planning if the generic OCI artifact cannot be attested,
anonymous pull differs from authenticated proof, exact snapshot reproduction is
not deterministic, the compatibility authority cannot be consumed without a
second owner, the GitHub App cannot be narrowly scoped, or a requested mutation
exceeds the current operator gate.

## Next Task

Update card `1104` to Ready after the card `1103` support floor is on pushed
`main`. Do not create the pack repository or publication state from this spec
alone.
