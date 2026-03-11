# 025 - Command Reference Matrix

This matrix is a quick operator reference for Effigy commands, key flags, JSON payload schemas, and deep-dive guides.


## Vision Alignment

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`
- Target movement: command lookup stays fast while linking every surface to stable schemas and deep-dive guidance.

## 1) Primary Commands

| Command | Purpose | Key Flags | JSON Schema(s) | Deep Dive |
| --- | --- | --- | --- | --- |
| `effigy help` / `effigy --help` | Show CLI help and topic guidance | `--json` | `effigy.help.v1` (inside command envelope) | `021-quick-start-and-command-cookbook.md` |
| `effigy tasks` | List discovered catalogs/tasks and probe routing | `--repo`, `--task`, `--resolve`, `--json`, `--pretty true\|false` | `effigy.tasks.v1`, `effigy.tasks.filtered.v1` | `016-task-routing-precedence.md` |
| `effigy doctor` | Run health checks and optional explain-mode selection diagnostics | `--repo`, `--fix`, `--verbose`, `--json` | `effigy.doctor.v1`, `effigy.doctor.explain.v1` | `018-doctor-explain-mode.md` |
| `effigy scan` | Run built-in repo scanners such as oversized code-file detection, duplicate-block detection, comment-ratio detection, bulky generated-asset detection, generated-in-src detection, attention-marker detection, and stale-suppression detection | `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-assets`, `generated-in-src`, `attention-markers`, `stale-suppressions`, `--json`, `--markdown`, `--out`, `--fail-on-findings`, `--show-warnings` | `effigy.scan.god-files.v1`, `effigy.scan.duplicate-blocks.v1`, `effigy.scan.comment-ratio.v1`, `effigy.scan.generated-assets.v1`, `effigy.scan.generated-in-src.v1`, `effigy.scan.attention-markers.v1`, `effigy.scan.stale-suppressions.v1` | `022-manifest-cookbook.md` |
| `effigy test` | Run built-in or explicit `tasks.test` test orchestration | `--plan`, `--verbose-results`, `--tui`, `--json` | `effigy.test.plan.v1`, `effigy.test.results.v1` | `013-testing-orchestration.md` |
| `effigy watch` | Policy-first file-triggered reruns for a target task | `--owner`, `--debounce-ms`, `--include`, `--exclude`, `--once`, `--max-runs`, `--json` | `effigy.watch.v1` (bounded JSON runs) | `019-watch-init-migrate-foundation.md` |
| `effigy init` | Scaffold baseline `effigy.toml` | `--dry-run`, `--force`, `--json` | `effigy.init.v1` | `019-watch-init-migrate-foundation.md` |
| `effigy migrate` | Import `package.json` scripts into `[tasks]` | `--from`, `--script`, `--apply`, `--json` | `effigy.migrate.v1` | `019-watch-init-migrate-foundation.md` |
| `effigy config` | Render config reference or schema snippets | `--schema`, `--minimal`, `--target`, `--runner`, `--json` | `effigy.config.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy unlock` | Clear lock scopes manually | `--all`, `--json` | `effigy.unlock.v1` | `020-dag-lock-policy-baseline.md` |
| `effigy cache` | Inspect and invalidate phase-1 cache metadata | `inspect`, `invalidate`, `--all`, `--json` | `effigy.cache.v1` | `022-manifest-cookbook.md` |
| `effigy completion` | Generate shell completion scripts and selector candidates | `bash\|zsh\|fish`, `candidates`, `--repo`, `--prefix`, `--json` | `effigy.completion.v1`, `effigy.completion.candidates.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy changelog` | Validate, format, analyze, and extract Northstar changelog content | `validate`, `format`, `analyze`, `extract`, `--write`, `--preview`, `--version`, `--json` | changelog subcommands render direct output; some results can be wrapped in `effigy.command.v1` with global JSON mode | `036-release-notes-authoring-template-and-examples.md` |
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
effigy config [--schema] [--minimal] [--target <section>] [--runner <runner>] [--json]
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
- `config --runner` requires `--schema --target test`.
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
effigy --json test --plan
effigy release simulate --repo .
effigy release prepare --repo . --plan
effigy release execute --repo . --plan
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
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`051-release-orchestration.md`](./051-release-orchestration.md)
- [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)
