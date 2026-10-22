# 022 - Manifest Cookbook

This guide is the copy-paste pattern bank for `effigy.toml`.

Use it when the next improvement should happen in the manifest instead of in
another wrapper script, shell note, or repo-local convention.

This is not the best first page if you want narrative explanation first.

Use:
- [`059-manifest-composition-guide.md`](059-manifest-composition-guide.md)
  for fragment layout and `[manifest].include`
- [`064-system-workspace-and-dev-contract.md`](064-system-workspace-and-dev-contract.md)
  for the mental model behind `system`, `workspace`, and `dev`
- [`061-rhai-script-steps-guide.md`](061-rhai-script-steps-guide.md) for
  Effigy-native scripting

## Start Here

Pick the first pattern by the friction you want to remove:

- one clean starting point:
  `Small Single-Repo Starter`
- simple commands, task chains, or file splits:
  `Compact Tasks and Task Chains` and `Split One Manifest Into Focused Fragments`
- local dev systems, workspaces, or mounted sibling repos:
  `Managed Dev Stack`, `Mounted Sibling Isolation`, and `System and Workspace Patterns`
- lightweight Rust-first glue:
  `Rhai Script Step For Rust-First Glue`
- tests, deferral, or bootstrap interop:
  `Testing and Legacy Interop`
- repo-health scans without more custom tooling:
  `Built-in Scans`

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

## Foundation Patterns

### Small Single-Repo Starter

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

### Health, Validate, and QA Cost Ladder

```toml
[tasks]
health = [{ run = "cargo fmt --all -- --check" }]
validate = [{ task = "health" }, { run = "cargo check --workspace" }]
qa = [{ task = "validate" }, { task = "test" }, { task = "qa:docs" }]
```

Keep `health` to seconds-scale orientation checks because `effigy doctor`
delegates to it. `validate` is the mid gate; `qa` is the full board. Never map
`health` directly or transitively to `qa`.

### Compact Tasks and Task Chains

```toml
[tasks]
api = "cargo run -p api {args}"
worker = "cargo run -p worker {args}"
"db:reset" = [{ task = "db:drop" }, { task = "db:migrate" }]
"db:drop" = "sqlx database drop -y"
"db:migrate" = "sqlx migrate run"
```

Use compact syntax for straightforward run commands and lightweight chains.
Single-step task objects can also carry task-level `run_in` when the task needs
an explicit execution context:

```toml
[tasks]
"state:capture-new-content" = { rhai = "scripts/tasks/state-capture-new-content.rhai", run_in = "host" }
```

### Split One Manifest Into Focused Fragments

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
[manifest]
minimum_effigy_version = "0.6.2"

[docs_policy.indexes.topic]
file = "docs/knowledge/README.md"
dir = "docs/knowledge"
section = "Vision Artifacts"
```

Use this when one `effigy.toml` has become a dumping ground and the better fix
is separation by concern instead of more comments or wrapper scripts.

Composition rules:
- `effigy.toml` stays the canonical entrypoint.
- included paths are resolved relative to the file that declares them.
- fragments are partial manifests, not standalone alternate roots.
- any fragment can declare `[manifest].minimum_effigy_version = "X.Y.Z"` to
  fail early on older Effigy binaries before deeper manifest parsing runs.
- repo-local development builds like `v0.6.1+local.<hash>` still pass those
  floors so unreleased feature work is not blocked by the last tagged version.
- distinct keys merge additively by default.
- conflicting values fail unless the include entry explicitly names the path in
  `override = [...]`.
- override is path-scoped and replaces the whole addressed value.

### Mounted Sibling Isolation

Producer repo:

```toml
[isolation]
paths = [
  "node_modules",
  "target",
]
```

Consumer repo:

```toml
[systems.dev]
mounts = ["../platform", "../poodle"]
```

Use this when a mounted sibling repo needs Linux/container-owned install or
build state without clobbering the host copy.

Rules:
- producer repos declare only the paths that are safe and useful to isolate
- mounted consumers normally do **not** repeat those repos under a second
  `isolation = [...]` list; `systems.<name>.mounts` auto-adopts the producer
  contract
- keep the producer list tight, especially on Colima/containerd, because each
  isolated path spends mount budget on the workspace container
- for user-global sibling library trees, pair this with
  [`069-workspace-host-integration.md`](069-workspace-host-integration.md)
  rather than inventing repo-local symlink tricks

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

## Task and Runtime Patterns

### Full Task Table with Runtime Controls

```toml
[tasks.build]
run = "bun run build"
run_in = "either"
fail_on_non_zero = true
```

Use full task tables when you need broad task settings (`fail_on_non_zero`, `env`, `mode`, `profiles`, etc.).
`run_in` accepts `host`, `container`, or `either`; `either` is the default and means use the current/default execution context.
For a single `run`, `task`, or `rhai` step, `run_in` can live in the compact
inline task object instead.

If a whole catalog should share the same task routing default, set it once at
the manifest level and only override the exceptions:

```toml
[task_defaults]
run_in = "either"

