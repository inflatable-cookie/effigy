# Command-Surface Preview 1109 Closeout

Status: complete
Created: 2026-09-02
Roadmap: `g09.001`
Spec: `116` (archived with this closeout)
Card: `1109`
Branch: `worker/g09-001-command-surface-preview-1109`
PR: [inflatable-cookie/effigy#85](https://github.com/inflatable-cookie/effigy/pull/85)
Review head: `worker/g09-001-command-surface-preview-1109` tip at submission
(recorded with the PR URL); repair commit `d37b6f816` below, branch tip
returned for re-review is the pushed head of this closeout

## Outcome

Effigy now routes five executable job namespaces — `effigy local`, `effigy
repo`, `effigy deliver`, `effigy extend`, `effigy admin` — to their grouped
child commands while every retained direct spelling stays executable as a
warned migration alias until the explicit `v1.0` gate. One command
implementation and one output owner per operation: grouped children produce
the same typed `Command` values (or the same built-in registry run for
`config`/`scan`) as their direct spellings.

## Changed Public Routes

- `effigy <namespace>` renders the group inventory (same panel as
  `effigy help <namespace>`); `<namespace> --help`/`-h` too.
- `effigy <namespace> <child> [...]` delegates to the child's typed command
  parser with the remaining arguments; argument, exit, text, identity,
  result-payload, and error semantics are byte-identical to the direct
  spelling (`repo graph status --json` == `graph status --json`).
- The five namespace words are reserved over manifest deferral: an exact
  space-separated `local|repo|deliver|extend|admin` never resolves as a task
  selector or `[defer] builtins` target. `work` stays help-only; the daily
  spine (`<task>`, `<catalog>/<task>`, `tasks`, `test`, `watch`, `doctor`,
  `init`) and global flags (`help`, `--help`, `--version`, leading `--json`,
  `--repo`) stay direct and unchanged.
- Slash selectors (`admin/<task>`, `repo/graph`) remain catalog/task
  selectors; grouped parsing never splits them.
- Grouped `repo scan` / `admin config` execute the built-in registry command
  directly through a new `ExecutionSurface::GroupedBuiltin` path that skips
  manifest selector resolution — the explicit escape for the two children
  that own their parse inside the built-in layer.
- Retained direct spellings warn only when routing proves the built-in owns
  the invocation: typed commands prove it at parse time; `config`/`scan`
  prove it at the runner's manifest-selection fallback, recorded into a
  direct-CLI scope and drained by the entry point.

## Migration Warning Schema

Human mode writes one line to stderr; stdout and exit status are unchanged.
JSON mode keeps stdout as one `effigy.command.v1` document and adds a
top-level `warnings` array only when nonempty. Each item:

```json
{
  "code": "legacy-direct-command",
  "message": "direct command `graph` is deprecated; use `effigy repo graph`",
  "replacement": "effigy repo graph",
  "removal": "v1.0"
}
```

Grouped routes, the daily spine, manifest tasks, `[defer]`-owned routes, and
slash selectors never warn and never gain an empty `warnings` field.
Help-root invocations (`effigy help <command>`) emit no warning; legacy detail
panels carry one migration note (`direct command `graph` is deprecated; use
`effigy repo graph`; removal at v1.0`) while canonical
`effigy <namespace> <child> --help` panels render note-free. `config`/`scan`
own one help panel for both routes, so it renders the grouped usage plus one
deprecation line. Success, usage-error (parse and builtin argument
validation), and runtime-error envelopes carry the same warning facts.

## Review Oracle Mapping (card `1109` / spec `116`)

| Oracle row | Named proof |
| --- | --- |
| 1. grouped pair reaches different logic or changes inner text/JSON, side effects, or exit | `src/tests/lib_tests_parse_tests/grouped_command_tests.rs` (`every_namespace_child_parses_identically_to_its_direct_spelling`, `grouped_child_help_renders_the_existing_typed_panel`, `grouped_child_flags_and_positional_args_retain_the_direct_parser`) plus `tests/cli_output_tests/grouped_command_surface_tests.rs` (`json_success_parity_between_direct_and_grouped_routes` compares `command`, `result`, and `error` payload equality with exit parity) |
| 2. `admin/<task>` or another slash selector enters grouped parsing | `grouped_command_tests.rs::slash_selectors_never_enter_grouped_parsing`; `tests/cli_output_tests/grouped_command_surface_tests.rs::catalog_slash_alias_stays_a_selector_under_admin_namespace` (real `[catalog] alias = "admin"` fixture; `admin/hello` runs the task, space-separated `admin hello` is a usage error) |
| 3. shadowing manifest task bypassed or warned through retained direct route | `grouped_command_surface_tests.rs::grouped_route_escapes_shadowing_while_direct_route_defers` (root `[tasks.docs]`: direct `effigy docs` runs the task with no warning; grouped runs the built-in), `registry_builtin_direct_routes_warn_only_when_the_builtin_is_selected` (root `[tasks.scan]`: direct runs the task silently, grouped escapes) |
| 4. grouped route cannot reach a built-in whose direct name is shadowed | same two fixtures (`repo docs check links .` and `repo scan god-files` under shadowing manifests), plus `src/cli/entrypoint.rs` `namespace_reservation_tests::grouped_routes_escape_root_task_shadowing_at_parse_time` and `repo_override_still_resolves_for_grouped_invocations` (leading `--repo` override) |
| 5. JSON stdout invalid envelope / empty warnings on unrelated routes / success-error divergence | `json_success_parity...`, `json_usage_error_warning_parity_between_direct_and_grouped_routes` (CliParseError envelope exit 2), `json_runtime_error_warning_parity_between_direct_and_grouped_routes` (RunnerError envelope exit 1), `registry_builtin_direct_routes_warn_only_when_the_builtin_is_selected` (builtin usage-error envelope carries the warning); absence assertions (`warnings` key absent for grouped/daily/help-root) in `daily_spine_and_help_routes_never_warn` and `src/cli/legacy_direct.rs` unit tests |
| 6. unknown grouped child executes a task | `grouped_command_tests.rs::unknown_grouped_child_fails_as_usage_and_lists_children` + `grouped_parse_never_falls_through_to_a_task_selector`; `grouped_command_surface_tests.rs::unknown_grouped_child_never_runs_a_same_named_task` (root owns `[tasks.deploy]`; `effigy repo deploy` exits 2, never executes the task) |
| 7. help/completion has two primary spellings or omits the legacy migration note | `src/tests/lib_tests_help_render_tests.rs` (general/group panels teach grouped rows, no direct rows), `crates/effigy-cli/src/command_surface/tests.rs` (`group_inventories_match_the_contract_taxonomy`, `deferred_builtin_for_help_topic_matches_the_inventory_row`), `crates/effigy-builtin` `command_index.rs` `completion_primary_surface_excludes_displaced_direct_spellings`, `grouped_command_surface_tests.rs::legacy_detailed_help_carries_the_migration_note_only_on_legacy_routes` + `json_help_legacy_payload_keeps_the_note_inside_the_text` |
| 8. displaced direct route, `watch`, global flag, or daily spine removed or changed | direct spellings still run (all warning fixtures exercise them); `help_and_flag_tests`/`graph_option_tests` etc. unchanged and green; retained `watch`/`tasks`/`test`/`doctor`/`init` parse and help tests green; full suites below |

Smallest counterexample set coverage: one child per namespace
(`grouped_command_tests.rs` full NAMESPACE_CHILDREN table, one typed value per
child); all children in a typed mapping test (same table); a shadowing `docs`
task (Tier C); an `admin` catalog alias with `admin/hello` and a spaced
`admin hello`; success, usage, and runtime JSON envelopes; an unknown child
backed by a same-named manifest task; help/completion snapshots containing
canonical-primary rows and legacy-detail note facts.

## Validation Actually Run

- `cargo test -p effigy --lib` — 1501 passed, 0 failed (final tree)
- `cargo test -p effigy --test cli_output_tests` — 302 passed, 1 failed,
  1 ignored: `cli_container_attached_session_handles_sigint_during_startup`
  (pre-existing environment flake; reproduced failing on the pristine base
  commit `55c29fca` in an isolated worktree, passes in isolation)
- `cargo test -p effigy --test documentation_coverage_tests` — 5 passed
- `cargo test -p effigy-cli --lib` — 18 passed
- `cargo test -p effigy-builtin` — 62 passed
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `git diff --check` — clean
- `effigy doctor --json` — exit 0, `effigy.command.v1` ok envelope
- `effigy qa:docs` (links, examples, index, agent-defaults, vision headings,
  workflow paths, next-action) — all passed on the final tree; repo QA task
  wiring moved to grouped spellings

## Consumer-Impact Reconciliation

The 2026-09-02 inventory found no bare-task collision for the five namespace
words across 30 top-level Effigy repositories, and no collision appeared
during implementation. Compli-me's catalog alias `admin/<task>` semantics are
covered by the alias fixture (`catalog_slash_alias_stays_a_selector...`).
Direct invocations now emit the migration warning exactly when the built-in is
selected; manifest-owned and slash-selector routes are silent. Repo-owned QA
configurations in this repository moved to grouped spellings so their runs
stay warning-free.

## Vision Target Delta

- Primary tags: `MAINT`, `ROUTE`, `RELEASE`
- Movement: help-only job grouping -> five executable command namespaces with
  grouped children, retained direct migration aliases with structured
  warnings, grouped-primary help/completion/docs/skill, and no routing or
  payload change for the daily spine, task selectors, or slash selectors
- Remaining gap: direct-route removal at `v1.0` (refreshed consumer inventory
  plus explicit release authority) and any release execution; both remain
  outside this PR

## Remaining v1.0 Gate

Direct-route removal is NOT part of this lane: no displaced route was removed,
no workflow or release mutation happened, and no removal card was created. The
next checkpoint is the future `v1.0` consumer-evidence gate (refreshed
consumer inventory plus explicit release authority). Historical direct
spellings in logs, archived specs, closed roadmaps, and the changelog were not
rewritten.

## Exact-Head Review Repair

Review of head `f8fa874cb91267ee12db1742026893cf7966625c` returned three
findings; all repaired on the same branch (classes `execution-miss`,
`integration-drift`, `oracle-gap`). Repair head:
`d37b6f816` (returned for re-review).

### 1. Nested registry fallback warning leak (execution-miss)

The thread-local recording scope stayed open through a shadowing manifest
task, so a nested registry fallback (counterexample: `[tasks.scan]`
`run = [{ task = "config" }]`, running `effigy scan`) populated the
top-level warning even though the repository-owned task executed. Recording
is now bound to the original direct child AND the top-level execution depth:
`open_registry_scope(task_name)` records only when the fallback selected the
same word at depth one; every execution request enters a depth guard
(`run_manifest_task_request_inner`), so nested fallbacks at any depth never
record.

Fixtures: `grouped_command_surface_tests.rs::nested_registry_fallback_never_warns_through_a_shadowing_manifest_task`
(text stderr empty, JSON envelope without a `warnings` key) plus unit tests
in `src/cli/legacy_direct.rs` (`registry_scope_records_only_the_original_child_at_top_depth`,
`nested_executions_never_populate_the_scope`). Contract `043` and archived
spec `116` now state the nested-execution binding.

### 2. Direct `graph watch` migration diagnostic and JSON-stream exception (integration-drift)

`run_cli` dispatched `Command::Graph(Watch)` straight to
`run_graph_watch_command`, dropping the classified warning. The direct
spelling now passes the warning through and emits the single stderr line in
text and JSON-stream modes; grouped `repo graph watch` stays silent. Event
stdout is untouched: the JSON stream still emits `effigy.graph.watch.event.v1`
lines and never a command envelope, matching guide `017`. Contract `043` and
archived spec `116` record the streaming exception to the envelope-warning
rule.

Fixtures: `grouped_command_surface_tests.rs::direct_graph_watch_warns_once_in_text_and_json_stream_modes`
(bounded spawns; text warning count, JSON stream schema, no envelope on
stdout, grouped silence in both modes).

### 3. `version` legacy help forms lacked the migration note (oracle-gap)

`version` renders the shared `General` panel, which has no single direct
owner, so `effigy help version` and `effigy version --help` carried no note.
`legacy_help_note` now takes the help-root topic word: the two legacy
word-based forms render the `effigy admin version` / v1.0 note, while
`--version` (unchanged), bare `effigy help`, and canonical
`effigy admin version --help` stay note-free in text and JSON payloads.

Fixtures: `grouped_command_surface_tests.rs::legacy_version_help_forms_carry_the_migration_note`
(text forms, `--version` purity, grouped note-free form, and JSON payload
assertions for both legacy forms plus the grouped form).

### Repair validation

- `cargo test -p effigy --test cli_output_tests` — full suite green on the
  repaired tree (includes the three new counterexample fixtures)
- `cargo test -p effigy --lib` — full suite green
- `cargo test -p effigy-cli --lib`, `cargo test -p effigy-builtin`,
  `cargo test -p effigy --test documentation_coverage_tests` — green
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `git diff --check` — clean
- `effigy qa` — exit 0 on the repaired tree
