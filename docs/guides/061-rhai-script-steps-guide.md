# 061 - Rhai Script Steps Guide

Use this guide when a Rust-first repo wants Effigy-native scripting instead of
another shell wrapper, Bun install, or repo-local Python glue script.

This is the front door for the Rhai-backed task step surface: when to use it,
how to declare it, what the v1 host API includes, and what it still does not
try to replace.

## Vision Alignment

- Primary tags: `ROUTE`, `CONTRACT`, `ADOPT`
- Target movement: Rust-first repos can move small automation glue into a
  bounded Effigy-native scripting surface without inventing a second runtime
  policy.

## 1) What Rhai Script Steps Are

Effigy now supports Rhai-backed run steps inside task `run = [ ... ]` arrays.

Use exactly one step entrypoint per step:

- `{ run = "..." }`
- `{ task = "..." }`
- `{ rhai = "scripts/example.rhai" }`

Rhai steps are for repo automation glue:

- small file/path transforms
- structured subprocess calls
- lightweight validation/report helpers
- Rust-first repo task glue that should not require Bun or shell

They are not the new universal runtime for every repo.

## 2) Use File-Backed Scripts

File-backed example:

```toml
[tasks.link:local]
run = [{ rhai = "scripts/rhai/install-local-bin-links.rhai" }]
```

Use `rhai = "..."` as a repo-relative Rhai script path when:

- the script is non-trivial
- you want normal file diffing/review
- the repo is building up a real native scripting surface under
  `scripts/rhai/`

## 3) Rhai v1 Host API

Current v1 helpers:

- logging:
  - `log(message)`
  - `log_warn(message)`
- context:
  - `args`
  - `cwd`
  - `repo_root`
  - `catalog_root`
  - `invocation_cwd`
  - `task_name`
- env and path helpers:
  - `env(name)`
  - `time::now_utc()`
  - `path::join(base, child)`
  - `path::file_name(path)`
  - `str::trim(value)`
  - `str::contains(value, needle)`
  - `str::starts_with(value, prefix)`
  - `str::ends_with(value, suffix)`
  - `str::replace(value, from, to)`
  - `str::split_lines(value)`
  - `regex::is_match(pattern, value)`
  - `regex::replace(pattern, value, replacement)`
  - `regex::escape(value)`
- file helpers:
  - `fs::make_temp_dir(prefix)`
  - `fs::append_file(path, contents)`
  - `fs::read_file(path)`
  - `fs::read_lines(path)`
  - `fs::write_file(path, contents)`
  - `fs::write_lines(path, lines_array)`
  - `fs::copy(source, destination)`
  - `fs::copy_if_missing(source, destination)`
  - `fs::env_file_entries(path)`
  - `fs::env_file_get(path, key)`
  - `fs::env_file_remove(path, key)`
  - `fs::env_file_set(path, key, value)`
  - `fs::exists(path)`
  - `fs::is_dir(path)`
  - `fs::list(path)`
  - `fs::is_file(path)`
  - `fs::is_symlink(path)`
  - `search::files(root, pattern, options_map)`
  - `fs::create_dir(path)`
  - `fs::remove(path)`
  - `fs::move_path(source, destination)`
  - `fs::replace_in_file(path, from, to)`
  - `fs::create_symlink(target, link)`
- structured data helpers:
  - `json::parse(raw)`
  - `json::stringify(value)`
  - `toml::parse(raw)`
  - `toml::stringify(value)`
- config helpers:
  - `config::raw()`
  - `config::effective()`
  - `config::get(path_string)`
