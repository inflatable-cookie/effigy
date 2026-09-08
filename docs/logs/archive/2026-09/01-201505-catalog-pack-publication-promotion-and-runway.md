# Catalog-Pack Publication Promotion And Runway

Status: complete
Created: 2026-09-01
Roadmap: `g08.048`
Spec: `115`
Planning base: `ad26d03a97ee23b6d3060d6e0a4e8bb49bedb4e4`

## Summary

The operator approved all eight remaining publication recommendations. This
batch promoted them into architecture `026` and contract `043`, preserved the
primary-source map under research, removed the promoted triage authority, and
compiled cards `1103` through `1108`.

## Promoted Decisions

- canonical pack root `pack/` in the dedicated source repository
- exact generated Effigy snapshot plus typed provenance lock
- offline snapshot drift and online artifact/provenance proof as separate gates
- deterministic digest-first publication with process-immutable version tags
- channel/digest-reporting update with verified no-op and failure atomicity
- Effigy-owned machine-readable support floor
- default-branch commit/blob freshness and release-existence publication checks
- protected source tags, narrow serialized package writes, and digest retry oracle

The supporting GHCR, ORAS, GitHub Actions, attestation, ruleset, Releases API,
and GitHub App references now live in research source map `002`.

## Ready Frontier

Only card `1103` is Ready. It establishes the Effigy-owned support input that
every later publication check consumes.

Serial edges:

1. `1103 -> 1104`: pack validation must consume an already-landed Effigy owner.
2. `1104 -> 1105`: no live mutation before deterministic no-push proof.
3. `1105 -> 1106`: generated baseline must name a real verified public digest.
4. `1106 -> 1107/1108`: update and proposal automation both depend on the
   generated snapshot/lock contract.

Cards `1107` and `1108` are the first parallel-safe frontier because they own
different repositories and write scopes. Card `1105` also retains a separate
operator mutation gate.

## Dispatch Decision

Card `1103` is day-to-day implementation: a bounded typed policy file,
validator, failure matrix, and docs closeout. It does not meet either frontier
implementation axis. Material compatibility risk stays with exact-head
orchestrator review.

## Scope Guard

No pack repository, workflow, tag, package, visibility, `stable`, public update,
generated snapshot, Effigy release, S3, or extension-transport mutation belongs
to card `1103`.

## Validation

Planning validation passed: `effigy qa:docs` and `git diff --check`.

## Next Task

Dispatch card `1103` from the pushed planning head. Promote `1104` only after
accepted exact-head review and merge.