[tasks]
qa = [{ task = "docs check links README.md" }]

[tasks.dev]
run_in = "container"
run = "bun run dev"
```

`[task_defaults]` only applies to tasks defined in that manifest file. Task-level
`run_in` still wins when both are present.

### Container Runtime Targets Across Catalogs

A catalog declares a container runtime target with a `[systems]` binding: the
default system resolves a workspace, and the workspace names a container from
`[containers]`:

```toml
[systems]
default = "ranch"

[systems.ranch]
default_workspace = "main"

[systems.ranch.workspaces.main]
container = "cattle"

[containers]
default = "cattle"

[containers.cattle]
driver = "colima"
project_name = "child-cattle"
primary_service = "cattle"
working_dir = "/workspace"

[containers.cattle.services.cattle]
catalog = "workspace-rust-bun"

[tasks.check]
run = "cargo test"
run_in = "container"
```

The catalog that owns a task owns its routing. When a nested member declares
its own `[systems]` table, `effigy <member>/<task>` runs on that member's
declared container — its compose project, activation, and exec — even when it
is invoked from the repository root and the root declares a different runtime
target. The root workspace never replaces a selected catalog's declared
target.

A catalog that declares no `[systems]` table inherits the invocation-scope
(ancestor) runtime. A `run_in = "container"` task with no resolvable target
fails before execution instead of silently running on the host.

Inspect the resolved target without executing:

```bash
effigy --plan cattle-grid/check
```

The plan prints the `runtime` target (`host`, `container`, or
`inline-container` with the container, primary service, and owning root), and
`effigy --json <selector> --plan` reports it under `result.runtime`.

Builtin test suites follow the same ownership rule. `effigy test` resolves
each target catalog's declared runtime target with the same binding grammar:
suites of a catalog with a named-container target (and their setup/teardown
steps) execute inside that container, while suites of a host-scoped catalog
stay on the host. Suites whose run carries nested effigy re-entries (`task:`
references) keep their own routing and are never wrapped into the target
container a second time. `effigy test --plan` reports each target's resolved
`runtime`.

### Validation and Run Arrays

```toml
[tasks.validate]
run = [
  { id = "lint", run = "bun run lint" },
  { id = "unit", task = "test vitest", timeout_ms = 180000 },
  { id = "report", run = "printf validate-ok" }
]
```

Steps execute in order and stop at the first failure, timeout or cancellation.
Host steps use the same kernel custody engine for text and JSON. See
[ordered sequences and locks](020-dag-lock-policy-baseline.md) for lifecycle,
reference cycles, shared locks and container migration limits.

Run-array step fields:

- `id = "<name>"`: optional non-empty, unique label for status and JSON.
- `run = "<command>"`: shell command; mutually exclusive with `task` and `rhai`.
- `task = "<selector>"`: invoke a task through the canonical resolver.
- `rhai = "<path>"`: run a file-backed Rhai script ([guide](061-rhai-script-steps-guide.md)).
- `timeout_ms = <n>`: host step lifetime in milliseconds; expiry settles custody and returns 124.
- `env = "<name>" | { KEY = "value" }`: apply environment directives at this point in the chain.
- `env_file = "<path>"`: update fallback dotenv sources for subsequent steps.

The former sequence dependency and retry policy keys are rejected with removal
instructions; declaration order defines the sequence.

`[task_defaults]` (covered in §3) does not apply at the array-element level;
it sets the default for whole tasks defined in the same manifest. Per-step
overrides happen inline on the array entry itself.

#### Rhai Script Step For Rust-First Glue

```toml
[tasks.report:stamp]
run = { rhai = "scripts/report-stamp.rhai" }
```

File-backed Rhai:

```rhai
let payload = #{ "task": task_name, "repo_root": repo_root };
write_file("tmp/proof.json", json_stringify(payload));
log("wrote tmp/proof.json");
```

Bootstrap-style env seeding:

```rhai
if copy_if_missing("infra/dev/bootstrap/app.env", ".env") {
    log("[ok] wrote .env from bootstrap template");
}
env_file_set(".env", "APP_HOST", "local.test");
env_file_set(".env", "APP_NAME", "Acme Local");
```

Another file-backed example:

```toml
[tasks.report:stamp]
run = { rhai = "scripts/report-stamp.rhai" }
```

Use this when:

- the repo is Rust-first
- the script is orchestration or repo glue
- adding Bun or more shell wrapper surface would be noise

Do not use this to fake shell pipelines or replace frontend build tooling.

For the full host API and v1 limits, use
[`061-rhai-script-steps-guide.md`](061-rhai-script-steps-guide.md).

#### Run-Array Env Directives

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

### Managed Dev Stack (`mode = "tui"`)

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
health_wait_timeout_secs = 90    # readiness deadline; defaults to 60
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
  gates pass. Managed lifecycle readiness probes only container-owned routes;
  host-target routes belong to concurrent app processes and are not probed
  before those processes start.
- `health_wait_timeout_secs = <N>` sets the readiness deadline in seconds and
  requires `health_wait = true`.
- `ready_message = "..."` is surfaced by the managed session once the ready
  signal fires; keep it short and operator-visible.

Concurrent-entry field reference:
- `start = <N>` controls process spawn order; ties keep manifest order.
- `start_after_ms = <N>` adds a delay before that ordered entry is spawned.
- `role = "lifecycle"` marks an entry whose exit status participates in the
  managed session's overall lifecycle (non-zero exit fails the session).
- `role = "shell"` opens an interactive workspace shell in that tab; pair with
  `service = "<service-name>"` to select the workspace service (usually
  `workspace` for the rust+bun developer surface).
- `setup = [{ run = "..." } | { task = "..." } | { rhai = "..." }]` runs
  ordered pre-steps inside that pane before the main `run`/`task` starts.
- `name = "..."` / `label = "..."` set the machine-stable identifier and the
  TUI-visible label; label defaults from `name` or the underlying task id.

Headless companion surface:

```sh
effigy dev --headless
effigy dev status
effigy dev logs [process] [--follow]
effigy dev stop
```

The headless supervisor uses the same concurrent plan and local-dev secret
unlock as the TUI. State and per-process logs live under
`.effigy/runtime/managed/<task>-<profile>/`.

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
- each run holds its task's gate, so unrelated tasks run concurrently in the same repo and every spelling of one task shares one gate
- distinct names such as `validate:activity-routing` and `validate-activity-routing` stay distinct
- set `tasks.<name>.lock = "<shared-name>"` when multiple tasks should serialize together
- a managed TUI run records its profile; two profiles of one task still exclude each other
- wait for a live run with `effigy --lock-wait-ms <MS> <task>` or `EFFIGY_LOCK_WAIT_MS`; inspect it with `effigy tasks status <selector>`
- a run's gate is released when it exits, crash and `kill -9` included; there is no unlock

## Public and internal tasks

Keep a short human inventory by declaring groups and hiding maintenance helpers:

```toml
[tasks.build]
run = [{ task = "build:prepare" }, { run = "./scripts/build {args}" }]
group = "Development"

