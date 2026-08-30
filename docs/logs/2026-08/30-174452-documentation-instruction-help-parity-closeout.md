# Documentation, Instruction, And Help Parity Refresh Closeout

Status: complete
Created: 2026-08-30
Roadmap: g08.036
Card: 1091
Spec: 109 (archived)

## Summary

Card `1091` refreshed the current documentation, agent-instruction, generated
reference, and shipped-help surfaces. The active queue now returns to card
`1089` in `g08.035`; no docs-context command was added in this maintenance
lane.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`, `ROUTE`
- Movement: the post-`g08.034` parity baseline now covers the shipped graph
  timeout/refresh behavior, live JSON envelope paths, the repository-defined
  docs graph profile, special help families, and both skill trees.
- Remaining gap: bounded `effigy docs context` retrieval remains card `1089`;
  code-only scan findings remain outside this documentation lane.

## Authority And Boundary

Implementation-side authority was rebuilt from the current command descriptors,
parsers, built-in task registry, manifest config types, JSON output, behavior
tests, generated help/config output, and `CHANGELOG.md`. Active docs own routes
and explanation. Historical logs, archived specs, closed roadmap prose,
vendored files, generated build output, release artifacts, workflows, and
production behavior were left untouched.

## Current Behavior Matrix

| Behavior family | Source owner / proof | Active route(s) | Finding and disposition |
| --- | --- | --- | --- |
| General help, version, global `--json`, global `--repo` | `crates/effigy-cli/src/command_surface.rs`, help registry, parser | `effigy --help`, `effigy help`, `README.md`, guides `017` and `025`, both skills | Routes checked; JSON examples now use the command envelope and live result paths. |
| Selector execution, catalog routing, managed task companions | `crates/effigy-core/src/builtin_tasks.rs`, runner routing, task parser | `README.md`, guides `016`, `025`, `050`, `012`, both skills | Existing routes retained; registry-to-matrix guard remains green. |
| `tasks`, `tasks status`, `tasks migrate`, `tasks unlock`, `tasks cache` | Built-in registry and task command parser | Matrix `025`, guides `016`, `019`, `020`, `022`, help routes | Scoped help families checked; no unresolved gap. |
| `test` and `watch` | test/watch command parsers and suite types | Matrix `025`, guides `013`, `019`, `048`, `055`, help | Current plan/run and watcher routes present. |
| `config` and shell completion | config renderer/schema, completion parser | Matrix `025`, guide `021`, generated reference/schema, help | Added generated `[docs_policy.graph]` profile coverage; completion route uses `effigy --json config completion candidates`. |
| `docs` QA and log-index commands | `crates/effigy-cli/src/runner/docs_command` | Matrix `025`, guide `029`, docs front door, help | `docs context` is not claimed; it remains the separate ready card `1089`. |
| `contracts` validation | contracts command surface and JSON selection artifacts | Matrix `025`, guide `017`, contracts front door, help | Route and schema references current. |
| `doctor` and explain-mode diagnostics | doctor runner, `effigy.doctor.v1` output | Matrix `025`, guides `018`, `063`, skills, help | Replaced stale `result.payload.checks/status` guidance with `result.findings[]`, evidence, severity, and remediation. |
| All ten `scan` families and `--graph-context` | scan registry and graph-aware scan runners | Matrix `025`, guides `076`, `022`, skills, help | Families listed and rescanned; final changed-file validation-gap scan is clean. |
| Code graph index/status/query/watch | graph command/parser, codegraph freshness and timeout source | Matrix `025`, guide `076`, skills, help | Added shipped `status --refresh`, 120000ms default, `EFFIGY_GRAPH_TIMEOUT_MS`, complete skip list, and recoverable cache recovery. |
| Deploy model/export/plan/apply/status/history/redeploy | deploy descriptors and runner | Matrix `025`, guides `074`, contracts `002` and `019`, help | Route checked; human-gated mutation guidance retained. |
| Release status/gates/resume/verify/preflight/validate/proof/evidence/prepare/execute | release descriptors and release protocol | Matrix `025`, guides `051`, `062`, `049`, skills, help | Release route current; no release action performed. |
| Bootstrap, bundle, and demo lifecycle | bootstrap/bundle/demo parsers and runners | Matrix `025`, guides `057`, `065`, `058`, help | Scoped routes checked; no unresolved gap. |
| Container, system, workspace, gateway, service, and exec | command descriptors plus container/runtime owners | Matrix `025`, guides `063`, `064`, `067`, `069`, help | Current lifecycle, workspace identity, non-console, and cleanup routes present. |
| Dependency links, committed Bun pins, and status | deps domain and command parser | Matrix `025`, guide `077`, changelog, skills, help | Existing parity retained; no unrelated dependency behavior changed. |
| Secrets and vault lifecycle | secrets runner/config types | Matrix `025`, guide `075`, contract `032`, skills, help | Metadata/value-separation route current. |
| Papercuts discovery/capture | papercuts command and domain | Matrix `025`, guide `078`, skills, help | Route current. |
| Defer and Rhai host surface | defer/rhai descriptors and runners | Matrix `025`, guides `015`, `061`, `068`, skills, help | Route current; no runtime changes. |
| State stacks and artifacts | state/artifact descriptors and domain crates | Matrix `025`, guides `072`, `073`, contract `016`, help | Plan/apply/capture and artifact routes current. |
| Changelog extraction and validation | changelog parser/runner | Matrix `025`, guide `052`, skills | Corrected JSON invocation to `effigy --json changelog extract ...`. |
| Uninstall planning/confirmation | uninstall route and help descriptor | Matrix `025`, `effigy uninstall --help` | Added missing primary matrix row; `--plan` and `--yes` are documented. |

### Manifest and generated-reference matrix

| Manifest family | Source owner / proof | Active route(s) | Disposition |
| --- | --- | --- | --- |
| `[manifest]`, includes, and `[task_defaults]` | `effigy-manifest` composition and config docs | Generated `effigy config`, schema, guide `059`, matrix | Current; profile output is adjacent to manifest docs. |
| `[docs_policy]` indexes/next actions and `[docs_policy.graph]` | `ManifestDocsPolicyGraphConfig`, contract `041`, architecture `024`, Northstar starter | Generated reference/schema, guide `025`, docs front doors, both skill config-shape refs | Generic output carries a repository-neutral graph grammar example; the bundled Northstar starter owns the exact Northstar profile. |
| `[tasks]`, managed sessions, cache, `[env]` | manifest task/config types and runtime tests | Generated config, guides `012`, `022`, `050`, `055`, skills | Current; task inventory examples use `.result.catalog_tasks[].task`. |
| `[systems]` and workspaces | manifest system/workspace types | Generated config, guide `064`, matrix, skills | Current. |
| `[containers]`, data, lifecycle, health, DNS, mounts | container manifest types and tests | Generated config, guides `063`, `067`, `069`, matrix | Current. |
| `[package_manager]` and `[test]` | test config types and config docs tests | Generated config/schema, guides `013`, `048`, `050`, matrix | Current. |
| `[bootstrap]`, `[catalog]`, `[bundle]` | bootstrap/catalog/bundle manifest types | Generated config, guides `057`, `065`, `066`, matrix | Current. |
| `[distribution]` and `[release]` | release/distribution config types | Generated config/schema, guides `049`, `051`, `062`, matrix | Current; no release mutation. |
| `[secrets]` | secrets manifest types and doctor tests | Generated config, guide `075`, contract `032`, matrix | Current. |
| `[state]`, `[deploy]`, `[defer]`, `[shell]`, `[scan]`, env schema | corresponding manifest/config modules | Generated config/schema, guides `050`, `073`, `074`, matrix | Current. |

## Northstar AGENTS Review

The target-local `effigy check:agent-instructions` selector was unavailable,
so the installed Northstar catalog was used with the target path:
`effigy --repo /Users/tom/.agents/skills/northstar northstar/check:agent-instructions /Users/tom/.t3/worktrees/effigy/t3code-20297a91`.

- `AGENTS.md`: 97 non-blank lines, 5596 bytes, approximately 1399 tokens,
  10 headings, 8 links, 1 fenced block
- placement leads: 5; procedure leads: 11; freshness leads: 2
- `CLAUDE.md` bridge: exact `@AGENTS.md`, status OK
- findings: the root instructions already route through docs authority, graph
  workflows, strict contracts, release/workflow boundaries, and Rust quality;
  no bounded repair was needed
- retained decision: leave `AGENTS.md` and `CLAUDE.md` unchanged; the parity
  repairs live in active docs, help, generated config, tests, and both skills

## Help And Rendered Config Proof

- Every current top-level and scoped help family was invoked successfully,
  including `tasks migrate`, `tasks unlock`, `tasks cache`, `config completion`,
  and `config completion candidates`.
- `cargo run --quiet --bin effigy -- graph --help` renders `--refresh`,
  `EFFIGY_GRAPH_TIMEOUT_MS=<MS>`, the 120000ms default, and the
  `.effigy/graph.backup-$(date +%s)` recovery example; the destructive
  `rm -rf .effigy/graph` example is absent.
- `cargo run --quiet --bin effigy -- config` and
  `config --schema --target manifest` both render `[docs_policy.graph]`,
  `roots = ["README.md", "docs"]`, and canonical `default-currentness`;
  `config --schema --minimal` omits the optional profile.
- The bundled `northstar` starter emits the Northstar graph profile, while the
  `minimal` starter remains profile-free.
- Live JSON spot checks confirmed `tasks.result.catalog_tasks[].task`,
  `test --plan.result.targets[]`, `doctor.result.findings[]`,
  `config completion candidates.result.candidates[]`,
  `release status.result.gates.results[]`, and graph status
  `result.payload.freshness`.

## Scan Results And Dispositions

All baseline counts below were taken after the explicit graph refresh/index
completed, so stale graph state was not treated as a repository finding.

| Scan | Before | After | Disposition |
| --- | --- | --- | --- |
| `god-files` | 5 warnings | 5 warnings | Code-only; retain for deps/state owners. |
| `boundary-violations` | 0 | 0 | No findings. |
| `dead-code` | 0 | 0 | No findings. |
| `duplicate-blocks` | 107 (critical 1, high 2, warning 104) | 107 (critical 1, high 2, warning 104) | Code-only; no docs/help repair. Critical pair is `crates/effigy-deps/src/bun_apply/tests.rs` ↔ `bun_unlink/tests.rs`; high pairs are `bun_apply.rs` ↔ `bun_unlink.rs` and the Bun inventory block in `bun_plan.rs`. |
| `comment-ratio` | 0 | 0 | No findings. |
| `generated-assets` | 0 | 0 | No findings. |
| `generated-in-src` | 0 | 0 | No findings. |
| `attention-markers` | 0 | 0 | No findings. |
| `stale-suppressions` | 10 high | 10 high | Code-only; retain for `crates/effigy-builtin/src/ports.rs`, `crates/effigy-containers/src/policy_support/generated_compose.rs`, `crates/effigy-manifest/src/bundles/source.rs` (2), `crates/effigy-manifest/src/test_config.rs` (2), `crates/effigy-state/src/config.rs`, `crates/effigy-tui/src/multiprocess/render/panes/mod.rs`, `src/runner/bootstrap_command/tests.rs`, and `src/tests/runner_tests/prelude/deferral.rs`. |
| changed-file `validation-gaps` | not applicable | 0 | No changed-file validation gaps. |

The five `god-files` warnings are `crates/effigy-deps/src/status.rs`,
`src/runner/deps_command.rs`, `crates/effigy-deps/src/bun_plan.rs`,
`crates/effigy-deps/src/cargo_plan.rs`, and `src/runner/state_command.rs`.
Their owners and the duplicate/suppression owners are code-quality lanes, so
they are explicitly deferred rather than disguised as parity completion.

`effigy --json doctor` remained structurally healthy: 19 passing checks, one
warning-level god-file section, and zero errors. `effigy --json tasks` remained
healthy with 29 built-ins and 30 catalog tasks. The graph status was
`ready`/usable with no stale paths after refresh.

## Changes

- Extended graph help, render tests, guide `076`, matrix `025`, and both graph
  skill references with `status --refresh`, timeout semantics, the full current
  skip list, and recoverable cache reset guidance.
- Added repository-neutral `[docs_policy.graph]` reference/schema output,
  kept the minimal schema profile-free, and made the bundled Northstar starter
  the adoption surface for the exact Northstar profile.
- Corrected root README JSON selectors and job-based agent routing, aligned the
  Northstar starter and consumer guide entry routes, and corrected both skill
  JSON-envelope references for live task, doctor, completion, release, error,
  and streaming behavior.
- Corrected the JSON changelog invocation in both skill workflow references.
- Added matrix/front-door routes for the docs graph profile, uninstall, and
  command/help families; kept card `1089` explicitly unshipped.
- Added deterministic coverage guards for public help families, live JSON
  paths, matrix profile markers, project-local/distributed skill parity, the
  neutral generic graph example, the Northstar starter boundary, and the
  job-based README route plus the qualified graph status mutation boundary.
- Updated `CHANGELOG.md` under `[Unreleased]`.
- Archived strict spec `109`, completed card `1091`/roadmap `g08.036`, and
  returned strict spec `108`, roadmap `g08.035`, and card `1089` to active/ready.
- No changes to `AGENTS.md` or `CLAUDE.md` were necessary.

## Recurrence Guards

- `tests/documentation_coverage_tests.rs` keeps the built-in registry routed to
  matrix `025`, checks special help families and stable live JSON selectors,
  and checks both skill trees for parity markers.
- `crates/effigy-builtin/src/config/docs/tests.rs` checks the repository-neutral
  graph profile renderer in both reference and schema modes, including
  canonical key spelling and the absence of Northstar ontology.
- `crates/effigy-catalog/src/starter.rs` checks that the Northstar starter owns
  the Northstar profile and the minimal starter does not.
- Config schema output tests check that minimal full and manifest-target
  schemas omit the optional graph profile.
- `src/tests/lib_tests_help_render_tests.rs` checks the graph help refresh,
  timeout, and recoverable-cache guidance.
- Existing skill inventory/semantic-parity coverage remains active.

## Validation Performed

- `effigy --json tasks`: passed
- `effigy --json doctor`: passed with the known warning-only god-file section
- all scoped help routes: passed
- rendered `config`, `config --schema --target manifest`, and `graph --help`:
  passed with the expected current tokens
- `cargo test -p effigy-builtin docs_policy_graph_profile_lines_are_repository_neutral`:
  passed
- `cargo test -p effigy-catalog northstar_starter_owns_the_northstar_graph_profile`:
  passed
- focused `documentation_coverage_tests` parity guard: passed
- `effigy test --test documentation_coverage_tests`: 4 passed, 0 skipped
- `effigy qa:docs`: passed (links, JSON examples, indexes, workflow paths,
  policy indexes, next-action checks, and contract selection)
- `effigy docs check workflow-paths`: passed
- `effigy qa:docs:agent-defaults`: passed
- `effigy qa`: 3411 passed, 1 skipped
- `cargo fmt --all -- --check`: passed
- `cargo clippy --all-targets -- -D warnings`: passed
- `git diff --check`: passed

## Residuals

- Card `1089` remains the single active next task for bounded
  `effigy docs context` retrieval and its JSON/help contract.
- Code-only scan findings remain with their existing owners. No production
  behavior, public runtime contract, workflow, release, or historical evidence
  change is part of this closeout.

## Next Task

Execute ready card
[`1089`](../../roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md).
