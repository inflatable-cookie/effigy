# 022 - Manifest Cookbook (`effigy.toml` Patterns)

This cookbook provides copy-paste manifest patterns you can adapt directly.

Use it when the next improvement should happen in `effigy.toml` instead of in
another wrapper script, shell note, or team-specific convention.

If you want the narrative front doors first, start with:

- [`058-demo-system-guide.md`](./058-demo-system-guide.md) for the demo system
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
  for `[manifest].include` and fragment layout
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)
  for the composed `system` / `workspace` runtime model
- [`061-rhai-script-steps-guide.md`](./061-rhai-script-steps-guide.md) for
  Effigy-native scripting in Rust-first repos


## Vision Alignment

- Primary tags: `ROUTE`, `MAINT`
- Target movement: manifest patterns encode maintainable routing and execution conventions with minimal ambiguity.

## Start Here

Pick the first pattern by the friction you want to remove:

- Need one clean starting point: use `Small Single-Repo Starter`.
- Need a few simple commands and task chains: use `Compact Tasks + Task Chain`.
- Need env/test/workflow behavior to be explicit: jump to `Run-Array Env
  Directives` and `Built-in Test Fanout`.
- Need repo-health checks without more custom tooling: jump to the `scan.*`
  patterns.

Useful companion commands while editing:

```sh
effigy init
effigy config --schema --minimal
effigy config --inspect
effigy tasks
effigy test --plan
```

## How To Use This Cookbook

- Start with the smallest pattern that removes real friction.
- Prefer one clear task name over multiple aliases for the same action.
- Move repeated env/test/setup rules into the manifest once, not into multiple
  task wrappers.
- When the manifest starts to explain the repo better than a README snippet,
  you are moving in the right direction.

## 1) Small Single-Repo Starter

```toml
[catalog]
alias = "app"

[tasks]
fmt = "cargo fmt --all"
lint = "cargo clippy --all-targets --all-features -- -D warnings"
check = [{ task = "fmt" }, { task = "lint" }]
```

Use when you want one local catalog with explicit command ownership while
leaving `effigy test` available as the built-in test entrypoint.

## 2) Compact Tasks + Task Chain

```toml
[tasks]
api = "cargo run -p api {args}"
worker = "cargo run -p worker {args}"
"db:reset" = [{ task = "db:drop" }, { task = "db:migrate" }]
"db:drop" = "sqlx database drop -y"
"db:migrate" = "sqlx migrate run"
```

Use compact syntax for straightforward run commands and lightweight chains.

## 2b) Split One Manifest Into Focused Fragments

```toml
[manifest]
include = [
  "effigy.tasks.toml",
  "effigy.docs.toml",
  { path = "effigy.local.toml", override = ["tasks.dev", "release.sync-files"] },
]
```

`effigy.tasks.toml`

```toml
[tasks]
dev = "bun run dev"
build = "bun run build"
```

`effigy.docs.toml`

```toml
[docs_policy.indexes.vision]
file = "docs/vision/README.md"
dir = "docs/vision"
section = "Vision Artifacts"
```

Use this when one `effigy.toml` has become a dumping ground and the better fix
is separation by concern instead of more comments or wrapper scripts.

Composition rules:
- `effigy.toml` stays the canonical entrypoint.
- included paths are resolved relative to the file that declares them.
- fragments are partial manifests, not standalone alternate roots.
- distinct keys merge additively by default.
- conflicting values fail unless the include entry explicitly names the path in
  `override = [...]`.
- override is path-scoped and replaces the whole addressed value.

Inspection:

```sh
effigy config --inspect
effigy config --inspect --path tasks.dev
```

Use this to confirm include order, overridden paths, effective sources, and the
final merged manifest before treating composition as settled.

Use `--path <dotted.path>` when the full composed view is too broad and the real
question is “where did this one effective value come from?” or “which override
replaced it?”

## 2c) Demo Registry Foundation

```toml
[demos.login-smoke]
title = "Login smoke"
summary = "Checks that the login flow still produces a working session."
proof = "Operator-visible smoke proof for login."
owner = "platform"
mode = "interactive"
status = "ready"
covers = ["auth.login"]
tags = ["smoke", "ui"]
task = "demo:login"
receipt = "demos/receipts/login-smoke.json"
artifacts = ["demos/artifacts/login-smoke/index.html"]
prerequisites = ["api.dev", "ui.dev"]
dependencies = ["seed.auth"]
```

Use this when proof/demo work should become first-class repo contract instead
of another flat script directory.

