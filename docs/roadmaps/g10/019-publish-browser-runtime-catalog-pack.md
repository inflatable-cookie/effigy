# g10.019 — Publish browser runtime catalog pack

Owner: protected `effigy-catalog-pack` publication transaction
Created: 2026-09-25
Governing refs: `docs/contracts/043-feature-placement-and-surface-migration-contract.md`, catalog-pack `AGENTS.md`, `README.md`, and `docs/validation.md`
Depends on: reviewed and merged `g10.018` source PR

## Outcome

The reviewed canonical pack source is published under a new immutable version tag and OCI digest, with required provenance, public anonymous pull, and `stable` channel proof. The published artifact can be imported into Effigy as its generated recovery baseline.

## Ready-state rubric

- [x] Tom explicitly approved pack publication after source review on 2026-09-25.
- [x] `g10.018` PR #7 was independently reviewed at `f009a8c6718cbcf0c602dadea5938158cfeabeca`, merged as `76d594d8e72731547c0c2918746d5c98291d77b4`, and closed out on pack main at `c932c58f64cafd10a70d24907dc77fb81230bb01`.
- [x] Pack `1.1.0` declares `>=0.13, <0.14`, admitting released Effigy `v0.13.0`; live provider controls, release freshness, publication model, and no-push rehearsal passed on 2026-09-25. The canonical source tag is `v1.1.0`, still absent at readiness; its proposed peeled source commit is pack main `c932c58f64cafd10a70d24907dc77fb81230bb01`.

## Dispatch manifest

- **State:** complete through Queue task `a047926b-d189-4c9e-838e-9e21c3490b4d`, protected workflow run `36124764718`, and evidence PR #8.
- **Completion:** protected publication succeeds and an independently reviewed evidence PR records source tag/object/commit, OCI version digest, attestation, public pull, compatibility, and `stable` result.
- **Owned mutable paths:** pack-repository publication evidence and direct documentation corrections needed to report the result. The protected manual workflow performs provider mutations under its existing gates.
- **Reserved surfaces:** Effigy's generated baseline, Underlay bundle, Acowtancy, unrelated release tags, provider settings and workflow code.
- **Worker:** Queue worker with source and provider-release competence; independent review of the evidence PR.
- **Escalation:** Chatterbox and operator for any provider gate, changed support floor, collision, missing attestation, or failed publication. Never move or reuse a failed tag.

## Work

1. Verify merged source HEAD, new pack version, Effigy 0.13.0 release, current `support/catalog-pack-update.toml`, provider controls, and publication rehearsal.
2. Prepare the exact annotated source tag and invoke the protected manual publication workflow only for that reviewed source commit. Honor the environment approval and visibility checkpoint.
3. Verify immutable OCI digest, digest-bound attestation, anonymous pull, exact bytes, and `stable` rollback target/read-back. Stop on partial or uncertain publication.
4. Publish a narrow evidence PR for independent review and merge. Give `g10.017` the exact accepted artifact identity for generated snapshot import.

## Acceptance and review oracle

| Invariant | Counterexample | Proof |
| --- | --- | --- |
| Reviewed source | Tag points to a different commit | Exact merged commit, annotated tag object, peeled commit |
| Immutable artifact | A mutable channel stands in for version identity | OCI version digest and digest-bound attestation |
| Consumer access | Worker token can pull but the public cannot | Anonymous digest pull and exact-byte comparison |
| Safe channel | `stable` moves before verification | Protected ordered workflow, rollback target and read-back |

## Stop conditions

- Stop on a changed source HEAD, support policy, package collision, missing public access or attestation, uncertain workflow outcome, or failed verification. Preserve partial publication evidence; never re-tag or silently retry.

## Next task

Resume retained `g10.017` only after the pack artifact and Queue callback identity are reconciled. It imports the exact generated baseline, then the Underlay bundle exposes the input.

## Closeout evidence

Annotated `v1.1.0` tag object `72f5d7551dc0430fcc83af36066463bd9f1aab82` peels to source commit `c932c58f64cafd10a70d24907dc77fb81230bb01`. Protected run `36124764718` succeeded with environment approvals. OCI `ghcr.io/inflatable-cookie/effigy-catalog-pack:v1.1.0` and `stable` resolve to `sha256:5699fcb8641424cc6365feb2a4c4cc7f6056de385fc9dc49f771aec63f6078ba`. Digest-bound SLSA attestation and anonymous byte-for-byte pull were verified independently; unpacked content identity is `sha256:e92cc2f217fa2ba4de302b8376ec558afb042acd3a83e4d33ecfb03dc40606a3` across 42 files. Evidence PR #8 was reviewed at `7881d8f9b12826ed38329945022c85b846493bb4`, merged as `7fd4beaf105ce190bdea31c2a9c60ef1d8d6b4bb`, and closed out at pack main `829925597d29ac100ee0fceb40d8d2330012aada`.
