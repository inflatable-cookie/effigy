# Help-First Command Discovery Closeout

Status: complete
Created: 2026-08-31
Roadmap: g08.038
Card: 1093
Spec: 111 (archived)
Contract: 043
Architecture: 026
Predecessor evidence: [`31-213000-northstar-profile-proof-1090.md`](./31-213000-northstar-profile-proof-1090.md)

## Summary

- `effigy --help` and `effigy help` now render the built-in inventory in six
  operator-job groups: `work`, `local`, `repo`, `deliver`, `extend`, `admin`.
- `effigy help <group>` renders one group. `effigy help <command>` reaches the
  command's existing detailed-help owner — the typed panel where one exists, the
  built-in's own help for `config` and `scan` — byte-identical to
  `effigy <command> --help`, and defers with it when repository routing owns the
  name.
- An unknown help topic fails with exit code `2` and names every valid group and
  command. The old silent fallback to general help is gone.
- Execution grammar is unchanged. No `effigy <group> <command>` route exists, no
  new top-level built-in name is reserved, and `work`, `local`, `repo`,
  `deliver`, `extend`, and `admin` remain ordinary manifest task selectors.
- One typed inventory (`GENERAL_HELP_ENTRIES`) now owns every general-help row
  and its single primary group, so general help, group help, and the ownership
  assertions read the same table.

## Implementation Shape

`crates/effigy-cli/src/command_surface.rs` gained `HelpGroup`, `GeneralHelpEntry`,
the ordered `GENERAL_HELP_ENTRIES` inventory, and `HELP_COMMAND_TOPICS`. The
three general-help fields (`general_help_command`, `general_help_description`,
`deferred_builtin`) moved off `CommandDescriptor` into that inventory;
`CommandDescriptor` keeps `topic` and `command_name`, which is still the
task-style routing lookup and was not touched.

`Command::HelpGroup(HelpGroup)` is a new discovery-only command variant.
`parse_help_command` in `crates/effigy-cli/src/command_parsing.rs` resolves the
single topic argument as group, then typed command, then built-in-owned help,
then diagnostic. Rendering lives in
`crates/effigy-cli/src/help/topics/general.rs`, where general help and group
help share one `render_group_section` so the two views cannot drift.

## Review Oracle

| # | Counterexample | Proof |
| --- | --- | --- |
| 1 | `effigy help repo` lists exactly `graph`, `scan`, `docs`, `contracts`, `papercuts` and no local/delivery command | `command_surface::tests::group_inventories_match_the_contract_taxonomy` (ordered, all six groups); `tests::help_render_tests::render_repo_group_help_lists_only_repository_intelligence_commands`; `help_and_flags_tests::cli_help_repo_group_lists_only_repository_intelligence_commands` |
| 2 | `effigy help docs` and `effigy docs --help` carry the same facts | `help_and_flags_tests::cli_help_command_and_direct_command_help_render_the_same_facts` compares full stdout for `docs`, `graph`, `release`, `tasks`, `state`, `config`, `scan`; `help_and_flags_tests::cli_help_for_builtin_owned_help_matches_the_direct_command_exactly` and `..._emits_the_same_json_envelope` add stdout, JSON, and exit parity for the built-in-owned pair; `help_and_flag_tests::parse_help_command_matches_direct_command_help` proves both spellings resolve to the same `HelpTopic` for all 31 typed names |
| 3 | With `[tasks.repo]`, `effigy repo` runs the task while `effigy help repo` renders discovery | `help_and_flags_tests::cli_manifest_selector_named_after_a_help_group_keeps_task_routing` |
| 4 | A manifest selector shadowing a deferred built-in keeps it out of general and primary-group help | `cli::help_dispatch::tests::manifest_selector_shadowing_a_builtin_hides_it_from_general_and_group_help` (`[tasks.docs]`); `cli::help_dispatch::tests::build_help_group_payload_hides_explicitly_deferred_builtins` (`[defer] builtins = ["graph"]`); `cli::entrypoint::help_deferral_tests` and `help_and_flags_tests::cli_help_command_topic_defers_with_the_direct_command_when_a_selector_shadows_it` for the detail surface |
| 5 | Inventory has no missing or duplicate primary-group owner | `command_surface::tests::general_help_entries_have_exactly_one_primary_group_owner`; `command_surface::tests::every_help_command_has_one_primary_group_row`; `command_surface::tests::general_help_entries_are_backed_by_inventory_metadata` |
| 6 | `effigy repo docs` is not a grouped built-in route | `help_and_flags_tests::cli_manifest_selector_named_after_a_help_group_keeps_task_routing` asserts the output is neither group help nor `docs Help`; `help_and_flag_tests::parse_help_group_words_stay_available_to_task_selectors` asserts all six slugs parse as `Command::Task` |
| 7 | `effigy help not-a-topic` fails deterministically with valid paths | `help_and_flags_tests::cli_unknown_help_topic_fails_with_valid_group_and_command_guidance` (exit `2`, groups and commands named on stderr); `help_and_flag_tests::parse_unknown_help_topic_fails_with_group_and_command_guidance` |