- execution helpers:
  - `time::stop_requested()`
  - `time::process_id()`
  - `time::sleep_ms(milliseconds)`
  - `process::run(program, args_array)` / `process::run(program, args_array, options_map)`
  - `process::stream(program, args_array)` / `process::stream(program, args_array, options_map)`
  - `process::tee(program, args_array)` / `process::tee(program, args_array, options_map)`
  - `http::get(url)`
  - `http::post(url)` / `http::post(url, options_map)`
  - `http::request(method, url, options_map)`
  - `http::download(url, path)` / `http::download(url, path, options_map)`
  - `task::run(task, args_array)`
  - `task::list()` / `task::list(options_map)`
  - `task::resolve(selector)`
  - `task::info(selector)`
  - `container::up(name, detach_bool)`
  - `container::down(name)`
  - `container::shell(name, command_string)`
  - `container::exec(name, args_array)`
  - `container::exec(name, service, args_array)`
  - `container::status(name)`
  - `container::status(#{ all: true })`
  - `container::logs(name, options_map)`
  - `container::reset(name, options_map)`
  - `container::data("list", name)`
  - `container::data("export", name, volume, path)`
  - `container::data("import", name, volume, path)`
  - `container::data("pull_production", name)`
  - `container::eject(name)`
  - `container::stats()`
  - `docs::check_links(options_map)`
  - `docs::check_json_examples(options_map)`
  - `docs::check_headings(options_map)`
  - `docs::check_paths(options_map)`
  - `docs::check_contains(options_map)`
  - `docs::check_forbidden(options_map)`
  - `docs::check_index(options_map)`
  - `docs::check_next_action(options_map)`
  - `docs::check_workflow_paths(options_map)`
  - `docs::add_log_index(options_map)`
  - `bundle::inspect()`
  - `service::list()`
  - `service::extract(name, options_map)`
  - `catalog::tasks()` / `catalog::tasks(options_map)`
  - `gateway::status()`
  - `gateway::setup_tls(options_map)`
  - `gateway::up(options_map)`
  - `gateway::down(options_map)`
  - `doctor::run(options_map)`
  - `scan::god_files(options_map)`
  - `scan::generated_assets(options_map)`
  - `scan::generated_in_src(options_map)`
  - `scan::duplicate_blocks(options_map)`
  - `scan::comment_ratio(options_map)`
  - `scan::attention_markers(options_map)`
  - `scan::stale_suppressions(options_map)`
  - `cache::inspect(options_map)`
  - `cache::invalidate(options_map)`
  - `contracts::check_json(options_map)`
  - `contracts::validate_selection(options_map)`
  - `deploy::model()`
  - `deploy::emit(#{ provider: "render", ... })`
  - `system::status(options_map)`
  - `system::logs(options_map)`
  - `demo::list(options_map)`
  - `demo::inspect(options_map)`
  - `demo::history(options_map)`
  - `changelog::validate(options_map)`
  - `changelog::extract(options_map)`
  - `test::plan(options_map)`
  - `unlock::scopes(options_map)`
  - `container::down_all()`
  - `container::shell(name, command)`
  - `container::shell(name, service, command)`
  - `effigy::run(args_array)`
  - `effigy::run_json(args_array)`
  - `task::run_json(task, args_array)`
  - `config::get_or(path_string, default_value)`
  - `http::post(url, body_string)`
  - `fs::env_file_get_detail(path, key)`
  - `random::jwt_env_keys()`
  - `random::base64(size)`

`process::run(...)` is structured subprocess execution, not shell parsing.

That means:

- good: `process::run("cargo", ["test", "--workspace"])`
- not v1: shell pipelines, shell quoting tricks, or arbitrary shell emulation

Process helpers split cleanly by output behavior:

- `process::run(...)` captures output and returns it after exit
- `process::stream(...)` streams output live and does not return captured text
- `process::tee(...)` streams output live and also returns captured
  `stdout` / `stderr`
- all three accept an optional options map with `cwd`, `env`, and `stdin_file`

Prefer first-class host helpers over recursively invoking Effigy. For example,
use `container::exec("stack", "postgres", ["psql", "-U", "postgres", "-d", "acme", "-c", sql])`
instead of `process::run("effigy", ["exec", ...])` or `effigy::run(["exec", ...])`.
`process::run("effigy", ...)`, `process::stream("effigy", ...)`, and
`process::tee("effigy", ...)` are rejected at runtime; hitting that seam
means Effigy needs a new typed Rhai host helper.
Use `config::effective()` or `config::get("systems.dev.container")` instead of
re-reading `effigy.toml` when a script needs Effigy's composed/bundle-expanded
manifest view.
Rhai imports resolve from the selected catalog root, not from the shell's
current directory. This keeps cross-catalog routed tasks stable: a script owned
by `farmyard/` can import `scripts/tasks/shared.rhai` even when the operator
runs the task from the parent app repo. The top-level `catalog_root` and
`invocation_cwd` constants are available when a script needs to distinguish the
selected task catalog from the operator's original working directory.
Similarly, use `http::request(...)` or `http::post(...)` instead of
`process::run("curl", [...])` for smoke probes.
Use `search::files(root, pattern, #{ glob: "*.rs" })` plus `regex::*` helpers
instead of `process::run("rg", [...])` for portable file audits and
allowlist-style path filtering.

`effigy::run(...)` and `effigy::run_json(...)` are escape hatches for surfaces
that do not yet have a typed helper. First-party scripts should use the typed
helper when one exists. First-party shipped Rhai scripts currently use neither
escape hatch; a regression test keeps that true. The maintained coverage matrix is in
[`068-rhai-host-surface-audit.md`](./068-rhai-host-surface-audit.md).

