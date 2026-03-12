# Starter Docs-Policy Bundle Proof

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: starter-docs-policy-bundle-proof

## Summary

Packaged the starter native consumer `[docs_policy]` bundle as a concrete,
reusable shape and proved it on a neutral fixture.

This closes the gap between:

- `the contract guide describes the starter bundle`
- `the Northstar skill template emits the starter bundle`
- `the bundled tasks actually run cleanly outside Effigy's own docs tree`

## Changes

- updated the consumer contract guide to include:
  - a starter `[docs_policy.indexes.vision]` block
  - a starter `[docs_policy.next_actions.vision]` block
  - a starter `docs/policy/vision-next-task-verbs.txt` allowlist
  - the corrected `effigy docs check-headings --require-heading ...` flag form
- upgraded the Northstar portable native template so it now emits:
  - starter vision docs-policy config
  - starter `qa:docs`
  - starter `qa:northstar`
  - the same task-composed validation vocabulary documented in Effigy
- added a neutral CLI fixture test that creates:
  - a minimal `effigy.toml`
  - a minimal docs spine
  - a local allowlist file
  - starter `qa:docs`, `qa:northstar`, and `qa`
  and proves those tasks all pass without any Effigy-repo-specific files

## Decision

The starter docs-policy bundle is now concrete enough to treat as a reusable
contract:

- Effigy owns the generic engines
- Northstar owns the portable template emission
- neither side needs another repo-specific shell layer for the starter path

That means the next question is no longer "what should the starter bundle be?"
It is "what remaining drift checks deserve product surface instead of staying in
templates and skill guidance?"

## Validation

Validated with focused coverage:

- `cargo test --test cli_output_tests cli_starter_docs_policy_bundle_tasks_pass_on_neutral_fixture`
- `cargo run --bin effigy -- docs check-links CHANGELOG.md docs/guides/056-northstar-effigy-consumer-repo-contract.md docs/logs/README.md docs/logs/2026-03/12-223500-product-boundary-and-verify-install-ssh-closeout.md docs/logs/2026-03/12-225500-starter-docs-policy-bundle-proof.md docs/roadmaps/g01/029-northstar-effigy-consumer-adoption-kit.md`
- `cargo run --bin effigy -- docs check-index --dir docs/logs --index docs/logs/README.md`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: baseline `starter docs-policy guidance existed in prose, but the
  reusable config and task bundle were not yet packaged and proven together` ->
  current `the starter native docs-policy bundle is documented, emitted by the
  Northstar template, and proven by a neutral fixture task run`
- Remaining gap: add non-Effigy-repo-specific agent-contract and docs-skeleton
  drift rules, then decide whether those stay in the template/skill layer or
  become a future `effigy init` or repo-contract surface

## Next Task

Use the proven starter bundle to add one more layer of generic contract-drift
checks: agent instruction drift, docs front-door drift, and minimum docs-spine
presence without assuming Effigy's own file inventory.
