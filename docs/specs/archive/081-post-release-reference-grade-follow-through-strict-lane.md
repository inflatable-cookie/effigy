# 081 - Post-Release Reference-Grade Follow-Through Strict Lane

Roadmap: [`g05.008`](../roadmaps/g05/008-post-release-reference-grade-follow-through-suite.md)
Contracts:
- [`027-state-domain-extraction-contract.md`](../contracts/027-state-domain-extraction-contract.md)
- [`023-container-command-decomposition-contract.md`](../contracts/023-container-command-decomposition-contract.md)
- [`030-low-risk-deduplication-contract.md`](../contracts/030-low-risk-deduplication-contract.md)
- [`032-secret-and-local-config-management-contract.md`](../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Execute the reopened `g05` cleanup suite without reopening broad architecture
 planning. This lane finishes high-confidence ownership moves already named by
 current contracts and the latest post-release audit.

## Lane Posture

Posture: `strict-closed`

This lane is executable because the new roadmaps are written, the governing
contracts already exist, and the next slices can be bounded behind honest
owner-level cards.

## Hard Boundaries

- no release execution
- no `.github/workflows/` edits
- no speculative crate splits or merges
- no command grammar redesign
- no broad rewrite of runner orchestration
- no schema changes unless a later card explicitly scopes them

## Execution Order

1. `722` complete: lane opened and ready chain wired
2. `723` complete: stable state capture/context models and enum codec helpers moved into
   `effigy-state`
3. `724` complete: runner-owned state text rendering moved out of `state_command.rs`
4. `725` open the shared secrets vault access lane
5. `726` add shared runner vault access support and switch task/command callers
6. `727` complete: container callers now use the shared vault access boundary
7. `728` complete: lifecycle-owned container secrets and shell prep now have dedicated owners
8. `729` complete: lifecycle cleanup and closeout helpers now have a dedicated owner
9. `730` complete: Rhai secrets and process support now live behind dedicated internal modules
10. `731` complete: Rhai streaming/search/http support now lives behind a dedicated internal module
11. `732` complete: CLI help topic lookup and general-help inventory now share one descriptor surface
12. `733` complete: local CLI output release fixtures now use private shared builders
13. `734` complete: duplicate proof captured and residual deferrals made explicit
14. `735` complete: active docs/spec references and currentness surfaces refreshed
15. `736` complete: reopened `g05` cleanup suite closed explicitly

## Ready Chain

- `722` is complete
- `723` is complete
- `724` is complete
- `725` is complete
- `726` is complete
- `727` is complete
- `728` is complete
- `729` is complete
- `730` is complete
- `731` is complete
- `732` is complete
- `733` is complete
- `734` is complete
- `735` is complete
- `736` is complete
- `728` through `736` remain planned but in-bounds for auto-continuation once
  each predecessor closes cleanly

## Auto-Continuation Envelope

Auto-start is enabled for this lane while:

- the previous card closes green
- no contract gap appears during implementation
- no schema or behavior widening becomes necessary
- no user-owned worktree conflict blocks the selected owner seam

Stop and replan if a card needs fresh product judgment, a contract change, or a
schema decision that the current roadmap did not authorize.

## Acceptance

This lane is complete when:

- the queued `g05.009` through `g05.015` ownership cleanups are either complete
  or deliberately deferred with evidence
- active front-door docs no longer advertise stale generation or spec state
- the reopened `g05` suite has an explicit closeout record

## Next Task

No next task. Lane `081` is closed.

## Residual Validation Note

`cargo test -p effigy-cli` still fails on the unrelated header-width unit test
`header::tests::render_cli_header_width_grows_to_fit_long_version`, which sits
outside the `732` help-registry seam.

The latest duplicate scan still reports the same high findings after `733`.
That is expected for this slice because the remaining high duplicates are now
mainly bootstrap cross-file setup, release test cross-file ownership, and
literal-heavy help topic bodies rather than the local CLI output release
fixtures this card targeted.

Residual dedup deferrals after `734`:

- bootstrap cross-file fixture/setup duplication stays queued for a later
  bootstrap-owned lane instead of growing this local builder slice into a
  cross-crate harness
- release crate versus runner release test duplication stays deferred until a
  release-owned test-boundary pass reclassifies domain versus adaptation proofs
- literal-heavy help topic duplication remains partially deferred because this
  lane chose descriptor convergence, not help copy normalization or macro-based
  generation

## Residual Validation Note

`cargo test -p effigy-rhai` still fails on two pre-existing first-party script
policy tests outside the `730` seam:

- `first_party_rhai_process_calls_are_allowlisted`
- `first_party_rhai_scripts_do_not_use_legacy_module_dot_calls`

Those failures live in existing `.rhai` scripts under `external/`, starter
bundles, and repo scripts. The `730` and `731` code splits did not introduce
them, but the lane should keep them visible until explicit script cleanup
addresses them.
