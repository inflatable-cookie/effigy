# Init Setup Wizard Closeout

Date: 2026-05-19  
Roadmap: [`g07.055`](../../roadmaps/g07/055-init-wizard-proof-docs-and-closeout.md)  
Batch card: [`1005`](../../roadmaps/g07/batch-cards/1005-close-init-setup-wizard-lane.md)  
Strict lane: [`093`](../../specs/093-init-setup-wizard-strict-lane.md)

## What Changed

- closed the init setup-wizard lane after landing:
  - bounded TTY wizard behavior for plain `effigy init`
  - shared setup inventory and checklist contract
  - explicit non-interactive selected-action execution
- added contextual proof for checklist and action execution across:
  - baseline managed setup
  - graph status
  - bundle inspection
  - secrets inspection
  - package.json task migration
- updated public docs and references so the live init story now matches the
  shipped surface:
  - root README
  - watch/init/migrate foundation guide
  - quick start
  - command reference
  - JSON payload examples
  - agent adoption guide
  - everyday workflows
- tightened init help coverage so the new flags and TTY behavior are asserted
  directly in tests

## Deferred Boundaries

- init still does not own hidden mutation for release, deploy, state, or
  distribution flows
- runtime bring-up remains guidance-first from init
- package-script cleanup still stops at migration/import, not destructive
  wrapper removal

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Moved:
  - `effigy init` is now one coherent setup front door for humans and agents
  - machine-readable planning and execution contracts exist for the wider setup
    surface
  - live guides now describe when init prompts, when it stays deterministic,
    and how agents can consume the same setup inventory
- Remaining open:
  - None in this lane

## Validation

- `cargo test run_manifest_task_builtin_init_ -- --nocapture`
- `cargo test render_init_help_shows_phase_scope -- --nocapture`
- `cargo test run_manifest_task_builtin_help_topics_render_expected_content -- --nocapture`
- `cargo test builtin_init_checklist_json_contract_has_versioned_shape -- --nocapture`
- `cargo test builtin_init_actions_json_contract_has_versioned_shape -- --nocapture`
- `./target/debug/effigy docs check json-examples --file docs/guides/026-json-payload-examples.md`
- `./target/debug/effigy docs check links README.md docs/guides/019-watch-init-migrate-foundation.md docs/guides/021-quick-start-and-command-cookbook.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/guides/047-agent-and-cross-repo-adoption.md docs/guides/055-everyday-workflows.md`
- `./target/debug/effigy docs check paths README.md docs/guides/019-watch-init-migrate-foundation.md docs/guides/021-quick-start-and-command-cookbook.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/guides/047-agent-and-cross-repo-adoption.md docs/guides/055-everyday-workflows.md`
- `cargo fmt --all -- --check`
- `cargo clippy -p effigy-builtin --tests -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `git diff --check`

## Next Task

No active ready card remains.