[tasks."build:prepare"]
run = "./scripts/prepare"
visibility = "internal"

[tasks.provider-smoke]
run = "./scripts/provider-smoke {args}"
visibility = "internal"
group = "Proofs"
```

`effigy tasks` shows public tasks grouped by `group`, with `General` for
ungrouped tasks. `tasks --all` and `tasks --json` include every task; JSON gives
effective visibility, its reason and group. Internal tasks run directly:

```sh
effigy provider-smoke --verbose
effigy --plan provider-smoke
```

Explicit visibility wins. Otherwise unnamespaced task names remain public
entry points, referenced namespaced helpers become internal, and unreferenced
tasks stay public. The same keys work in compact task forms, includes and
bundle exports. Use explicit internal visibility for standalone generated
proofs; references alone cannot identify them.

The draft namespace was removed in v0.15 (`[drafts]`, `draft` step keys, the
QA member `surface = "draft"` value and the `effigy draft`/`effigy drafts`
commands). Each definition is refused with the fix: move it to
`[tasks.<name>]`, add `visibility = "internal"`, remove
`created`/`expires`/`purpose`, and replace `draft` step keys with `task`. Then
run it by name. Task expiry checks and the separate inventory are gone.
Includes remain explicit; Effigy never scans for or deletes temporary task
files.

## Testing and Legacy Interop

### Built-in Test Fanout and Suite Source of Truth

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

### Minimal Test Runner Override Only

```toml
[test.runners]
vitest = "bun x vitest run"
"cargo-nextest" = { command = "cargo nextest run --workspace --status-level skip" }
```

Use when auto-detection is fine but default commands need tuning.

### Deferral Fallback for Legacy Interop

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

### Bootstrap Repo Bring-Up

```toml
[bootstrap]
setup = ["workspace:js:prepare", "db:seed:schema"]
start = "dev"
submodules = true

