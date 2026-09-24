# Cross-repository docs-source consent and provenance

Raised: 2026-09-24. Release-readiness audit requested by the operator.
Status: open; not execution authority.
Owner: Effigy Chatterbox for planning; docs-context source routing for repair.

## Evidence

- Contract `041` and guide `079` require a neighbor's sharing decision from
  committed root `effigy.toml` bytes. `load_committed_docs_policy_sources` in
  `crates/effigy-manifest/src/config_sections/docs_policy.rs` currently reads
  the working-tree file. An uncommitted change can therefore opt a neighbor in
  or out. Existing loader tests use temporary directories without Git and do
  not prove the committed-blob boundary.
- `dirty_paths` in `crates/effigy-codegraph/src/git.rs` parses human-formatted
  porcelain and strips quotes without Git C-quote decoding. A changed path
  containing characters Git quotes can miss the dirty set, after which
  `docs_context/sources.rs` can label a working-tree excerpt `committed`.

## Known direction and open checks

- Preserve the already-settled committed opt-in and conservative provenance
  contracts. This note proposes no new sharing policy.
- Prove dirty root-manifest opt-in and opt-out against an actual Git fixture;
  includes and local overlays must remain unable to grant membership.
- Use unambiguous Git path output for dirty-path identity. Prove quoted and
  renamed Markdown paths never receive optimistic `committed` labels.
- Inspect failure behavior for unreadable or unborn Git state; it must remain
  conservative and must not write to an unshared neighbor.

## Promotion condition

Reconcile the exact code path and fixtures against contract `041`, then obtain
operator confirmation for a bounded implementation task. Keep this separate
from doctor, skill stdio, and release mutation.

## Next check

Review the evidence with the operator and decide whether this is part of the
pre-release repair frontier.
