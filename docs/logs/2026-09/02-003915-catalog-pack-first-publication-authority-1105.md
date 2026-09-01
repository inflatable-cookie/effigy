# Catalog-Pack First-Publication Authority 1105

Status: complete
Created: 2026-09-02
Roadmap: `g08.048`
Spec: `115`
Card: `1105`

## Operator Decision

The operator explicitly authorized the first official catalog-pack publication
in response to the named mutation set. The authority covers:

- annotated source tag `v1.0.0`;
- GHCR package creation and public visibility for
  `ghcr.io/inflatable-cookie/effigy-catalog-pack`;
- digest-bound provenance attestation; and
- `stable` movement to the one verified manifest digest.

It does not authorize an Effigy binary release, another catalog-pack version,
a different registry/repository, broader workflow or collaborator authority,
or bypass of a failed gate.

## Execution Boundary

Card `1105` is Ready. Execution is split at the irreversible boundary:

1. a worker lands the protected publication implementation and deterministic
   pre-mutation proof in a reviewable PR;
2. the orchestrator accepts and merges that exact head;
3. the same worker creates the protected annotated tag, dispatches publication,
   verifies attestation, anonymous exact-byte pull, rollback, and `stable`, then
   records immutable evidence in a follow-up PR.

No live mutation may occur before step 2. Card `1105` completes only after the
evidence PR is accepted and merged.

## Base Integration Finding

The pack repository's current `effigy doctor` reports that ongoing support
proof still requires Effigy `HEAD` to equal the one-time import commit
`055595340c2219d3d47296072f5818c524c341f0`. Current Effigy `main` is
`417e894515b66e53dc75ff28ac9a706243f04167`.

This is in scope for `1105`, not a reason to repin current authority to old
state. Contract `043` requires publication to resolve the support file from
Effigy's current default-branch commit and record that commit plus file blob.
The immutable import commit/tree/blob remain only the historical byte-import
proof. The implementation PR must split those two authorities and restore a
green health path before any publication mutation.

## Ready Frontier And Routing

Only card `1105` is Ready. Cards `1106` through `1108` depend on accepted
publication evidence. This is a bounded, materially risky but well-specified
day-to-day implementation lane: the contract, ordered mutation boundary, and
review oracle bound the remaining reasoning, so frontier implementation is not
justified. Material risk stays with frontier-strength exact-head review.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`
- Movement: operator-gated publication blocked -> first-publication authority
  explicit and card `1105` Ready
- Remaining gap: implementation merge, verified public artifact/channel,
  generated Effigy baseline, public update cutover, and proposal automation

## Next Task

Commit and push one worker handoff in the pack repository, then launch the
implementation-only first phase. Preserve the same worker identity across the
post-merge publication and evidence phase.
