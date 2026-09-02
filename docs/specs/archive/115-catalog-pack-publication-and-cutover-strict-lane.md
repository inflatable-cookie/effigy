# 115 Catalog-Pack Publication And Cutover Strict Lane

Status: Complete
Owner: Effigy orchestrator
Created: 2026-09-01
Roadmap: [`g08.048`](../../roadmaps/g08/048-catalog-pack-publication-and-cutover.md)
Architecture: [`026`](../../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md)
Research: [`source map 002`](../../research/source-hubs/002-catalog-pack-publication-source-map-v1.md)

## Outcome

Move concrete catalog assets to a dedicated independently versioned repository,
publish one verified official OCI pack, derive Effigy's permanent generated
baseline from it, and expose an explicit safe update path without weakening
offline operation or Effigy release authority.

## Fixed Decisions

- Source repository: public `inflatable-cookie/effigy-catalog-pack`.
- Canonical asset root: `pack/`. Failed pre-push source tag `v1.0.0` is retained
  immutably; the first public pack version/tag is `1.0.1` / `v1.0.1`.
- OCI repository: `ghcr.io/inflatable-cookie/effigy-catalog-pack`; channel:
  `stable`; OCI manifest digest is the immutable identity.
- Effigy keeps an exact generated baseline plus typed provenance lock.
- Ordinary commands never probe GHCR or activate pack content implicitly.
- Effigy owns the support floor. The pack repository consumes it by resolved
  commit and blob digest.
- First publication and every Effigy binary release remain separate explicit
  operator mutations.
- A failed source tag is never moved or reused. A pre-package failure resumes
  from a newly reviewed PATCH source/version tag.

## Dependency Runway

```text
1103 Effigy support-floor authority
  -> 1104 pack repository foundation + no-push rehearsal
    -> 1105 first publication (operator-gated external mutation)
      -> 1106 generated Effigy baseline + provenance proof
        -> 1107 official update cutover
        -> 1108 narrow baseline-PR proposal automation
```

Cards `1107` and `1108` completed on 2026-09-02. The update command merged in
Effigy at `20d9040c`; proposal automation merged in the catalog-pack repository
at `4dd8b8a5`, with the empty-delta provider checkpoint at `ebb813e1`.

## Cards

- [`1103`](../../roadmaps/g08/batch-cards/1103-establish-catalog-pack-support-floor.md)
  — Complete; Effigy-owned compatibility authority.
- [`1104`](../../roadmaps/g08/batch-cards/1104-build-catalog-pack-repository-foundation.md)
  — Complete; dedicated public repository foundation and no-push rehearsal.
- [`1105`](../../roadmaps/g08/batch-cards/1105-publish-first-official-catalog-pack.md)
  — Complete; `v1.0.0` is preserved and public `v1.0.1` plus `stable` resolve
  to the accepted attested digest.
- [`1106`](../../roadmaps/g08/batch-cards/1106-cut-over-generated-catalog-baseline.md)
  — Complete; Effigy's generated recovery snapshot and provenance lock are cut
  over (evidence
  [`02-144609`](../../logs/2026-09/02-144609-catalog-pack-generated-baseline-1106.md)).
- [`1107`](../../roadmaps/g08/batch-cards/1107-expose-official-catalog-pack-update.md)
  — Complete; Effigy owns public update resolution and transaction integration.
- [`1108`](../../roadmaps/g08/batch-cards/1108-propose-generated-baseline-updates.md)
  — Complete; pack repository owns generated-only proposal automation and its
  narrowly scoped App installation.

## Lane Rules

- A card may move to Ready only when every predecessor and external gate is
  evidenced, its repository authority is explicit, and its review oracle is
  falsifiable without relying on a future card.
- Worker PRs do not publish, tag, change package visibility, move `stable`, or
  release Effigy unless the current card and operator instruction name that
  mutation exactly.
- Workflow edits are limited to the pack repository implementation lane.
  Effigy's `.github/workflows/` remains out of scope without separate authority.
- First publication uses serialized protected publish/finalize jobs. GitHub's
  documented operator package-settings control supplies the one-time public
  visibility checkpoint between them; no undocumented package PATCH is part of
  the release transaction.
- The finalize job uses exact-SHA `actions/attest`. Its repository selected-
  actions policy may add only that exact action before dispatch.
- Failed publication tag `v1.0.0` is immutable incident evidence. The narrow
  live-ORAS classifier repair and provider-control reconciliation must merge
  before a new annotated `v1.0.1` source tag and protected retry.
- An absent pre-publication `stable` target is rollback evidence, not deletion
  authority. Prove that branch in the non-mutating model and move `stable` once
  after final gates. A live retag rollback exercise applies only when a previous
  verified digest exists.
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

Run the operator intent checkpoint from vision `020`. The first non-empty
proposal is future operational evidence; Effigy release authority remains
separate.