Registry rules:
- each demo lives at `[demos.<id>]`
- `title`, `summary`, `proof`, `owner`, `status`, `mode`, and `covers` are required
- declare exactly one runnable entrypoint with `task = "..."` or `run = "..."`
- demo `run` also accepts the same run-step array shape as task `run`, so small
  proof chains can live directly in `[demos.*]` without an extra `demo:*` task
- `receipt` and `artifacts` are optional, but they let `effigy demo inspect`
  show the latest known proof state instead of only static metadata
- if `receipt` is omitted, `effigy demo run` writes a normalized receipt to
  `.effigy/demo/receipts/<demo-id>.json`

Discovery, inspection, and execution:

```sh
effigy demo list
effigy demo browser
effigy demo list --owner platform --status ready
effigy demo list --group-by owner --stale-only
effigy demo inspect login-smoke
effigy demo history login-smoke --limit 5
effigy demo history login-smoke --attempt login-smoke-1775944053944
effigy demo run login-smoke
effigy demo stop login-smoke
effigy demo rerun login-smoke
effigy --json demo inspect login-smoke
```

Use this when you need a stable proof inventory plus one operator-visible proof
entrypoint now.

The Effigy repo now self-hosts this surface with two concrete demos:
- `browser-proof-report` is task-backed and generates a human-checkable HTML
  artifact plus browser query snapshots
- `lifecycle-window` is run-backed and stays active until `effigy demo stop
  lifecycle-window` is used

Lifecycle notes:
- `demo list` now supports focused browser-style discovery with `--search`,
  `--owner`, `--tag`, `--mode`, `--cover`, `--status`, `--gap`,
  `--stale-only`, and `--group-by`.
- `demo browser` now provides the first live list/detail TUI on top of the
  shipped registry, query, inspect, run, stop, and rerun surfaces.
- inside `demo browser`, `Tab`/`Shift+Tab` switches focus between the demo list
  and the detail pane; when the detail pane is focused, `←`/`→` switches
  between `Overview`, `History`, `Terminal`, and `Artifacts`; `↑`/`↓` acts
  inside the focused pane or active tab; `Enter` opens the action sheet from
  the list, activates the selected detail-side entry, or toggles terminal input
  capture on the `Terminal` tab when supported; `Esc` closes overlays, leaves
  terminal input capture, returns non-overview tabs to `Overview`, or quits
  from the root overview.
- inside `demo browser`, use `/` for direct search and `f` for the filter
  sheet, which now owns owner/tag/mode/cover/status/gap/stale/grouping
  controls.
- the detail side is now tabbed and intentionally short rather than one long
  scrolling receipt/log document.
- `demo inspect` now shows both the latest terminal receipt and any active
  in-flight attempt, plus explicit action availability, receipt freshness, and
  a bounded recent-attempt history for older terminal outcomes.
- `demo inspect` now also exposes a runner-owned active terminal/session
  handoff for active demos, including stream-vs-pty transport metadata,
  explicit input-forwarding capability signaling, recent stdout/stderr
  snapshots, and an explicit `nested_tui = false` boundary for browser
  consumers.
- `demo history <id>` gives that retained result history a dedicated query
  surface, with optional `--limit <N>` trimming, so operators can inspect one
  demo's recent terminal outcomes without overloading the broader inspect view.
- `demo history <id> --attempt <ATTEMPT_ID>` drills into one retained
  historical attempt so the prior receipt, artifacts, and log references are
  visible without widening `demo list` or the browser.
- `demo stop <id>` works for directly runner-owned run-backed demos.
- `demo rerun <id>` starts a fresh attempt and fails fast if the demo is
  already active.
- task-backed demos remain runnable, but stop is still an explicit
  `not supported through the current runtime` boundary.
- the browser currently focuses on honest navigation, bounded action dispatch,
  artifact-opening, metadata-query parity, integrated retained-history review,
  and a live terminal tab for the bounded honest runtime cases. It still must
  not launch nested TUIs.

