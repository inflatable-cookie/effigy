# Rhai Host Surface Audit

This audit tracks Effigy features that are available to `.rhai` scripts without
recursively launching `effigy`.

Policy:

- Prefer typed helpers such as `container::exec(...)` and `bundle::inspect()`.
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
- Runtime-sensitive scripts should move toward the universal runtime context and
  execution request helper. `process::run(...)` and `container::exec(...)`
  remain low-level helpers, not the preferred way for first-party scripts to
  choose host versus container execution.

## Exposed Helpers

### Low-Level Runtime Helpers

| Surface | Rhai helpers | Status |
| --- | --- | --- |
| Logging and context | `log`, `log_warn`, `env` (flat); `time::now_utc`, `time::process_id`, `time::sleep_ms`, `time::stop_requested` | Exposed |
| Path and string utilities | `path::join`, `path::file_name`, `path::parent`, `str::trim`, `str::contains`, `str::starts_with`, `str::ends_with`, `str::replace`, `str::split_lines`, `str::parse_int`, `str::shell_quote`, `regex::is_match`, `regex::replace`, `regex::captures`, `regex::escape`, `url::parse`, `url::query_get`, `url::parse_mysql_dsn`, `url::parse_pg_dsn` | Exposed |
| File and directory operations | `fs::make_temp_dir`, `fs::make_temp_file`, `fs::read_file`, `fs::read_lines`, `fs::write_file`, `fs::append_file`, `fs::write_lines`, `fs::copy`, `fs::copy_if_missing`, `fs::env_file_entries`, `fs::env_file_map`, `fs::env_file_get`, `fs::env_file_remove`, `fs::env_file_set`, `fs::env_file_get_detail`, `fs::move_path`, `fs::replace_in_file`, `fs::exists`, `fs::is_dir`, `fs::is_file`, `fs::is_symlink`, `fs::file_size`, `fs::sha256`, `fs::list`, `fs::list_recursive`, `fs::create_dir`, `fs::remove`, `fs::create_symlink`, `search::files` | Exposed |
| Structured data | `json::parse`, `json::stringify`, `json::stringify_compact`, `json::read_file`, `json::write_file`, `toml::parse`, `toml::stringify`, `toml::read_file`, `toml::write_file`, `yaml::parse`, `yaml::stringify`, `yaml::read_file`, `yaml::write_file` | Exposed |
| Host subprocess execution | `process::run`, `process::stream`, `process::tee` | Exposed |
| Basic HTTP | `http::get`, `http::post`, `http::request`, `http::download`, `http::capture` | Exposed |

| Surface | Rhai helpers | Status |
| --- | --- | --- |
| Runtime context | `runtime::context` backed by `EffigyRuntimeContext` | Exposed |
| Execution request | `exec::run` backed by Effigy's routed execution helpers | Exposed |
| Config | `config::raw`, `config::effective`, `config::get`, `config::get_or`, `config::user_path`, `config::user_get`, `config::user_set`, `config::user_unset` | Exposed |
| Tasks | `task::run`, `task::run_json`, `task::list`, `task::resolve`, `task::info`, `catalog::tasks` | Exposed |
| Container lifecycle | `container::up`, `container::down`, `container::down_all`, `container::shell`, `container::shell` (with service), `container::exec` | Exposed |
| Container inspection | `container::status`, `container::logs`, `container::stats` | Exposed |
| Container cleanup | `container::cache_list`, `container::cache_prune`, `container::volume_list`, `container::volume_prune` | Exposed |
| Container data | `container::data`, `container::data_dump`, `container::data_seed`, `container::data_pull_production` | Exposed |
| Container reset/eject | `container::reset`, `container::eject` | Exposed |
| State stack orchestration | `state::plan`, `state::apply`, `state::capture`, `state::history` | Exposed |
| State capture context | `state::capture_context`, `state::capture_context_path`, `state::capture_source`, `state::capture_destination_ref` | Exposed |
| Artifacts | `artifact::inspect`, `artifact::stage`, `artifact::capture` | Exposed |
| Docs checks | `docs::check_links`, `docs::check_json_examples`, `docs::check_headings`, `docs::check_paths`, `docs::check_contains`, `docs::check_forbidden`, `docs::check_index`, `docs::check_next_action`, `docs::check_workflow_paths`, `docs::add_log_index` | Exposed |
| Bundles | `bundle::inspect` | Exposed |
| Services | `service::list`, `service::extract` | Exposed |
| Gateway | `gateway::status`, `gateway::setup_tls`, `gateway::up`, `gateway::down` | Exposed |
| Doctor | `doctor::run` | Exposed |
| Scan | `scan::god_files`, `scan::generated_assets`, `scan::generated_in_src`, `scan::duplicate_blocks`, `scan::comment_ratio`, `scan::attention_markers`, `scan::stale_suppressions` | Exposed |
| Cache | `cache::inspect`, `cache::invalidate` | Exposed |
| Contracts | `contracts::check_json`, `contracts::validate_selection` | Exposed |
| Deploy | `deploy::model`, `deploy::emit`, `deploy::provider_context`, `deploy::provider_context_path`, `deploy::provider_report_path`, `deploy::provider_report` | Exposed |
| System | `system::status`, `system::logs` | Exposed |
| Demo | `demo::list`, `demo::inspect`, `demo::history` | Exposed |
| Changelog | `changelog::validate`, `changelog::extract` | Exposed |
| Test | `test::plan` | Exposed |
| Unlock | `unlock::scopes` | Exposed |
| Secrets | `secrets::get`, `secrets::has`, `secrets::set`, `secrets::set_many` | Exposed |
| Effigy | `effigy::active_version`, `effigy::run`, `effigy::run_json` | Exposed |

