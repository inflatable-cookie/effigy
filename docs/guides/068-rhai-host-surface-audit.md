# Rhai Host Surface Audit

This audit tracks Effigy features that are available to `.rhai` scripts without
recursively launching `effigy`.

Policy:

- Prefer typed helpers such as `container::exec(...)` and `bundle::emit(...)`.
- Do not use `process::run("effigy", ...)` in first-party Rhai scripts.
- `process::run("effigy", ...)`, `process::stream("effigy", ...)`, and
  `process::tee("effigy", ...)` are rejected at runtime; add a typed host
  helper instead.
- First-party `process::run(...)`, `process::stream(...)`, and
  `process::tee(...)` use is covered by a static allowlist test. New
  entries should be rare and justified.
- Treat `effigy::run(...)` and `effigy::run_json(...)` as escape hatches only.
- First-party shipped Rhai scripts currently use neither `effigy::run(...)` nor
  `effigy::run_json(...)`; a regression test keeps that true.
- Keep long-running interactive flows CLI-first unless they gain a script-safe mode.

## Exposed Helpers

### Low-Level Runtime Helpers

| Surface | Rhai helpers | Status |
| --- | --- | --- |
| Logging and context | `log`, `log_warn`, `env` (flat); `time::now_utc`, `time::process_id`, `time::sleep_ms`, `time::stop_requested` | Exposed |
| Path and string utilities | `path::join`, `path::file_name`, `str::trim`, `str::contains`, `str::starts_with`, `str::ends_with`, `str::replace`, `str::split_lines`, `str::shell_quote` | Exposed |
| File and directory operations | `fs::make_temp_dir`, `fs::read_file`, `fs::read_lines`, `fs::write_file`, `fs::append_file`, `fs::write_lines`, `fs::copy`, `fs::copy_if_missing`, `fs::env_file_entries`, `fs::env_file_get`, `fs::env_file_remove`, `fs::env_file_set`, `fs::env_file_get_detail`, `fs::move_path`, `fs::replace_in_file`, `fs::exists`, `fs::is_dir`, `fs::is_file`, `fs::is_symlink`, `fs::list`, `fs::create_dir`, `fs::remove`, `fs::create_symlink`, `search::files` | Exposed |
| Structured data | `json::parse`, `json::stringify`, `toml::parse`, `toml::stringify` | Exposed |
| Host subprocess execution | `process::run`, `process::stream`, `process::tee` | Exposed |
| Basic HTTP | `http::get`, `http::post`, `http::request`, `http::download` | Exposed |

| Surface | Rhai helpers | Status |
| --- | --- | --- |
| Config | `config::raw`, `config::effective`, `config::get`, `config::get_or` | Exposed |
| Tasks | `task::run`, `task::run_json`, `task::list`, `task::resolve`, `task::info`, `catalog::tasks` | Exposed |
| Container lifecycle | `container::up`, `container::down`, `container::down_all`, `container::shell`, `container::shell` (with service), `container::exec` | Exposed |
| Container inspection | `container::status`, `container::logs`, `container::stats` | Exposed |
| Container data | `container::data` | Exposed |
| Container reset/eject | `container::reset`, `container::eject` | Exposed |
| Docs checks | `docs::check_links`, `docs::check_json_examples`, `docs::check_headings`, `docs::check_paths`, `docs::check_contains`, `docs::check_forbidden`, `docs::check_index`, `docs::check_next_action`, `docs::check_workflow_paths`, `docs::add_log_index` | Exposed |
| Bundles | `bundle::list`, `bundle::inspect`, `bundle::emit` | Exposed |
| Services | `service::list`, `service::extract` | Exposed |
| Gateway | `gateway::status`, `gateway::setup_tls`, `gateway::up`, `gateway::down` | Exposed |
| Doctor | `doctor::run` | Exposed |
| Scan | `scan::god_files`, `scan::generated_assets`, `scan::generated_in_src`, `scan::duplicate_blocks`, `scan::comment_ratio`, `scan::attention_markers`, `scan::stale_suppressions` | Exposed |
| Cache | `cache::inspect`, `cache::invalidate` | Exposed |
| Contracts | `contracts::check_json`, `contracts::validate_selection` | Exposed |
| Deploy | `deploy::model`, `deploy::emit` | Exposed |
| System | `system::status`, `system::logs` | Exposed |
| Demo | `demo::list`, `demo::inspect`, `demo::history` | Exposed |
| Changelog | `changelog::validate`, `changelog::extract` | Exposed |
| Test | `test::plan` | Exposed |
| Unlock | `unlock::scopes` | Exposed |
| Effigy | `effigy::run`, `effigy::run_json` | Exposed |