[sources.content]
git = "git@github.com:acme/content.git"
rev = "4a1b0c8f2d9e4f0b6a3c5d7e9f1a2b3c4d5e6f70"
path = "../content"

[sources.catalogue]
git = "git@github.com:acme/catalogue.git"
rev_file = "catalogue.rev"
path = "../catalogue"
```

Use this when the repo should describe its own first-run bring-up path after
`effigy bootstrap <git-url>`.

Behavior:
- `setup` is an ordered list of ordinary task selectors, run in the cloned or
  updated repo after its pinned bundle, catalog and `[sources.<name>]` sources
  are materialised
- `start` is one ordinary selector, run last unless the operator passes
  `--no-start`
- `submodules = true` initialises submodules recursively at the commits
  `.gitmodules` records; omit it when the repo has none
- `[sources.<name>]` pins one checkout to exactly one commit (`rev` or
  `rev_file`). A missing path is cloned at the pin; an existing checkout with
  local changes or another HEAD is reported as drift and left untouched
- `path` values are relative to the manifest, so `../sibling` paths are normal
  and may sit outside the repo
- `effigy doctor` reports pinned-source drift (`sources.pinned-drift`)
- inline run steps, `{ task = ... }` entries, `[bootstrap].children`,
  `bootstrap deps sync`, `--db-seed`, `--fresh`, `--backend` and `--start` are
  retired in v0.15 and refuse with their replacement (Q-009)

## Developer Session Patterns

### Managed Shell Tabs

`[shell]` was removed in v0.15. Managed shell tabs always use
`exec ${SHELL:-/bin/zsh} -i`; there is no manifest override, and a manifest
that still declares `[shell]` is refused with that replacement.

### Task Cache (removed)

The `cache` task key and `effigy tasks cache` were removed in v0.15. A task
that still declares `cache` is refused with a message naming the fix: delete
the table and let the tool's own cache (cargo, turbo, vite) decide.

## Built-in Scans

### Built-in God-File Scanner

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

### Built-in Generated-Assets Scanner

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

### Built-in Duplicate-Blocks Scanner

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
- keep `doctor = false` as the default; the current `example-app` benchmark takes about `16.9s` and yields enough findings that this is better as an opt-in health check

### Built-in Generated-In-Src Scanner

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
- keep `doctor = true` as the default; the current `example-app` benchmark takes about `2.1s` and yields `4` warning-level findings, which is acceptable for default health runs

### Built-in Attention-Markers Scanner

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

### Built-in Comment-Ratio Scanner

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
- keep `doctor = true` as the default; the current `example-app` benchmark takes about `2.4s` and yields `15` findings, which is acceptable for default health runs

### Built-in Stale-Suppressions Scanner

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
- keep `doctor = false` as the default; the current `example-app` benchmark takes about `3.9s` and yields `69` findings, which is useful but too noisy for routine doctor runs

## System and Workspace Patterns

### Task-Local Runtime Env (Cargo Isolation)

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

### Flattened `[systems.<name>]` Substrate

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
compose + gateway + workspace shell) that `effigy container up`, `effigy workspace`,
and managed `mode = "tui"` tasks can all bind to.

Shape rules (current v0.3+ form):
- `[systems.<name>]` is flattened; there is no nested `[systems.<name>.workspace_defaults]`
  or managed subtable — workspace defaults live directly under the system.
- `[systems.<name>.services.<svc>]` accepts either inline service definition
  fields or `catalog = "<catalog-service-name>"` to reuse a shipped catalog
  fragment (e.g. `postgres`, `redis`, `mariadb`, `workspace-rust-bun`).
- `[systems.<name>.services.<svc>.health]` declares container health gates
  (`tcp = <port>`, `http = "<url>"`, `exec = ["..."]`, `timeout_ms = <N>`,
  `interval_ms = <N>`) used by `health_wait = true` and `effigy container status`.
- `[systems.<name>.workspaces.<ws>]` picks which service hosts the interactive
  workspace plus workspace-local overrides; defaults come from the system body.
- generated compose is internal and immutable; read it with
  `effigy container render <name>`. `infra/dev/catalog/` holds project catalog
  fragments, including those `effigy container migrate <name>` writes for a
  retired `compose_file`.

Typical commands:
- `effigy container up` / `effigy container down` / `effigy container status`
- `effigy container logs --follow`
- `effigy container profile repair` or `effigy container profile reset-runtime`
  for runtime recovery
- `effigy workspace` to open the resolved workspace shell after the environment
  is up

### Multi-Catalog Monorepo Baseline

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

### Sidecar Database Targets (`[data.targets.<name>]`)

```toml
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
databases = ["example-app", "example-app_test"]

