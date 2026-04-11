# 025 - Command Reference Matrix

This matrix is a quick operator reference for Effigy commands, key flags, JSON payload schemas, and deep-dive guides.


## Vision Alignment

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`
- Target movement: command lookup stays fast while linking every surface to stable schemas and deep-dive guidance.

## Start Here

Use this page when you already know roughly what you want to do and need the
right command shape fast.

If you do not know where to start yet, use these first:

```sh
effigy tasks
effigy tasks --resolve test
effigy doctor --verbose
effigy test --plan
effigy --json tasks
```

For narrative workflow guidance instead of lookup, start with:

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)

## Pick The Right Command Quickly

- Need to discover tasks or inspect routing: use `effigy tasks`.
- Need health checks or routing diagnosis for one selector: use `effigy doctor`.
- Need tests, watch mode, init, or migrate: use `effigy test`, `watch`,
  `init`, or `migrate`.
- Need machine-readable output: add `--json`.
- Need repo-health scanners: use `effigy scan`.
- Need to clone or update a repo and run its declared bring-up path: use
  `effigy bootstrap`.
- Need proof/demo discovery, inspection, or one-off proof execution: use
  `effigy demo`.
- Need release workflows: use `effigy release`.

## 1) Primary Commands

| Command | Purpose | Key Flags | JSON Schema(s) | Deep Dive |
| --- | --- | --- | --- | --- |
| `effigy help` / `effigy --help` | Show CLI help and topic guidance | `--json` | `effigy.help.v1` (inside command envelope) | `021-quick-start-and-command-cookbook.md` |
| `effigy tasks` | List discovered catalogs/tasks and probe routing | `--repo`, `--task`, `--resolve`, `--json`, `--pretty true\|false` | `effigy.tasks.v1`, `effigy.tasks.filtered.v1` | `016-task-routing-precedence.md` |
| `effigy doctor` | Run health checks and optional explain-mode selection diagnostics | `--repo`, `--fix`, `--verbose`, `--json` | `effigy.doctor.v1`, `effigy.doctor.explain.v1` | `018-doctor-explain-mode.md` |
| `effigy docs` | Run reusable docs QA checks such as path presence, link validation, heading/content/forbidden-text checks, JSON example validation, markdown index consistency checks, next-action policy validation, workflow-path validation, and log-index entry insertion | `check-links`, `check-json-examples`, `check-headings`, `check-paths`, `check-contains`, `check-forbidden`, `check-index`, `check-next-action`, `check-workflow-paths`, `add-log-index`, `--repo`, `--file`, `--section`, `--min-blocks`, `--require`, `--require-heading`, `--require-block`, `--forbid`, `--policy-index`, `--policy`, `--dir`, `--index`, `--json` | `effigy.docs.link-check.v1`, `effigy.docs.json-examples.v1`, `effigy.docs.heading-check.v1`, `effigy.docs.path-check.v1`, `effigy.docs.contains-check.v1`, `effigy.docs.forbidden-check.v1`, `effigy.docs.index-check.v1`, `effigy.docs.next-action-check.v1`, `effigy.docs.workflow-path-check.v1`, `effigy.docs.add-log-index.v1` | `029-docs-qa-checklist-and-validation.md` |
| `effigy contracts` | Validate reusable JSON contract artifacts such as selection payloads and schema-index contract coverage | `check-json`, `validate-selection`, `--repo`, `--index`, `--fast`, `--full`, `--changed-only`, `--print-selected`, `--contract`, `--artifact`, `--json` | `effigy.contracts.check-json.v1`, `effigy.contracts.selection-validation.v1` | `017-json-output-contracts.md` |
| `effigy distribution` | Run non-publish distribution preflight checks, validate release/distribution metadata, write first-publish summary contracts, check artifact bundles, and generate acceptance closeout logs from captured artifacts | `preflight`, `validate-metadata`, `validate-artifacts`, `generate-closeout`, `write-summary`, `--repo`, `--tag`, `--skip-docs`, `--skip-smoke`, `--artifacts-dir`, `--crate-version`, `--repo-url`, `--brew-formula`, `--output`, `--owner`, `--expect-homebrew`, `--homebrew-executed`, `--log-file`, `--json` | `effigy.distribution.preflight.v1`, `effigy.distribution.metadata.v1`, `effigy.distribution.artifacts.v1`, `effigy.distribution.closeout.v1`, `effigy.distribution.summary.v1` | `044-distribution-first-publish-execution-runbook.md` |
| `effigy bootstrap` | Clone or update a repo from a git URL, apply its root bootstrap contract, sync optional submodules, bring along child repos, run setup, and optionally start the declared dev task | `<git-url>`, `--path`, `--branch`, `--start`, `--plan`, `--json` | `effigy.bootstrap.v1` | `057-bootstrap-repo-bringup.md` |
| `effigy demo` | Discover repo-owned proof demos, inspect active/latest state, execute new attempts, and control runner-owned lifecycle for active demos | `list`, `inspect`, `run`, `stop`, `rerun`, `--repo`, `--json` | `effigy.demo.list.v1`, `effigy.demo.inspect.v1`, `effigy.demo.run.v1`, `effigy.demo.stop.v1`, `effigy.demo.rerun.v1` | `022-manifest-cookbook.md` |
| `effigy scan` | Run built-in repo scanners such as oversized code-file detection, duplicate-block detection, comment-ratio detection, bulky generated-asset detection, generated-in-src detection, attention-marker detection, and stale-suppression detection | `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-assets`, `generated-in-src`, `attention-markers`, `stale-suppressions`, `--json`, `--markdown`, `--out`, `--fail-on-findings`, `--show-warnings` | `effigy.scan.god-files.v1`, `effigy.scan.duplicate-blocks.v1`, `effigy.scan.comment-ratio.v1`, `effigy.scan.generated-assets.v1`, `effigy.scan.generated-in-src.v1`, `effigy.scan.attention-markers.v1`, `effigy.scan.stale-suppressions.v1` | `022-manifest-cookbook.md` |
| `effigy test` | Run built-in or explicit `tasks.test` test orchestration | `--plan`, `--verbose-results`, `--tui`, `--json` | `effigy.test.plan.v1`, `effigy.test.results.v1` | `013-testing-orchestration.md` |
| `effigy watch` | Policy-first file-triggered reruns for a target task | `--owner`, `--debounce-ms`, `--include`, `--exclude`, `--once`, `--max-runs`, `--json` | `effigy.watch.v1` (bounded JSON runs) | `019-watch-init-migrate-foundation.md` |
| `effigy init` | Scaffold baseline `effigy.toml` | `--dry-run`, `--force`, `--json` | `effigy.init.v1` | `019-watch-init-migrate-foundation.md` |
| `effigy migrate` | Import `package.json` scripts into `[tasks]` | `--from`, `--script`, `--apply`, `--json` | `effigy.migrate.v1` | `019-watch-init-migrate-foundation.md` |
| `effigy config` | Render config reference/schema snippets or inspect the effective composed manifest | `--inspect`, `--path`, `--schema`, `--minimal`, `--target`, `--runner`, `--json` | `effigy.config.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy unlock` | Clear lock scopes manually | `--all`, `--json` | `effigy.unlock.v1` | `020-dag-lock-policy-baseline.md` |
| `effigy cache` | Inspect and invalidate phase-1 cache metadata | `inspect`, `invalidate`, `--all`, `--json` | `effigy.cache.v1` | `022-manifest-cookbook.md` |
| `effigy completion` | Generate shell completion scripts and selector candidates | `bash\|zsh\|fish`, `candidates`, `--repo`, `--prefix`, `--json` | `effigy.completion.v1`, `effigy.completion.candidates.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy changelog` | Validate, format, analyze, and extract Northstar changelog content | `validate`, `format`, `analyze`, `extract`, `--write`, `--preview`, `--version`, `--json` | changelog subcommands render direct output; some results can be wrapped in `effigy.command.v1` with global JSON mode | `052-changelog-workflows-and-northstar-profile.md` |
| `effigy release` | Inspect release readiness, run gates, preview or apply release mutations, resume prepared-state review, execute release flow, and verify tagged installs | `status`, `gates`, `resume`, `simulate`, `prepare`, `execute`, `verify-install`, `--check-gates`, `--plan`, `--dry-run`, `--yes`, `--version`, `--allow-stale`, `--tag`, `--repo-url`, `--json` | `effigy.release.status.v1`, `effigy.release.gates.v1`, `effigy.release.resume.v1`, `effigy.release.simulate.v1`, `effigy.release.prepare.plan.v1`, `effigy.release.prepare.v1`, `effigy.release.execute.plan.v1`, `effigy.release.execute.v1`, `effigy.release.verify-install.v1` | `051-release-orchestration.md` |
| `effigy <task>` / `effigy <catalog>/<task>` | Run manifest-defined tasks with routing rules | passthrough args, `--json` | `effigy.task.run.v1` | `022-manifest-cookbook.md` |

## 2) Global JSON Envelope

For sample payloads per schema, see [`026-json-payload-examples.md`](./026-json-payload-examples.md).


Canonical JSON mode:

```sh
effigy --json <command>
```

All command JSON responses are wrapped in:
- envelope schema: `effigy.command.v1`
- command-specific payload in `result` (or `error.details` for some failures)

See [`017-json-output-contracts.md`](./017-json-output-contracts.md) for envelope and payload details.

## 3) Command Shapes

```sh
effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>] [--json] [--pretty true|false]
effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]
effigy doctor [--repo <PATH>] <task> -- <args> [--json]
effigy docs check-links [--repo <PATH>] [<FILE>...] [--json]
effigy docs check-json-examples [--repo <PATH>] [--file <PATH>] [--section <TITLE>] [--min-blocks <N>] [--require <TEXT>]... [--require-block <N:TEXT>]... [--json]
effigy docs check-headings [--repo <PATH>] <FILE>... --require-heading <TEXT>... [--json]
effigy docs check-paths [--repo <PATH>] <PATH>... [--json]
effigy docs check-contains [--repo <PATH>] <FILE>... --require <TEXT>... [--json]
effigy docs check-forbidden [--repo <PATH>] <FILE>... --forbid <TEXT>... [--json]
effigy docs check-index [--repo <PATH>] [--policy-index <NAME>] [--dir <PATH>] [--index <PATH>] [--json]
effigy docs check-next-action [--repo <PATH>] [--policy <NAME>] [--json]
effigy docs check-workflow-paths [--repo <PATH>] [--dir <PATH>] [--json]
effigy docs add-log-index [--repo <PATH>] <LOG_FILE> [--json]
effigy contracts check-json [--repo <PATH>] [--index <PATH>] [--fast|--full] [--changed-only <BASE>] [--print-selected|--print-selected=json] [--json]
effigy contracts validate-selection [--repo <PATH>] [--contract <PATH>] [--artifact <PATH>] [--json]
effigy distribution preflight [--repo <PATH>] [--tag <TAG>] [--skip-docs] [--skip-smoke] [--output <PATH>] [--json]
effigy distribution validate-metadata [--repo <PATH>] [--tag <TAG>] [--json]
effigy distribution validate-artifacts [--repo <PATH>] --artifacts-dir <DIR> [--expect-homebrew] [--json]
effigy distribution generate-closeout [--repo <PATH>] --tag <TAG> --artifacts-dir <DIR> [--output <PATH>] [--owner <NAME>] [--expect-homebrew] [--json]
effigy distribution write-summary [--repo <PATH>] --tag <TAG> --artifacts-dir <DIR> [--crate-version <VER>] [--repo-url <URL>] [--brew-formula <NAME>] [--homebrew-executed] [--log-file <NAME>]... [--json]
effigy bootstrap <GIT_URL> [--path <DIR>] [--branch <NAME>] [--start] [--plan] [--json]
effigy demo list [--repo <PATH>] [--json]
effigy demo inspect <DEMO_ID> [--repo <PATH>] [--json]
effigy demo run <DEMO_ID> [--repo <PATH>] [--json]
effigy demo stop <DEMO_ID> [--repo <PATH>] [--json]
effigy demo rerun <DEMO_ID> [--repo <PATH>] [--json]
effigy scan god-files [--threshold <N>] [--high <N>] [--critical <N>] [--show-warnings] [--markdown] [--out <PATH>] [--fail-on-findings] [--no-gitignore] [--include <GLOB>] [--exclude <GLOB>] [--json]
effigy scan duplicate-blocks [--threshold <N>] [--high <N>] [--critical <N>] [--show-warnings] [--markdown] [--out <PATH>] [--fail-on-findings] [--no-gitignore] [--include <GLOB>] [--exclude <GLOB>] [--json]
effigy scan comment-ratio [--threshold <RATIO>] [--high <RATIO>] [--critical <RATIO>] [--min-code-lines <N>] [--show-warnings] [--markdown] [--out <PATH>] [--fail-on-findings] [--no-gitignore] [--include <GLOB>] [--exclude <GLOB>] [--json]
effigy scan generated-assets [--threshold <BYTES>] [--high <BYTES>] [--critical <BYTES>] [--show-warnings] [--markdown] [--out <PATH>] [--fail-on-findings] [--no-gitignore] [--include <GLOB>] [--exclude <GLOB>] [--json]
effigy scan generated-in-src [--threshold <BYTES>] [--high <BYTES>] [--critical <BYTES>] [--source-root <GLOB>] [--show-warnings] [--markdown] [--out <PATH>] [--fail-on-findings] [--no-gitignore] [--include <GLOB>] [--exclude <GLOB>] [--json]
effigy scan attention-markers [--show-warnings] [--markdown] [--out <PATH>] [--fail-on-findings] [--no-gitignore] [--include <GLOB>] [--exclude <GLOB>] [--json]
effigy scan stale-suppressions [--show-warnings] [--warning-marker <VALUE>] [--high-marker <VALUE>] [--critical-marker <VALUE>] [--markdown] [--out <PATH>] [--fail-on-findings] [--no-gitignore] [--include <GLOB>] [--exclude <GLOB>] [--json]
effigy test [--plan] [--verbose-results] [--tui] [suite] [runner args]
effigy watch --owner <effigy|external> [--debounce-ms <MS>] [--include <GLOB>] [--exclude <GLOB>] <task> [task args]
effigy watch --owner effigy --once <task> [task args]
effigy init [--dry-run] [--force] [--json]
effigy migrate [--from <PATH>] [--script <NAME>]... [--apply] [--json]
effigy config [--inspect] [--path <dotted.path>] [--json]
effigy config [--schema] [--minimal] [--target <manifest|section>] [--runner <runner>] [--json]
effigy unlock [--all | <scope>...] [--json]
effigy cache inspect [<selector>] [--json]
effigy cache invalidate [<selector>...] [--all] [--json]
effigy completion <bash|zsh|fish> [--json]
effigy completion candidates [--repo <PATH>] [--prefix <value>] [--json]
effigy changelog validate [FILE] [--json]
effigy changelog format [FILE] [--write|--preview]
effigy changelog analyze [FILE] [--json]
effigy changelog extract [FILE] --version <VERSION>
effigy release status [--repo <PATH>] [--check-gates] [--json]
effigy release gates [--repo <PATH>] [--json]
effigy release resume [--repo <PATH>] [--allow-stale] [--json]
effigy release verify-install [--repo <PATH>] [--tag <TAG>] [--repo-url <URL>] [--json]
effigy release simulate [--repo <PATH>] [--version <SEMVER>] [--json]
effigy release prepare [--repo <PATH>] [--check-gates]
effigy release prepare (--plan|--dry-run) [--repo <PATH>] [--check-gates] [--version <SEMVER>] [--json]
effigy release prepare --yes [--repo <PATH>] [--check-gates] [--version <SEMVER>] [--json]
effigy release execute [--repo <PATH>] [--allow-stale]
effigy release execute (--plan|--dry-run) [--repo <PATH>] [--allow-stale] [--json]
effigy release execute --yes [--repo <PATH>] [--allow-stale] [--json]
```

## 4) Scope Notes and Constraints

- `tasks --pretty false` is valid only with `--json`.
- `watch --json` requires bounded mode (`--once` or `--max-runs`).
- `watch --owner` is required; `external` owner blocks nested watch loops.
- `bootstrap` is stateless by default: destination is cwd-relative unless `--path`
  is supplied.
- `bootstrap` runs `start` only when `--start` is supplied.
- `bootstrap` fails fast on dirty existing checkouts or remote mismatches.
- `demo inspect` reads declared or generated receipt/artifact references and
  normalizes the latest known proof state and any active in-flight attempt
  without executing the demo.
- `demo run` executes either a declared task-backed or run-backed entrypoint,
  writes a normalized receipt, and refreshes the latest-attempt state that
  `demo inspect` reports.
- `demo stop` only works for demos whose active attempt is directly owned by
  the runner; task-backed demos still report an explicit unstoppability
  boundary.
- `demo rerun` starts a fresh attempt and fails if the demo already has an
  active attempt.
- task execution locks on `task:<name>` by default; use `tasks.<name>.lock = "<shared-name>"` to opt multiple tasks into the same `shared:<name>` scope.
- managed `mode = "tui"` tasks also acquire `profile:<task>/<profile>` in addition to the task or shared scope.
- managed `concurrent` entries accept `shutdown_on_exit = true` when one
  process should terminate the whole managed session after it exits.
- `scan god-files` accepts either `--json` or `--markdown`, not both.
- `scan god-files --out <PATH>` resolves relative paths from the scanned repo root.
- `scan god-files` hides warning rows in terminal text output unless `--show-warnings` is set.
- `scan.god_files` config can set defaults for thresholds, output format/path, traversal globs, and doctor participation.
- `scan duplicate-blocks` accepts either `--json` or `--markdown`, not both.
- `scan duplicate-blocks --out <PATH>` resolves relative paths from the scanned repo root.
- `scan duplicate-blocks` hides warning rows in terminal text output unless `--show-warnings` is set.
- `scan.duplicate_blocks` config can set defaults for thresholds, minimum occurrence count, output format/path, traversal globs, and doctor participation.
- `scan.duplicate_blocks` remains doctor-opt-in by default; the current `acowtancy` benchmark is useful but too expensive/noisy for default doctor runs.
- `scan comment-ratio` accepts either `--json` or `--markdown`, not both.
- `scan comment-ratio --out <PATH>` resolves relative paths from the scanned repo root.
- `scan comment-ratio` hides warning rows in terminal text output unless `--show-warnings` is set.
- `scan.comment_ratio` config can set defaults for ratio thresholds, minimum code lines, output format/path, traversal globs, and doctor participation.
- `scan.comment_ratio` now defaults to doctor participation; the current `acowtancy` benchmark took about `2.4s` and produced `15` findings, which is acceptable for default health runs.
- `scan generated-assets` accepts either `--json` or `--markdown`, not both.
- `scan generated-assets --out <PATH>` resolves relative paths from the scanned repo root.
- `scan generated-assets` hides warning rows in terminal text output unless `--show-warnings` is set.
- `scan.generated_assets` config can set defaults for byte thresholds, output format/path, and traversal globs.
- `scan generated-in-src` accepts either `--json` or `--markdown`, not both.
- `scan generated-in-src --out <PATH>` resolves relative paths from the scanned repo root.
- `scan generated-in-src` hides warning rows in terminal text output unless `--show-warnings` is set.
- `scan.generated_in_src` config can set defaults for byte thresholds, source-root globs, output format/path, and doctor participation.
- `scan.generated_in_src` now defaults to doctor participation; the current `acowtancy` benchmark took about `2.1s` and produced `4` warning-level findings, which is acceptable for default health runs.
- `scan attention-markers` accepts either `--json` or `--markdown`, not both.
- `scan attention-markers --out <PATH>` resolves relative paths from the scanned repo root.
- `scan attention-markers` hides warning rows in terminal text output unless `--show-warnings` is set.
- `scan attention-markers` does not accept threshold flags; marker families come from defaults or `[scan.attention_markers]`.
- `scan.attention_markers` config can set defaults for marker families, output format/path, traversal globs, and doctor participation.
- `scan stale-suppressions` accepts either `--json` or `--markdown`, not both.
- `scan stale-suppressions --out <PATH>` resolves relative paths from the scanned repo root.
- `scan stale-suppressions` hides warning rows in terminal text output unless `--show-warnings` is set.
- `scan stale-suppressions` does not accept threshold flags; suppression families come from defaults or `[scan.stale_suppressions]`.
- `scan.stale_suppressions` config can set defaults for marker families, output format/path, traversal globs, and doctor participation.
- `config --minimal` requires `--schema`.
- `config --inspect` cannot be combined with `--schema`.
- `config --path` requires `--inspect`.
- `config --runner` requires `--schema --target test`.
- `config --inspect` is the native way to inspect include order, override
  results, and effective value sources for `[manifest].include`.
- `config --inspect --path <dotted.path>` narrows that view to one effective
  value, its source file, and any matching override history.
- `unlock` accepts either explicit scopes or `--all` (not both).
- `cache` phase-1 works only for tasks with explicit `[tasks.<name>.cache]` opt-in.
- `cache invalidate` accepts selectors or `--all` (not both).
- release operator flows should prefer the built-in `effigy release ...`
  surface; legacy wrapper scripts are backup channels documented in guide `049`
  rather than the primary manual interface.
- task-local runtime env is supported with `env = { KEY = "value" }` under task definitions (full table or compact inline table).
- run arrays also support env directives (`{ env = { ... } }` or `{ env = "<profile>" }`) to update env for later entries in the chain.
- run-array env directives also support cross-catalog indirection via `env = "<catalog-path>/<name>"` (path relative to current catalog unless absolute).
- top-level `[env]` defines reusable named entries for run-array indirection; entries can be direct values (`NAME = "value"`) or grouped profile arrays (`name = [{ KEY = "value" }, ...]`).
- when a named env entry is not defined in `[env]`, effigy falls back to process env and `.env` lookup for that key.
- task-level `env_file` and run-step `{ env_file = ... }` can override dotenv fallback source (`.env` by default); supports string or ordered array where first file containing a key wins.
- run-step `env`/`env_file` directives can be standalone no-op state updates (no `run` or `task` key required).
- for cross-catalog `env = "<catalog-path>/<name>"`, dotenv fallback uses that target catalog root (including `env_file` overrides) and does not check process env.
- `tasks.<name>.env` values support `{project}` and `{repo}` catalog-root token substitution.
- built-in `test` automatically applies manifest `[env]` `CARGO_*` values to cargo suites (`cargo-nextest` and `cargo-test`), including grouped profile entries.
- when manifest `[env]` does not define `CARGO_HOME` or `CARGO_TARGET_DIR`, built-in `test` falls back per target root: process env first, then `<target-root>/.env`.
- built-in cargo-env matching mode is configured via `[test].cargo_env_match` (`executable-only`, `prefix-aware`, `shell-aware`).
- `effigy test --plan` target output (text and JSON) includes effective `cargo_env_match` per target.
- `effigy test --verbose-results` text output and `effigy.test.results.v1` targets include effective `cargo_env_match` per target.
- built-in cargo-env auto-apply matching accepts optional `env`/`exec`/`command` wrappers, leading `KEY=value` assignments, and path-qualified cargo binaries.
- shell-wrapped commands such as `sh -lc "cargo test --workspace"` are matched only when `[test].cargo_env_match = "shell-aware"`.
- `completion` command list is sourced from the built-in command index (`BUILTIN_TASKS`) to reduce drift with command discovery output.
- `completion candidates` includes built-ins plus discovered `<task>` and `<catalog>/<task>` selectors.
- `completion candidates` JSON payload reports `cache_hit`, `cache_state`, `cache_age_ms` (on hit), `cache_ttl_ms` (on hit), `effective_cache_ttl_ms`, `cache_ttl_source`, and `manifest_count` for memoized candidate scans.
- `cache_state` values: `miss_initial`, `hit`, `miss_ttl`, `miss_manifest_change`.
- `miss_manifest_change` is triggered from manifest stamp drift (mtime/size/content digest), so cache invalidation is not dependent on timestamp granularity alone.
- Completion candidates cache TTL can be tuned with `EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS` (bounded to `100..60000`, default `2000`).
- `cache_ttl_source` values: `default`, `env`, `env_invalid` (invalid env values fall back to default TTL).

## 5) Common Recipes

Routing diagnosis:

```sh
effigy tasks --resolve test
effigy doctor --repo /path/to/workspace app/build -- --watch
```

Test planning and execution:

```sh
effigy test --plan
effigy test vitest
```

CI/JSON mode:

```sh
effigy --json tasks
effigy --json doctor
effigy --json scan god-files
effigy --json scan duplicate-blocks
effigy --json scan comment-ratio
effigy --json scan generated-in-src
effigy --json scan attention-markers
effigy --json scan stale-suppressions
effigy --json bootstrap git@github.com:inflatable-cookie/loophole.git --plan
effigy --json test --plan
effigy release simulate
effigy release prepare --plan
effigy release execute --plan
effigy --json release status --check-gates
```

Lock recovery:

```sh
effigy unlock task:watch:test
effigy unlock --all
```

## Related Guides

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`057-bootstrap-repo-bringup.md`](./057-bootstrap-repo-bringup.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`051-release-orchestration.md`](./051-release-orchestration.md)
- [`052-changelog-workflows-and-northstar-profile.md`](./052-changelog-workflows-and-northstar-profile.md)
- [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)

## Expected Outcome

After this guide, you should be able to:

- choose the right Effigy command without scanning multiple docs first
- confirm the key flags and JSON schema for a command quickly
- jump from the reference surface to the deeper workflow page only when needed

## Next Step

After using this matrix to find the right command, move to the matching workflow
guide and simplify the corresponding repo path so people can rely on that
command directly instead of a local wrapper or tribal workaround.