## Intentionally CLI-First

| Surface | Reason |
| --- | --- |
| Release execution | Human confirmation and release protocol safety |
| Distribution publishing | External publishing side effects |
| Bootstrap | Repo creation and handoff flow |
| Workspace sessions | Interactive/container session lifecycle |
| Demo browser/input sessions | Long-running human-facing UI |
| Attached/following logs | Long-running terminal ownership |

## Return Shapes

Process-like helpers return:

```rhai
#{
  status: 0,
  success: true,
  stdout: "...",
  stderr: "...",
}
```

For subprocess helpers:

- `process::run(...)` captures output and returns it after exit
- `process::stream(...)` streams output live and does not capture it
- `process::tee(...)` streams output live and also returns captured
  `stdout` / `stderr`
- all three accept an optional options map with `cwd`, `env`, and `stdin_file`

Command/report helpers return the same JSON payload as their CLI `--json`
counterpart, converted into a Rhai map/array value.

Config helpers expose the manifest view Effigy uses:

- `config_raw()` returns the parsed root `effigy.toml` only.
- `config_effective()` returns the composed, include-merged, bundle-expanded
  manifest plus composition metadata.
- `config::get(path)` returns a value from the effective manifest, using dot
  paths such as `systems.dev.container`.

HTTP helpers return:

```rhai
#{
  status: 200,
  success: true,
  headers: #{},
  body: "...",
}
```

`http::download(...)` returns:

```rhai
#{
  status: 200,
  success: true,
  path: "/repo/tmp/download.bin",
  size: 1234,
  headers: #{},
}
```

## New Helpers

### Structured Task Output

- `task::run_json(task, args)` — like `task::run` but parses the task output as JSON
  and returns a Rhai dynamic value.

### Config with Default

- `config::get_or(path, default)` — like `config::get` but returns `default` instead
  of `()` when the path is missing or null.

### HTTP Convenience

- `http::post(url, body)` — POST with a plain string body; equivalent to
  `http::post(url, #{ body: "..." })`.

### Envfile Detail

- `fs::env_file_get_detail(path, key)` — returns a map with `file_exists`, `key_exists`,
  and `value` fields so scripts can distinguish missing files from missing keys.

### Container Service Targeting

- `container::shell(name, command)` — shell in the default service
- `container::shell(name, service, command)` — shell in a specific service
- `container::down_all()` — stop all containers (equivalent to `effigy container --all down`)

### Crypto and Random

- `random::jwt_env_keys()` — returns a map with `private_key` and `public_key`
- `random::base64(size)` — returns a secure random base64 string

## Return Shapes

### Process-like helpers

Process-like helpers return:

```rhai
#{
  status: 0,
  success: true,
  stdout: "...",
  stderr: "...",
}
```

For subprocess helpers:

- `process::run(...)` captures output and returns it after exit
- `process::stream(...)` streams output live and does not capture it
- `process::tee(...)` streams output live and also returns captured
  `stdout` / `stderr`
- all three accept an optional options map with `cwd`, `env`, and `stdin_file`

### Effigy escape hatches

`effigy::run(...)` returns:

```rhai
#{
  status: 0,
  success: true,
  output: "...",
  error: "",
  rendered_output: "",
}
```

`effigy::run_json(...)` returns the parsed JSON payload directly as a Rhai
map/array value.

### Envfile detail

`fs::env_file_get_detail(...)` returns:

```rhai
#{
  file_exists: true,
  key_exists: true,
  value: "...",
}
```

## Option Conventions

Helpers with optional arguments accept a Rhai map:

```rhai
docs::check_headings(#{
  paths: ["docs/README.md"],
  required_headings: ["Overview", "Next"],
});

container::logs("stack", #{ service: "postgres" });

scan::god_files(#{
  threshold: 900,
  show_warnings: true,
});

let response = http::post("https://api.acme.test/v1/dev/error-smoke");
if response["status"] != 500 {
  throw response["body"];
}

let matches = search::files("crates/api/src/routes", "StatusCode::BAD_REQUEST", #{ glob: "*.rs" });
if matches["success"] {
  throw matches["stdout"];
}
```

`container::logs(..., #{ follow: true })` is rejected from Rhai. Follow mode is
terminal-attached and should stay CLI-first.