[data.targets.legacy_mysql]
service = "mysql"
database = "example-app"
```

Use `[data.targets.<name>]` when a sidecar database should participate in
bootstrap/data seed/data dump without becoming part of `[bundle].databases`.

Rules:

- each target needs `service` (the container service name) and `database` (the
  logical database name)
- `data dump` and `data seed` resolve targets from both `[bundle].databases`
  and `[data.targets]`
- when multiple targets are declared, dump/seed inputs must name a target
- when only one target exists, the target name can be omitted

This is useful when:

- a legacy or migration database runs alongside the primary app database
- a target should participate in seed/dump but not in bundle-level deployment
  modeling

## Secrets

Declare secrets with consumer targets:

```toml
[secrets]
backend = "effigy-vault"

[secrets.keys.database_url]
required = true
targets = ["tasks", "containers"]
description = "Application database connection URL"
```

Store values with `effigy secrets set database_url`. Values inject into task
processes, containers, Rhai scripts, deploy hooks, and state hooks without
writing plaintext to repo files. See
[`075-secrets-and-vault-guide.md`](075-secrets-and-vault-guide.md).

## State Stacks

Declare ordered schema/seed/import/capture layers:

```toml
[state.uat]
schema = "effigy.state-stack.v1"
name = "acme-uat"
environment = "uat"