## Missing But Planned

| Surface | Status | Notes |
| --- | --- | --- |
| `state::capture_set` | missing | CLI has `state capture-set`; Rhai only has `state::capture` |
| `deploy::plan` | missing | CLI has full deploy transaction surface |
| `deploy::apply` | missing | CLI has full deploy transaction surface |
| `deploy::status` | missing | CLI has full deploy transaction surface |
| `deploy::history` | missing | CLI has full deploy transaction surface |
| `deploy::redeploy` | missing | CLI has full deploy transaction surface |
| `distribution` | missing | CLI has `distribution preflight`, `check-glibc-floor`, `validate-artifacts` |

## Intentionally CLI-First

| Surface | Reason |
| --- | --- |
| Release execution | Human confirmation and release protocol safety |
| Distribution publishing | External publishing side effects |
| Bootstrap | Repo creation and handoff flow |
| Workspace sessions | Interactive/container session lifecycle |
| Demo browser/input sessions | Long-running human-facing UI |
| Attached/following logs | Long-running terminal ownership |
| Init | Interactive scaffolding with TTY prompts |
| Watch | Long-running file-watching loops |
| Tasks migrate | One-shot import utility |

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

`http::capture(method, url, path, options)` writes the text body to `path` and
also returns it:

```rhai
#{
  status: 500,
  success: false,
  path: "/repo/tmp/response.txt",
  body: "forced boom",
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
- `http::capture(method, url, path, options)` — request with optional
  `headers`, `body`, `timeout_ms`, and `danger_accept_invalid_certs`, write the
  response text to `path`, and return status/body/header metadata.

### Shell-Replacement Convenience

- `fs::make_temp_file(prefix)` — create an empty unique temp file and return
  the absolute path.
- `fs::list_recursive(path)` — return sorted recursive file paths under `path`.
- `fs::list_recursive(path, #{ extension: "rs" })` — return sorted recursive
  file paths with one extension; a leading dot is optional.
- `fs::env_file_map(path)` — parse dotenv-style files into a Rhai map, returning
  an empty map when the file is missing.
- `str::parse_int(value)` — trim and parse a value into a Rhai integer.

## Runtime and Execution Surface

Effigy already exposes a typed runtime context helper and a routed execution
helper for scripts that need Effigy to choose the correct host-versus-container
path from declared intent.

### Runtime Context

Shape:

```rhai
let ctx = runtime::context();
```

Return shape:

```rhai
#{
  invocation_cwd: "/path/where/effigy/started",
  command_root: "/path/to/target/repo",
  repo_override: "/path/to/target/repo",
  invocation_mode: "host",
  inside_container_handoff: false,
  host: #{
    os: "macos",
    arch: "aarch64",
    no_color: false,
    ci: false,
  },
}
```

Rules:

- the map is read-only from Effigy's perspective
- paths are strings because Rhai has no native path type
- `repo_override` is `()` when no explicit override was used
- `invocation_mode` is `"host"` or `"container_handoff"`
- scripts may use this for reporting and path construction, but execution
  routing should still go through `exec::run(...)`

### Execution Helper

Shape:

```rhai
let result = exec::run(
    ["mysql", "--skip-ssl", "-h", "db", "-uroot", database],
    #{
        run_in: "container",
        container: "web",
        service: "db",
        stdin_file: staged_path,
    },
);
```

Required options:

- `run_in`: `"host"`, `"container"`, or `"either"`
- `container`: container stack name when `run_in = "container"`
- `service`: service name when the command targets a specific service

Optional options:

- `cwd`: repo-relative or absolute working directory
- `env`: map of string env overrides
- `stdin_file`: repo-relative or absolute file path to stream into stdin
- `output`: `"capture"` initially; `"stream"` and `"tee"` can follow later

Return shape matches process-like helpers and adds route detail:

```rhai
#{
  status: 0,
  success: true,
  stdout: "...",
  stderr: "...",
  route: #{
    run_in: "container",
    container: "web",
    service: "db",
    invocation_mode: "host",
  },
}
```

Routing rules:

- `run_in = "host"` uses host process execution
- `run_in = "container"` uses the container manager when invocation mode is host
- `run_in = "container"` with `inside_container_handoff = true` runs the command
  directly when the active handoff already targets the requested service
- `stdin_file` resolves from the captured command root unless absolute
- scripts should not switch manually between `process::run(...)` and
  `container::exec(...)` for the same logical command

### DecodeLabs Seed Migration Target

```rhai
let reset = exec::run(
    mysql_args + ["-e", reset_sql],
    #{ run_in: "container", container: container_name, service: "db" },
);

let imported = exec::run(
    mysql_args + [database],
    #{
        run_in: "container",
        container: container_name,
        service: "db",
        stdin_file: staged_path,
    },
);
```

The script should stop constructing separate host/container execution branches.
Effigy owns the path and handoff decision.

### Envfile Detail

- `fs::env_file_get_detail(path, key)` — returns a map with `file_exists`, `key_exists`,
  and `value` fields so scripts can distinguish missing files from missing keys.

### Container Service Targeting

- `container::shell(name, command)` — shell in the default service
- `container::shell(name, service, command)` — shell in a specific service
- `container::down_all()` — stop all containers (equivalent to `effigy container down --global`)

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

`effigy::active_version()` returns the current running binary's active version
string.

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
