# Contract-Drift Path-Check Layer

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: contract-drift-path-check-layer

## Summary

Added one generic missing piece to the starter contract bundle:
`effigy docs check-paths`.

This closes the gap between:

- starter content drift checks that already worked well
- starter path/spine presence checks that previously needed awkward substring
  hacks or repo-local scripts

The starter `qa:northstar` bundle can now validate:

- root front-door presence
- docs-spine presence
- agent-loop content
- front-door discoverability links
- vision index and next-action policy

without assuming Effigy's own repo layout beyond the documented starter
contract.

## Changes

- added built-in `effigy docs check-paths` for generic repo-relative
  file/directory presence checks
- wired parser/help/runtime/tests for the new docs validator
- extended the neutral starter-bundle fixture to validate:
  - root/docs spine presence
  - AGENTS contract content
  - README to docs front-door linkage
  - docs front-door links to vision/roadmaps/logs
- updated the consumer contract docs so `README.md` is now part of the starter
  contract rather than an implied extra
- updated the Northstar `northstar-effigy` native template so it emits:
  - `README.md.template`
  - `docs.README.md.template`
  - a `qa:northstar` bundle with path, agent, readme, and docs-front-door
    drift checks

## Decision

One new generic built-in was worth productizing.

`check-paths` is small, reusable, and clearly generic. It does not encode
Northstar-specific semantics; it only makes required path presence checkable by
the same native docs surface that already owns headings, contains, forbidden,
indexes, and next-action policy.

The rest of the contract still belongs where expected:

- Effigy owns generic validation engines
- Northstar owns starter scaffolding and template prose
- repo-specific heading inventories and policy names remain manifest/template
  work rather than hardcoded product defaults

## Validation

Validated with focused coverage:

- `cargo test --lib parse_docs_check_paths_with_repo_and_json`
- `cargo test --lib render_docs_help_shows_validation_options`
- `cargo test --test cli_output_tests cli_docs_check_paths_json_reports_missing_path`
- `cargo test --test cli_output_tests cli_starter_docs_policy_bundle_tasks_pass_on_neutral_fixture`
- `cargo test --test cli_output_tests cli_docs_help_is_command_specific -- --exact`
- `cargo run --bin effigy -- docs check-links CHANGELOG.md docs/guides/029-docs-qa-checklist-and-validation.md docs/guides/056-northstar-effigy-consumer-repo-contract.md docs/logs/README.md docs/logs/archive/2026-03/12-225500-starter-docs-policy-bundle-proof.md docs/logs/archive/2026-03/12-233000-contract-drift-path-check-layer.md docs/roadmaps/g01/029-northstar-effigy-consumer-adoption-kit.md`
- `cargo run --bin effigy -- docs check-index --dir docs/logs --index docs/logs/README.md`
- `cargo run --bin effigy -- docs check-links ../northstar/skills/northstar-effigy/SKILL.md ../northstar/skills/northstar-effigy/references/repo-contract.md ../northstar/skills/northstar-effigy/assets/templates/README.md`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: baseline `starter bundle validation could prove content policy, but
  not cleanly prove required root/docs path presence` -> current `starter
  bundle validation now covers path presence and front-door discoverability
  through one generic native validator plus template-aligned task composition`
- Remaining gap: prove the completed starter bundle on one more non-Effigy
  fixture or calm consumer repo, then decide whether the scaffolding side stays
  entirely in Northstar or whether a future `effigy init` surface should own a
  thin repo-contract bootstrap

## Next Task

Use the finished contract-drift layer to do one final boundary batch: prove the
starter bundle on a non-Effigy-shaped target, then decide whether bootstrap
scaffolding remains exclusively in the `northstar-effigy` skill or whether
Effigy should gain a narrow `init` / repo-contract surface.