Migration note:
- if a consumer repo already has a `demos/` script pack or one wrapper task per
  demo, use
  [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
  for the practical extraction and inline-`run = [ ... ]` path instead of
  expanding this cookbook section further.

## 3) Full Task Table with Runtime Controls

```toml
[tasks.build]
run = "bun run build"
run_in = "either"
fail_on_non_zero = true
```

Use full task tables when you need settings (`run_in`, `fail_on_non_zero`, `env`, `mode`, `profiles`, etc.).
`run_in` accepts `host`, `container`, or `either`; `either` is the default and means use the current/default execution context.

If a whole catalog should share the same task routing default, set it once at
the manifest level and only override the exceptions:

```toml
[task_defaults]
run_in = "either"

[tasks]
qa = [{ task = "docs check-links README.md" }]

[tasks.dev]
run_in = "container"
run = "bun run dev"
```

`[task_defaults]` only applies to tasks defined in that manifest file. Task-level
`run_in` still wins when both are present.

## 4) DAG-Style Validation Flow

```toml
[tasks.validate]
run = [
  { id = "lint", run = "bun run lint", retry = 1, retry_delay_ms = 200 },
  { id = "unit", task = "test vitest", depends_on = ["lint"], timeout_ms = 180000 },
  { id = "contract", run = "cargo run --bin effigy -- contracts check-json --fast --print-selected", depends_on = ["lint"] },
  { id = "report", run = "printf validate-ok", depends_on = ["unit", "contract"], fail_fast = false }
]
```

Use when you need dependency-aware orchestration, retry policy, and per-step timeouts.

## 4a) Rhai Script Step For Rust-First Glue

```toml
[tasks.link:local]
run = [{ rhai = "scripts/rhai/install-local-bin-links.rhai" }]
```

File-backed Rhai:

```rhai
let payload = #{ "task": task_name, "repo_root": repo_root };
write_file("tmp/proof.json", json_stringify(payload));
log("wrote tmp/proof.json");
```

Another file-backed example:

```toml
[tasks.report:stamp]
run = [{ rhai = "scripts/rhai/report-stamp.rhai" }]
```

Use this when:

- the repo is Rust-first
- the script is orchestration or repo glue
- adding Bun or more shell wrapper surface would be noise

Do not use this to fake shell pipelines or replace frontend build tooling.

For the full host API and v1 limits, use
[`061-rhai-script-steps-guide.md`](./061-rhai-script-steps-guide.md).

## 4b) Run-Array Env Directives

```toml
[env]
CARGO_HOME = "{project}/.effigy/cargo/home"
CARGO_TARGET_DIR = "{project}/.effigy/cargo/target"
# Optional grouped profile form:
cargo = [{ CARGO_HOME = "{project}/.effigy/cargo/home" }, { CARGO_TARGET_DIR = "{project}/.effigy/cargo/target" }]

[tasks.api]
env_file = [".env.local", ".env.test"]
run = [
  { env = "cargo" },
  { env = "DATABASE_URL" },
  { run = "cargo run -p api {args}" },
  { env = { RUST_LOG = "debug" } },
  { task = "jobs" }
]
```

Use this when you want env changes to take effect at specific points in a run chain.

Behavior:
- an `env` step updates the effective env for subsequent run-array entries
- an `env_file` step updates dotenv fallback sources for subsequent run-array entries
- `env = "<name>"` resolves in order: top-level `[env]`, process env, then `<catalog-root>/.env`
- `env = "<catalog-path>/<name>"` resolves `<name>` from another catalog `[env]` table, then that catalog `.env` (relative to current catalog root unless absolute)
- cross-catalog refs (`env = "<catalog-path>/<name>"`) do not use process env fallback
- `[env].<name>` can be either a single value (`KEY = "value"`) or a grouped profile array (`name = [{ KEY = "value" }, ...]`)
- `tasks.<name>.env_file` sets fallback dotenv for that task; accepts string or ordered array (`[".env.local", ".env.test"]`)
- run arrays can update fallback dotenv mid-chain with `{ env_file = ".env.test" }` or `{ env_file = [".env.local", ".env.test"] }`
- for array form, files are checked in order and first file containing the key wins
- `env` and `env_file` steps can be mixed with `run` and `task` steps
- `env`/`env_file` directives can be standalone entries with no `run`/`task` command (no-op step used for state changes)
- `tasks.<name>.env` still applies globally to the whole task; run-array `env` steps can override later entries
- `{project}`/`{repo}` in env values always resolve from the task currently executing
- `.env` parsing accepts `KEY=value` and `export KEY=value`; matching single/double quotes are stripped from values

## 5) Managed Dev Stack (`mode = "tui"`)

```toml
[tasks.dev]
mode = "tui"
lock = "dev-stack"
fail_on_non_zero = true
run_in = "either"               # host, container, or either/current context
# optional managed bindings (flattened, no nested subtables):
system = "dev"                   # pin this managed task to a declared system
workspace = "main"               # pin to a specific workspace inside that system
container_lifecycle = true       # ensure the declared system/containers come up
gateway = true                   # auto-start the host gateway before the first tab
health_wait = true               # wait for declared service health gates before ready
ready_message = "dev up"         # line emitted once the stack is ready to use

concurrent = [
  { role = "lifecycle", task = "app/api", start = 1, tab = 2 },
  { task = "app/worker", start = 2, tab = 3, start_after_ms = 1200 },
  { run = "bun run docs:dev", start = 3, tab = 1, shutdown_on_exit = true },
  { role = "shell", service = "workspace", start = 4, tab = 4, setup = [
    { run = "bun install" }
  ] }
]

[tasks.dev.profiles.admin]
concurrent = [
  { task = "app/api", start = 1, tab = 2 },
  { run = "bun run admin:dev", start = 2, tab = 1 }
]
```

Use for multi-process local development with profile-specific variants.

Lifecycle controls:
- `fail_on_non_zero = true` keeps non-zero exits as task failures.
- `shutdown_on_exit = true` on a `concurrent` entry tells Effigy to stop the
  whole managed session when that process exits, even if it exits `0`.
- Use `shutdown_on_exit` when one process is the natural root of the session,
  such as an Electron window, desktop shell, or primary app process.
- `system` / `workspace` bind the managed task to a declared substrate from
  `[systems.<name>]`; without them the task runs on the host.
- `container_lifecycle = true` ensures the bound system and its declared
  services are up before the first tab starts, and participates in teardown on
  exit.
- `gateway = true` starts (or confirms) the host-native gateway before managed
  tabs start, so container-owned DNS and TLS routes resolve from tab one.
- `health_wait = true` blocks the ready signal until declared service health
  gates pass (see `[systems.<name>.services.<svc>.health]`).
- `ready_message = "..."` is surfaced by the managed session once the ready
  signal fires; keep it short and operator-visible.

Concurrent-entry field reference:
- `role = "lifecycle"` marks an entry whose exit status participates in the
  managed session's overall lifecycle (non-zero exit fails the session).
- `role = "shell"` opens an interactive workspace shell in that tab; pair with
  `service = "<service-name>"` to select the workspace service (usually
  `workspace` for the rust+bun developer surface).
- `setup = [{ run = "..." } | { task = "..." } | { rhai = "..." }]` runs
  ordered pre-steps inside that pane before the main `run`/`task` starts.
- `name = "..."` / `label = "..."` set the machine-stable identifier and the
  TUI-visible label; label defaults from `name` or the underlying task id.

Electron-style example:

```toml
[tasks.desktop]
mode = "tui"

concurrent = [
  { run = "bun run electron:main", start = 1, tab = 2, shutdown_on_exit = true },
  { run = "bun run vite", start = 2, tab = 1 },
  { task = "shell", start = 3, tab = 3 }
]
```

Use this when closing the Electron app window should tear down the supporting
web/dev processes automatically instead of leaving them running in the
background.

Lock behavior:
- tasks lock on `task:<name>` by default, so unrelated tasks can run concurrently in the same repo
- set `tasks.<name>.lock = "<shared-name>"` when multiple tasks should serialize together
- managed TUI tasks still add `profile:<task>/<profile>` so profile-specific runs stay isolated
- recover specific collisions with `effigy unlock task:<name>`, `effigy unlock shared:<name>`, or `effigy unlock profile:<task>/<profile>`

## 6) Built-in Test Fanout and Suite Source of Truth

```toml
[package_manager]
js = "bun"

[test]
max_parallel = 2
cargo_env_match = "prefix-aware"

[test.suites]
unit = "bun x vitest run"
integration = "cargo nextest run --workspace"

[test.runners]
vitest = "bun x vitest run"
"cargo-nextest" = "cargo nextest run --workspace"
"cargo-test" = "cargo test --workspace"
```

Use this to make test routing explicit and reproducible across mixed stacks.

When a suite needs env/setup/teardown semantics, use a full suite table instead of a plain string:

```toml
[test.suites.managed]
run = "cargo nextest run --workspace --test-threads=1 --build-jobs=1"
env = "TEST_DATABASE_URL"
env_file = ".env"
setup = [
  { run = "cargo run -p app-db --bin reset_test_db" },
  { run = "cargo run -p app-db --bin migrate_test_db" },
]
teardown = [
  { run = "cargo run -p app-db --bin reset_test_db" },
]
teardown_policy = "always"
```

Use this to replace custom wrapper scripts while keeping `effigy test` as the only operator entrypoint.

## 7) Minimal Test Runner Override Only

```toml
[test.runners]
vitest = "bun x vitest run"
"cargo-nextest" = { command = "cargo nextest run --workspace --status-level skip" }
```

Use when auto-detection is fine but default commands need tuning.

## 8) Deferral Fallback for Legacy Interop

```toml
[defer]
run = "my-process {request} {args}"
builtins = ["release"]
```

Use only when unresolved selectors should be delegated to another runner.

Notes:
- unresolved selectors still defer through `run = ...` as before
- `builtins = ["release"]` is the explicit escape hatch for legacy repos where a parser-level Effigy built-in would otherwise shadow the old command family
- explicitly deferred built-ins disappear from general help and from the built-in section in `effigy tasks`
- pure PHP-legacy repos using the automatic `composer.json` + `effigy.json` fallback already defer `release` by default, so you only need `builtins = [...]` when you are bypassing additional built-ins or overriding that implicit mode with explicit `[defer]`

## 8b) Bootstrap Repo Bring-Up

```toml
[bootstrap]
setup = ["bootstrap:local", "doctor"]
start = "dev"
submodules = "recursive"

[[bootstrap.children]]
path = "aura"
repo = "git@github.com:inflatable-cookie/aura.git"
branch = "main"
setup = ["install"]
required = true

[tasks."bootstrap:local"]
run = "bun install"
```

Use this when the repo should be able to describe its own first-run bring-up
path after `effigy bootstrap <git-url>`.

Behavior:
- root setup tasks run in the cloned or updated root repo
- child setup tasks run inside each child repo
- `start` only runs when the operator supplies `--start`
- child `path` values are always relative to the root repo
- optional children (`required = false`) degrade to warnings instead of failing
  the whole bootstrap
- existing dirty or mismatched checkouts fail fast instead of being silently
  repurposed

## 9) Shell Override for Managed Tabs

```toml
[shell]
run = "exec ${SHELL:-/bin/zsh} -i"
```

Use when you need predictable interactive shell startup behavior in TUI shell tabs.

## 10) Explicit Task Cache (Phase 1)

```toml
[tasks.build]
run = "cargo build --workspace"

[tasks.build.cache]
enabled = true
inputs = ["src/**/*.rs", "Cargo.toml", "Cargo.lock"]
outputs = ["target/debug/my-app"]
env = ["RUSTFLAGS", "CARGO_PROFILE_DEV_DEBUG"]
```

Use this for deterministic local up-to-date checks.

Phase-1 guardrails:
- cache is opt-in only (`enabled = true`)
- no implicit input/output discovery
- cache hit requires matching fingerprint and declared outputs to exist

Inspection and invalidation:
- `effigy cache inspect build`
- `effigy cache invalidate build`
- `effigy cache invalidate --all`

## 11) Built-in God-File Scanner

```toml
[scan.god_files]
warn = 250
high = 400
critical = 700
doctor = true
fail_on_findings = false
respect_gitignore = true
include = ["src/**", "app/**"]
exclude = ["docs/**", "dist/**", "coverage/**"]
format = "markdown"
out = "reports/god-files.md"
```

Use this when you want a repo-level oversized-file check that can also feed `effigy doctor`.

Typical commands:
- `effigy scan god-files`
- `effigy scan god-files --show-warnings`
- `effigy scan god-files --fail-on-findings`
- `effigy --json scan god-files`

Behavior:
- counts code-only lines for common source-file types and falls back to non-blank lines for unknown extensions
- terminal text output hides warning rows by default and prints a warning count summary; `--show-warnings` restores the full list
- skips common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts by default
- respects `.gitignore`/`.ignore` during traversal unless disabled
- `effigy doctor` uses the same scanner core but keeps its own report semantics; warning-level god-file findings still appear there when `doctor = true`
- doctor text output summarizes scan counts and writes file-level details to `.effigy/reports/doctor/scan-god-files.md`
- `doctor = false` keeps the config available for `scan` without surfacing it in `effigy doctor`

## 12) Built-in Generated-Assets Scanner

```toml
[scan.generated_assets]
warn = 1000000
high = 5000000
critical = 20000000
doctor = true
fail_on_findings = false
respect_gitignore = true
include = ["dist/**", "vendor/**", "third_party/**"]
exclude = ["docs/**"]
format = "markdown"
out = "reports/generated-assets.md"
```

Use this when you want a repo-level check for bulky vendored/generated artifacts that slipped into versioned paths.

Typical commands:
- `effigy scan generated-assets`
- `effigy scan generated-assets --show-warnings`
- `effigy scan generated-assets --fail-on-findings`
- `effigy --json scan generated-assets`

Behavior:
- thresholds are measured in bytes
- terminal text output hides warning rows by default and prints a warning count summary; `--show-warnings` restores the full list
- matches vendored/build paths, bundle/minified/source-map names, and generated markers
- respects `.gitignore`/`.ignore` during traversal unless disabled
- `effigy doctor` uses the same scanner core and includes findings when `doctor = true`
- doctor text output summarizes scan counts and writes file-level details to `.effigy/reports/doctor/scan-generated-assets.md`

## 13) Built-in Duplicate-Blocks Scanner

```toml
[scan.duplicate_blocks]
warn = 20
high = 40
critical = 80
min_occurrences = 2
doctor = false
fail_on_findings = false
respect_gitignore = true
include = ["src/**", "crates/**", "tests/**"]
exclude = ["vendor/**"]
format = "markdown"
out = "reports/duplicate-blocks.md"
```

Use this when you want a repo-level scan for large repeated normalized code spans across source files.

Typical commands:
- `effigy scan duplicate-blocks`
- `effigy scan duplicate-blocks --show-warnings`
- `effigy scan duplicate-blocks --fail-on-findings`
- `effigy --json scan duplicate-blocks`

Behavior:
- thresholds are measured in normalized code lines after blank/comment-only line filtering
- findings report merged duplicate spans, occurrence counts, snippets, and location ranges
- terminal text output hides warning rows by default and prints a warning count summary; `--show-warnings` restores the full list
- includes source and test files by default while skipping common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts
- respects `.gitignore`/`.ignore` during traversal unless disabled
- `effigy doctor` can include the same scanner when `doctor = true`, with file-level details written to `.effigy/reports/doctor/scan-duplicate-blocks.md`
- keep `doctor = false` as the default; the current `acowtancy` benchmark takes about `16.9s` and yields enough findings that this is better as an opt-in health check

## 14) Built-in Generated-In-Src Scanner

```toml
[scan.generated_in_src]
warn = 1
high = 20000
critical = 200000
source_roots = ["src/**", "app/**", "lib/**", "crates/**", "packages/*/src/**"]
doctor = true
fail_on_findings = false
respect_gitignore = true
include = ["src/**", "app/**", "lib/**"]
exclude = ["vendor/**"]
format = "markdown"
out = "reports/generated-in-src.md"
```

Use this when you want a repo-level scan for generated files that have landed inside maintained source trees.

Typical commands:
- `effigy scan generated-in-src`
- `effigy scan generated-in-src --show-warnings`
- `effigy scan generated-in-src --source-root src/** --source-root packages/*/src/**`
- `effigy --json scan generated-in-src`

Behavior:
- thresholds are measured in bytes
- target scope is bounded to configured `source_roots`, then generated-file heuristics run within those paths
- terminal text output hides warning rows by default and prints a warning count summary; `--show-warnings` restores the full list
- matches generated markers, generated-style filenames, and minified/source-map artifacts inside source trees
- respects `.gitignore`/`.ignore` during traversal unless disabled
- `effigy doctor` uses the same scanner core and includes findings when `doctor = true`
- doctor text output summarizes scan counts and writes file-level details to `.effigy/reports/doctor/scan-generated-in-src.md`
- keep `doctor = true` as the default; the current `acowtancy` benchmark takes about `2.1s` and yields `4` warning-level findings, which is acceptable for default health runs

## 15) Built-in Attention-Markers Scanner

```toml
[scan.attention_markers]
warning = ["TODO", "REVIEW", "NOTE", "placeholder"]
high = ["FIXME", "HACK", "@deprecated", "workaround"]
critical = ["BUG", "SECURITY", "remove before release"]
doctor = true
fail_on_findings = false
respect_gitignore = true
include = ["src/**", "crates/**", "tests/**"]
exclude = ["vendor/**"]
format = "markdown"
out = "reports/attention-markers.md"
```

Use this when you want a repo-level scan for deferred-work, deprecation, and placeholder markers that can also feed `effigy doctor`.

Typical commands:
- `effigy scan attention-markers`
- `effigy scan attention-markers --show-warnings`
- `effigy scan attention-markers --fail-on-findings`
- `effigy --json scan attention-markers`

Behavior:
- matches explicit marker strings rather than fuzzy prose inference
- terminal text output hides warning rows by default and prints a warning count summary; `--show-warnings` restores the full list
- includes source and test files by default while skipping common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts
- respects `.gitignore`/`.ignore` during traversal unless disabled
- `effigy doctor` uses the same scanner core and includes findings when `doctor = true`
- doctor text output summarizes scan counts and writes file-level details to `.effigy/reports/doctor/scan-attention-markers.md`

## 16) Built-in Comment-Ratio Scanner

```toml
[scan.comment_ratio]
warn = 1.5
high = 2.0
critical = 3.0
min_code_lines = 20
doctor = true
fail_on_findings = false
respect_gitignore = true
include = ["src/**", "crates/**", "tests/**"]
exclude = ["vendor/**"]
format = "markdown"
out = "reports/comment-ratio.md"
```

Use this when you want a repo-level scan for files where comment-only lines materially outweigh executable lines.

Typical commands:
- `effigy scan comment-ratio`
- `effigy scan comment-ratio --show-warnings`
- `effigy scan comment-ratio --fail-on-findings`
- `effigy --json scan comment-ratio`

Behavior:
- thresholds are measured as `comment_lines / code_lines`
- only files with at least `min_code_lines` code-only lines are evaluated
- terminal text output hides warning rows by default and prints a warning count summary; `--show-warnings` restores the full list
- includes source and test files by default while skipping common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts
- respects `.gitignore`/`.ignore` during traversal unless disabled
- `effigy doctor` uses the same scanner core and includes findings when `doctor = true`
- doctor text output summarizes scan counts and writes file-level details to `.effigy/reports/doctor/scan-comment-ratio.md`
- keep `doctor = true` as the default; the current `acowtancy` benchmark takes about `2.4s` and yields `15` findings, which is acceptable for default health runs

## 17) Built-in Stale-Suppressions Scanner

```toml
[scan.stale_suppressions]
warning = ["@ts-ignore", "@ts-expect-error", "type: ignore", "eslint-disable-next-line"]
high = ["#[allow(", "#[expect(", "rubocop:disable", "swiftlint:disable"]
critical = ["nolint", "#[allow(warnings)]", "shellcheck disable=", "eslint-disable"]
doctor = false
fail_on_findings = false
respect_gitignore = true
include = ["src/**", "crates/**", "tests/**"]
exclude = ["vendor/**"]
format = "markdown"
out = "reports/stale-suppressions.md"
```

Use this when you want a repo-level scan for inline suppressions that hide warnings, lint failures, or type errors.

Typical commands:
- `effigy scan stale-suppressions`
- `effigy scan stale-suppressions --show-warnings`
- `effigy scan stale-suppressions --critical-marker "eslint-disable"`
- `effigy --json scan stale-suppressions`

Behavior:
- matches explicit suppression markers rather than trying to prove a suppression is definitely stale
- terminal text output hides warning rows by default and prints a warning count summary; `--show-warnings` restores the full list
- includes source and test files by default while skipping common docs, lockfiles, migrations, fixtures, examples, benchmarks, and generated artifacts
- respects `.gitignore`/`.ignore` during traversal unless disabled
- `effigy doctor` uses the same scanner core and can include findings when `doctor = true`
- doctor text output summarizes scan counts and writes file-level details to `.effigy/reports/doctor/scan-stale-suppressions.md`
- keep `doctor = false` as the default; the current `acowtancy` benchmark takes about `3.9s` and yields `69` findings, which is useful but too noisy for routine doctor runs

## 18) Task-Local Runtime Env (Cargo Isolation)

Compact inline-table shape:

```toml
[env]
CARGO_HOME = "{project}/.effigy/cargo/home"
CARGO_TARGET_DIR = "{project}/.effigy/cargo/target"

[tasks]
build = [{ env = "CARGO_HOME" }, { env = "CARGO_TARGET_DIR" }, { run = "cargo build --workspace" }]
```

Full task-table shape:

```toml
[tasks.build]
run = "cargo build --workspace"
env = { CARGO_HOME = "{project}/.effigy/cargo/home", CARGO_TARGET_DIR = "{project}/.effigy/cargo/target" }
```

Use this when multiple repos build concurrently and you need project-local Cargo state to avoid cross-repo contention.

Behavior:
- process environment is inherited by default
- `tasks.<name>.env` overrides inherited values for that task
- run-array env directives support either inline maps (`env = { ... }`) or named entries (`env = "CARGO_HOME"`/`env = "cargo"` from `[env]`)
- named entry resolution order is `[env]` -> process env -> dotenv fallback (`.env` or `env_file` override)
- referenced tasks keep their own `env` when called via `task = "..."` entries
- run-array `task = "..."` entries can target managed `mode = "tui"` / `concurrent = [...]` tasks; Effigy delegates those through a nested task invocation instead of requiring inline `run = ...`
- env value token substitution supports `{project}` and `{repo}` (aliases for catalog root path)
- built-in `test` also reads manifest `[env]` for `CARGO_*` keys and applies them automatically to cargo suites (`cargo-nextest`/`cargo-test`)
- set `[test].cargo_env_match = "executable-only"` for direct cargo binary token matching only
- set `[test].cargo_env_match = "prefix-aware"` (default) to include wrapper/prefix forms like `env KEY=value cargo ...`
- set `[test].cargo_env_match = "shell-aware"` to include shell-wrapped forms like `sh -lc 'cargo test --workspace'`

## 18b) Flattened `[systems.<name>]` Substrate

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "main"
working_dir = "/workspace"
user = "dev"
home = "/home/dev"
mounts = ["{project}/.effigy/cache:/cache"]

[systems.dev.workspaces.main]
service = "workspace"
working_dir = "/workspace"
user = "dev"

[systems.dev.services.workspace]
image = "ghcr.io/inflatable-cookie/workspace-rust-bun:latest"
# workspace role fields live directly here rather than under a nested managed table

[systems.dev.services.postgres]
catalog = "postgres"

[systems.dev.services.postgres.health]
tcp = 5432
timeout_ms = 30000
```

Use this when one repo should declare a reproducible developer substrate (VM +
compose + gateway + workspace shell) that `effigy system up`, `effigy workspace`,
and managed `mode = "tui"` tasks can all bind to.

Shape rules (current v0.3+ form):
- `[systems.<name>]` is flattened; there is no nested `[systems.<name>.workspace_defaults]`
  or managed subtable — workspace defaults live directly under the system.
- `[systems.<name>.services.<svc>]` accepts either inline service definition
  fields or `catalog = "<catalog-service-name>"` to reuse a shipped catalog
  fragment (e.g. `postgres`, `redis`, `mariadb`, `workspace-rust-bun`).
- `[systems.<name>.services.<svc>.health]` declares container health gates
  (`tcp = <port>`, `http = "<url>"`, `exec = ["..."]`, `timeout_ms = <N>`,
  `interval_ms = <N>`) used by `health_wait = true` and `effigy system status`.
- `[systems.<name>.workspaces.<ws>]` picks which service hosts the interactive
  workspace plus workspace-local overrides; defaults come from the system body.
- generated compose now lives under `.effigy/runtime/compose/`; `infra/dev/` is
  only used for user-owned ejected compose files (`effigy container <name> eject`).

Typical commands:
- `effigy system up` / `effigy system down` / `effigy system status`
- `effigy system logs --follow`
- `effigy system repair` or `effigy system reset-runtime` for recovery
- `effigy workspace` to open the resolved workspace shell after system is up

## 19) Multi-Catalog Monorepo Baseline

Root `effigy.toml`:

```toml
[catalog]
alias = "root"

[tasks]
validate = [{ task = "api/validate" }, { task = "web/validate" }]
```

`services/api/effigy.toml`:

```toml
[catalog]
alias = "api"

[tasks]
dev = "cargo run -p api"
validate = [{ run = "cargo fmt --check" }, { run = "cargo test" }]
```

`apps/web/effigy.toml`:

```toml
[catalog]
alias = "web"

[tasks]
dev = "bun run dev"
validate = [{ run = "bun run lint" }, { run = "bun run test" }]
```

Use catalog aliases to keep task ownership local while retaining root-level orchestration.

## Notes

- Discovery scans for `effigy.toml` recursively.
- Catalog aliases must be unique across discovered manifests.
- Useful interpolation tokens in run commands:
  - `{project}` catalog root path (shell-quoted alias of `{repo}`)
  - `{repo}` catalog root path (shell-quoted)
  - `{args}` passthrough args (shell-quoted)
  - `{request}` unresolved selector (deferral only)
- Useful interpolation tokens in `tasks.<name>.env` values and `[env]` entries:
  - `{project}` catalog root path
  - `{repo}` alias of `{project}`

## Related Guides

- [`013-testing-orchestration.md`](./013-testing-orchestration.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md)
- [`015-deferral-fallback-migration.md`](./015-deferral-fallback-migration.md)
- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- [`019-watch-init-migrate-foundation.md`](./019-watch-init-migrate-foundation.md)
- [`020-dag-lock-policy-baseline.md`](./020-dag-lock-policy-baseline.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`063-container-system-guide.md`](./063-container-system-guide.md)
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)

## Expected Outcome

After this guide, you should be able to:

- choose a manifest pattern that matches the next real repo friction
- move task, env, test, and scan behavior into `effigy.toml` deliberately
- avoid growing new wrapper scripts when the manifest can own the workflow

## Next Step

After adapting one of these patterns, run `effigy tasks` and `effigy test --plan`
to confirm the repo now explains itself more clearly, then use
[`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
or [`025-command-reference-matrix.md`](./025-command-reference-matrix.md) to
close any remaining rough edges in the operator path.
