# Catalog-Pack First-Publication Authority 1105

Status: complete
Created: 2026-09-02
Roadmap: `g08.048`
Spec: `115`
Card: `1105`

## Operator Decision

The operator explicitly authorized the first official catalog-pack publication
in response to the named mutation set. The initial authority covered:

- annotated source tag `v1.0.0`;
- GHCR package creation and public visibility for
  `ghcr.io/inflatable-cookie/effigy-catalog-pack`;
- digest-bound provenance attestation; and
- `stable` movement to the one verified manifest digest.

It did not authorize an Effigy binary release, another catalog-pack version, a
different registry/repository, broader workflow or collaborator authority, or
bypass of a failed gate.

## Failed Attempt And Recovery Decision

Implementation PR `#2` merged at
`f70637abe1024cf7b54cabe58c3bd5877dcf8eca`. The same worker added only the
pinned attest action to selected-actions, created annotated `v1.0.0` at that
merge (tag object `f2b59e65b1938600907de8dea566ad957e63be69`), and dispatched
protected run `33622687650`. The publish job failed on
its first GHCR descriptor read because real ORAS 1.3.3 reports absence as
`failed to find ...: not found`, while the reviewed network-free fixture used a
different phrase. The run made no package, attestation, or `stable` write.

The operator then selected the contract-valid PATCH recovery:

- keep `v1.0.0` immutable at its original object and commit;
- land the exact live-stderr classifier fixture and narrow repair;
- reconcile the selected-actions live oracle and bump the pack to `1.0.1`;
- review and merge that repair before creating annotated `v1.0.1`; and
- retry the protected first-publication transaction only from that new source.

Deleting or recreating `v1.0.0`, or executing post-fix scripts against the old
tag, is not authorized. Effigy binary release authority remains separate.

## Execution Boundary

Card `1105` is Ready. Execution is split at the irreversible boundary:

1. implementation is merged and the failed `v1.0.0` attempt is preserved;
2. the same worker lands the exact classifier, provider-oracle, version, docs,
   and incident-evidence repair in a reviewable PR;
3. the orchestrator accepts and merges that exact repair head;
4. the same worker creates protected annotated `v1.0.1` and dispatches the
   serialized version-publish job with `stable` unchanged;
5. the operator makes the linked organization package public through GitHub's
   documented package settings;
6. the same worker dispatches protected finalization, which verifies public
   linkage, attestation, anonymous exact-byte pull, authority freshness, and
   the safe rollback shape before moving `stable` once; and
7. the worker records immutable evidence in the repair or a follow-up PR.

No second live attempt may occur before step 3. Card `1105` completes only
after the evidence PR is accepted and merged.

## Review Planning Repair

Exact-head review of implementation PR `#2` exposed two plan-level provider
facts before any mutation:

1. ORAS `manifest delete <repo>:stable` deletes the resolved manifest, not only
   the tag. When the first-publication rollback target is absent, live deletion
   could destroy the candidate also held by `v1.0.0`. Canonical behavior now
   records the absent target, proves rollback-to-absence in the non-mutating
   model, and moves `stable` once. Live retag rollback is required only when a
   previous verified digest exists.
2. GitHub documents package visibility through operator package settings, not
   the proposed REST PATCH. First publication now uses serialized protected
   publish/finalize jobs. The version is published first with `stable`
   unchanged; the operator performs the already-authorized organization package
   visibility change; finalization then verifies public linkage before
   attestation, anonymous pull, authority refresh, and one `stable` movement.

The protected finalizer uses exact-SHA `actions/attest`. Before dispatch, the
same worker may add only that exact action to this repository's selected-
actions policy and must record the live provider state. No broader Actions or
collaborator authority is granted.

## Resolved Base Integration Finding

The pack repository initially reported that ongoing support proof required
Effigy `HEAD` to equal the one-time import commit
`055595340c2219d3d47296072f5818c524c341f0`. Current Effigy `main` is
`417e894515b66e53dc75ff28ac9a706243f04167`.

Implementation PR `#2` resolved this by consuming Effigy's current
default-branch support commit and blob while leaving the immutable import
commit/tree/blob only as historical byte-import proof. Doctor and publication
preflight were green before run `33622687650`.

## Ready Frontier And Routing

Only card `1105` is Ready. Cards `1106` through `1108` depend on accepted
`v1.0.1` publication evidence. This is a bounded, materially risky but
well-specified
day-to-day implementation lane: the contract, ordered mutation boundary, and
review oracle bound the remaining reasoning, so frontier implementation is not
justified. Material risk stays with frontier-strength exact-head review.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`
- Movement: failed pre-push `v1.0.0` preserved -> `v1.0.1` PATCH recovery
  authorized and card `1105` Ready
- Remaining gap: repair merge, verified public artifact/channel, generated
  Effigy baseline, public update cutover, and proposal automation

## Next Task

Resume the existing pack worker lane for the bounded `v1.0.1` repair PR. Merge
that reviewed repair before the new protected publication attempt.