[[state.uat.layers]]
key = "structure"
role = "structure"
source = "db:migrate"
apply_mode = "task"
environment_policy = "all"
```

Plan with `effigy state plan uat`, apply with `effigy state apply uat --yes`,
capture with `effigy state capture uat new-content --yes`. See
[`073-state-stack-guide.md`](073-state-stack-guide.md).

## Deploy Config

Declare UAT and production deployment environments:

```toml
[deploy.uat]
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "acme-uat"
artifact_policy = "digest-preferred"

[deploy.uat.provider]
adapter = "railway"
```

Inspect intended work with `effigy deploy plan uat`. Model/export remain
supported; `deploy apply` and `deploy redeploy` are not yet supported in v0.15,
even with `--yes`. Full execution design is in Queue's deploy lane (Q-003).
See [`074-deployment-guide.md`](074-deployment-guide.md).

## Notes

- Catalog membership is declared by the root manifest. Nested `effigy.toml`
  files do not join the parent task surface automatically:

```toml
[catalog.members]
web = "apps/web"
api = "services/api"
```

  System mounts may reference a named member with `{ member = "web" }`, or
  declare an inline member with `{ source = "../tooling", catalog = true }`.
  Ordinary and legacy string mounts never imply membership.
- Catalog aliases must be unique across effective membership.
- Useful interpolation tokens in run commands:
  - `{project}` catalog root path (shell-quoted alias of `{repo}`)
  - `{repo}` catalog root path (shell-quoted)
  - `{args}` passthrough args (shell-quoted)
  - `{request}` unresolved selector (deferral only)
- For standard string commands, the catalog path uses the selected runtime's
  filesystem: a host path for host tasks, or the container workspace path plus
  the catalog suffix for container tasks. A host aggregate does not turn its
  container leaf's `{repo}` into a host path. Plans render that same command;
  literal host paths and user arguments are preserved.
- Useful interpolation tokens in `tasks.<name>.env` values and `[env]` entries:
  - `{project}` catalog root path
  - `{repo}` alias of `{project}`
- Standard command task env values use the same runtime catalog path; env-file
  and configuration discovery remain host-side. Mixed container sequences and
  managed commands keep their separate execution routes.

## Related Guides

- [`013-testing-orchestration.md`](013-testing-orchestration.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](048-built-in-test-suite-lifecycle-and-env.md)
- [`015-deferral-fallback-migration.md`](015-deferral-fallback-migration.md)
- [`016-task-routing-precedence.md`](016-task-routing-precedence.md)
- [`019-init-foundation.md`](019-init-foundation.md)
- [`020-dag-lock-policy-baseline.md`](020-dag-lock-policy-baseline.md)
- [`021-quick-start-and-command-cookbook.md`](021-quick-start-and-command-cookbook.md)
- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](055-everyday-workflows.md)
- [`063-container-system-guide.md`](063-container-system-guide.md)
- [`064-system-workspace-and-dev-contract.md`](064-system-workspace-and-dev-contract.md)
- [`073-state-stack-guide.md`](073-state-stack-guide.md)
- [`074-deployment-guide.md`](074-deployment-guide.md)
- [`075-secrets-and-vault-guide.md`](075-secrets-and-vault-guide.md)

## Expected Outcome

After this guide, you should be able to:

- choose a manifest pattern that matches the next real repo friction
- move task, env, test, and scan behavior into `effigy.toml` deliberately
- avoid growing new wrapper scripts when the manifest can own the workflow

## Next Step

After adapting one of these patterns, run `effigy tasks` and `effigy test --plan`
to confirm the repo now explains itself more clearly, then use
[`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md)
or [`025-command-reference-matrix.md`](025-command-reference-matrix.md) to
close any remaining rough edges in the operator path.