### Note on oracle 7

The failure exits `2` through the repository's standard parse-error path, which
prints the error block on stderr and then the general help panel on stdout for
every parse error. That trailing panel is the pre-existing affordance for all
invalid arguments, not a fallback for the help topic: the command fails, the
message names the unknown topic, and the exit code is non-zero.

### Detail help defers with the direct command

`effigy <command> --help` already routes to the repository when a manifest task
or `[defer] builtins` entry owns the built-in name — with `[tasks.docs]`,
`effigy docs --help` runs the task and prints `docs-task`. Left alone, the new
`effigy help docs` route would have resurfaced the built-in panel in exactly
that repository, breaking both the deferral rule and the `help <command>` /
`<command> --help` parity rule.

`reject_help_for_deferred_builtin` in `src/cli/entrypoint.rs` closes that: after
`effigy help <command>` parses, the resolved topic's owning inventory row is
checked against the repository's deferred built-ins, and a shadowed name fails
with exit code `2` and the message `` `docs` is deferred to this repository's
own routing, so its built-in help panel is unavailable here; run
`effigy docs --help` for what `effigy docs` actually does ``. General help,
group help, and every unshadowed topic stay reachable.

The rule is deliberately row-inherited: `effigy help <command>` defers exactly
when that command's general-help row hides, and only then. It does not invent a
wider deferral set. Rows that already stayed visible under a shadowing selector
before this card — `effigy exec` is the standing example, and `changelog` has no
general-help row at all — keep behaving as they did.
`deferred_builtin_for_help_topic_matches_the_inventory_row` pins that
correspondence for every row.

## Built-In-Owned Help: `config` And `scan`

Added in review repair for the `execution-miss` recorded at head
`d34f264012014f65a9f48e84aa396b82841e5716`.

`config` and `scan` are the two general-help rows whose detailed help is owned
by the built-in itself rather than a typed `HelpTopic` panel:
`effigy config --help` and `effigy scan --help` are not `Command::Help` at all —
they parse to `Command::Task { name, args: ["--help"] }` and reach the renderers
in `crates/effigy-builtin/src/config.rs` and `scan.rs`.

The first cut of this card treated that as an exception and made
`effigy help config` / `effigy help scan` fail with a pointer. That silently
narrowed the contract-`043` parity rule and the guide's "ask for help on any
command" promise, so the review rejected it. It is repaired by resolving those
two names to the *same command value* the direct form produces:

```rust
if let Some(name) = command_surface::help_builtin_route(&topic) {
    return Ok(Command::Task(TaskInvocation {
        name: name.to_owned(),
        args: vec!["--help".to_owned()],
    }));
}
```

Because both spellings become one identical command value, parity is structural
rather than asserted: there is no second renderer, no copied text, and no way
for the two to drift. Measured on the repaired binary, `effigy help config` and
`effigy config --help` produce byte-identical stdout at 14983 bytes, and
`effigy help scan` / `effigy scan --help` at 3374 bytes, both exiting `0`.

`HELP_COMMAND_BUILTIN_ROUTES` names the pair, and `help_command_names()` joins
it with `HELP_COMMAND_TOPICS` so the unknown-topic diagnostic advertises all 33
accepted command names.

### Help still never executes repository work

`config` and `scan` are absent from `is_top_level_builtin_command`, so a
manifest task of either name shadows the built-in and `effigy <name> --help`
runs the repository's task. Following that through the help route would have
turned `effigy help` into an execution surface — a contract-`043` stop
condition. `reject_help_for_deferred_builtin` therefore also inspects the
resolved `Command::Task`, and `root_manifest_declares_task` refuses the help
route when the repository owns the selector. The refusal reuses the same message
the deferred-built-in guard emits.

`cli_help_for_builtin_owned_help_refuses_to_run_a_shadowing_selector` proves
both halves: the direct form still prints the task's own marker, while the help
route exits `2` and never emits that marker.

## Inventory Content Is Byte-Identical

The 39 general-help rows that existed before this card still exist, with the
same command string, the same description text, and the same
`deferred_builtin` value. Only their layout and their now-explicit primary
group changed. That was checked by extracting the pre-card rows from
`HEAD:crates/effigy-cli/src/command_surface.rs` plus the hard-coded chain in
`HEAD:crates/effigy-cli/src/help/topics/general.rs` and comparing the triples
against `GENERAL_HELP_ENTRIES`: 39 rows in, 39 rows out, no additions, no
removals, no text drift.