Helpers that mirror CLI reports return the same JSON payload as the CLI
`--json` mode, converted into Rhai maps/arrays. Process-like helpers such as
`container::exec(...)` return `{ status, success, stdout, stderr }`.

## 4) Practical Patterns

Structured file copy without loading the whole template through script memory:

```toml
[tasks.report:write]
run = [{ rhai = "scripts/rhai/copy-template.rhai" }]
```

```rhai
if fs::copy_if_missing("infra/dev/bootstrap/template.env", ".env") {
    log("[ok] wrote .env from template");
}
```

Small in-place mutation after copy:

```rhai
fs::copy_if_missing("infra/dev/bootstrap/app.env", ".env");
fs::replace_in_file(".env", "APP_HOST=example.test", "APP_HOST=local.test");
```

Envfile-aware mutation when the script really means “set this key”:

```rhai
fs::copy_if_missing("infra/dev/bootstrap/app.env", ".env");
fs::env_file_set(".env", "APP_HOST", "local.test");
fs::env_file_set(".env", "APP_NAME", "Cumberland Local");
```

Structured process call with live progress plus captured output:

```rhai
let result = process::tee("cargo", ["test", "-p", "effigy-rhai", "--lib"]);
if !result["success"] {
    throw result["stderr"];
}
```

Structured process call with an overridden working directory and env:

```rhai
let result = process::run(
    "sh",
    ["-lc", "printf '%s|%s' \"$PWD\" \"$APP_ENV\""],
    #{ cwd: "services/api", env: #{ APP_ENV: "local" } },
);
if result["stdout"] != cwd + "/services/api|local" {
    throw result["stdout"];
}
```

Structured process call with file-backed stdin:

```rhai
let result = process::tee(
    "mysql",
    ["--skip-ssl", "-h", "db", "-uroot", "cbs"],
    #{ stdin_file: "/var/www/html/.effigy/local/db-seeds/latest.sql" },
);
if !result["success"] {
    throw result["stderr"];
}
```

Reading and pruning dotenv entries:

```rhai
let env = fs::env_file_entries(".env");
if env["APP_HOST"] == "example.test" {
    fs::env_file_remove(".env", "LEGACY_FLAG");
}
```

Structured process call:

```toml
[tasks.test:smoke]
run = [{ rhai = "scripts/rhai/test-smoke.rhai" }]
```

Ephemeral workspace and timestamp:

```rhai
let generated_at = time::now_utc();
let scratch = fs::make_temp_dir("repo-proof");
```

Long-running lifecycle loop:

```rhai
while !time::stop_requested() {
    fs::append_file("artifacts/events.log", `heartbeat ${time::now_utc()}\n`);
    time::sleep_ms(1000);
}
```

Nested task call:

```toml
[tasks.docs:proof]
run = [{ rhai = "scripts/rhai/docs-proof.rhai" }]
```

## 5) Good Boundary

Use Rhai when the repo is Rust-first and the script is mostly orchestration or
repo automation glue.

Use Bun + TypeScript when the repo is web-oriented and already lives in that
toolchain.

Keep external ecosystem tools when the job is genuinely attached to that
ecosystem:

- frontend build systems
- Electron packaging stacks
- ML/data pipelines that depend on Python-native libraries

## 6) v1 Limits

Rhai v1 intentionally does not provide:

- arbitrary shell emulation
- shell pipelines and shell quoting semantics
- broad socket-level or streaming network APIs
- a frontend/build-tool replacement layer
- a promise that every historical shell or Python script should disappear in
  one pass

That narrow boundary is deliberate. The product goal is “native scripting for
repo glue,” not “Effigy becomes a replacement shell.”

## Expected Outcome

After this guide, you should be able to:

- declare a Rhai-backed run step with `rhai = "path/to/script.rhai"`
- use the current v1 host API safely
- decide when Rhai is the right tool versus Bun + TS or an external ecosystem
- migrate small Rust-repo glue tasks without reintroducing shell wrappers

## Related Guides

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
- [`../roadmaps/g02/004-rust-native-scripting-surface-contract.md`](../roadmaps/g02/004-rust-native-scripting-surface-contract.md)

## Next Step

After this guide, use
[`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md) if
the next job is splitting Rhai scripts into focused manifest fragments, use
[`022-manifest-cookbook.md`](./022-manifest-cookbook.md) if you want broader
task-pattern examples, or return to the active `g02.004` spec lane when the
next move is deciding which repo migration slice should land after the
foundation.