## Direct-Command Regression Proof

- `command_surface::tests::command_descriptors_cover_current_top_level_help_routes`
  reparses all 30 `effigy <command> --help` routes and asserts the same
  `HelpTopic` as before. Unchanged from the pre-card assertion.
- `help_and_flag_tests::parse_migrate_help_is_scoped` still resolves
  `effigy migrate --help` to a task invocation. `migrate` deliberately has no
  `effigy help` name, because giving it one would reserve a top-level word.
- `released_surface_v0_2_13_tests` and `released_surface_transition_tests` pass
  unchanged.
- The released-surface guard caught one real regression during implementation:
  `effigy help --repo <PATH>` had silently ignored its trailing flags.
  `parse_help_command` now accepts trailing `--json` and `--repo <PATH>` as
  inert, exactly as before, and only rejects a second topic.
- Completion candidates are generated from `effigy_core::builtin_tasks::BUILTIN_TASKS`,
  which this card did not touch, so no group word enters completions.

## Documentation And Generated Coverage

`tests/documentation_coverage_tests.rs::help_first_discovery_paths_are_documented`
asserts the new `## Help-First Discovery` section and all six
`effigy help <group>` routes in `docs/guides/025-command-reference-matrix.md`,
the `effigy help repo` / `effigy help docs` examples in
`docs/guides/021-quick-start-and-command-cookbook.md`, and the discovery text in
both copies of `references/built-in-surfaces.md`. The existing skill-parity test
keeps `.agents/skills/effigy/` and `skills/effigy/` byte-identical.

`AGENTS.md`, `README.md`, and `CHANGELOG.md` describe the grouped help path
without advertising executable grouped aliases.

## Validation

All checks below were run on the final tree, after the review repair described
in `Built-In-Owned Help`.

| Check | Result |
| --- | --- |
| `cargo test --workspace` | exit `0`; 95 targets, all `ok`, `0 failed` |
| — `cargo test --lib` leg | 1433 passed |
| — `cargo test --test cli_output_tests` leg | 290 passed, 1 ignored |
| — `cargo test --test documentation_coverage_tests` leg | 5 passed |
| `cargo test -p effigy-cli` | 18 passed |
| `effigy qa` | exit `0` |
| — `effigy test` leg (nextest) | 3517 passed, 1 skipped |
| — docs QA leg | links, json-examples, index, forbidden, headings, contains, workflow-paths, vision index, next-action all passed |
| — JSON contract leg | passed |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `git diff --check` | clean |

### Flakes under machine load, classified

Two failures appeared during development, both only while the machine was
running concurrent builds, and neither reproducible on an uncontended run.

`command_behavior_tests::cli_container_attached_session_handles_sigint_during_startup`
failed twice with `startup colima invocation marker was not created in time`. It
is load-sensitive, not card-sensitive: it failed identically at the base commit
`8a2ccd991` in a clean detached worktree with no part of this diff applied, it
passed in every uncontended `cargo test --workspace` run, and it passed in all
`effigy qa` nextest runs. The test spawns `container up` against a fake `colima`
shim with a three-second start delay and waits on a marker file, so a loaded
machine can miss the window.

A second, unnamed failure (`36 passed; 1 failed`) appeared once while two full
`cargo test --workspace` invocations were running concurrently against the same
target directory. The test name was not captured. The immediately following
uncontended run was green across all 95 targets, as was `effigy qa`. Recorded
here rather than omitted, but not attributed: no evidence links it to this diff,
and no container, runtime, or backend code is touched here.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`
- Moved: the flat ~39-row general help inventory became six job-grouped
  inventories with one typed primary owner per row, plus `effigy help <group>`
  and `effigy help <command>` discovery paths. Baseline `effigy help <topic>`
  silently rendered general help; it now fails deterministically.
- Unchanged: every executable route, argument, side effect, text and JSON
  contract, diagnostic, and exit code for direct commands; selector precedence;
  which rows the deferred-built-in filter hides. The filter's reach grew only to
  cover the new `effigy help <command>` surface, inheriting each row's existing
  deferral rather than defining a new one.
- Remaining open: the catalog-pack acquisition transport and update policy, the
  extension transport for optional providers, and the S3 consumer replacement
  gate. All three stay in planning under contract `043`.

## Next Task

Return to planning for the catalog-pack acquisition prototype under contract
[`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md).
Keep S3 extraction deferred until its consumer gate is proved. No release action
or generation rollover is implied.
