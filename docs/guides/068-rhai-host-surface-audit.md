# Rhai Host Surface Audit

This audit tracks Effigy features that are available to `.rhai` scripts without
recursively launching `effigy`.

Policy:

- Prefer typed helpers such as `container_exec(...)` and `bundle_export(...)`.
- Do not use `run_process("effigy", ...)` in first-party Rhai scripts.
- `run_process("effigy", ...)`, `run_process_stream("effigy", ...)`, and
  `run_process_tee("effigy", ...)` are rejected at runtime; add a typed host
  helper instead.
- First-party `run_process(...)`, `run_process_stream(...)`, and
  `run_process_tee(...)` use is covered by a static allowlist test. New
  entries should be rare and justified.
- Treat `run_effigy(...)` and `run_effigy_json(...)` as escape hatches only.
- First-party shipped Rhai scripts currently use neither `run_effigy(...)` nor
  `run_effigy_json(...)`; a regression test keeps that true.
- Keep long-running interactive flows CLI-first unless they gain a script-safe mode.

## Exposed Helpers

### Low-Level Runtime Helpers

| Surface | Rhai helpers | Status |
| --- | --- | --- |
| Logging and context | `log`, `log_warn`, `env`, `stop_requested`, `now_utc`, `process_id`, `sleep_ms` | Exposed |
| Path and string utilities | `path_join`, `path_file_name`, `trim_string`, `string_contains`, `string_starts_with`, `string_ends_with`, `replace_string`, `split_lines`, `shell_quote_string` | Exposed |
| File and directory operations | `make_temp_dir`, `read_file`, `read_lines`, `write_file`, `append_file`, `write_lines`, `copy_file`, `copy_if_missing`, `env_file_entries`, `env_file_get`, `env_file_remove`, `env_file_set`, `move_path`, `replace_in_file`, `path_exists`, `is_dir`, `is_file`, `is_symlink`, `list_dir`, `create_dir`, `remove_path`, `create_symlink`, `search_files` | Exposed |
| Structured data | `json_parse`, `json_stringify`, `toml_parse`, `toml_stringify` | Exposed |
| Host subprocess execution | `run_process`, `run_process_stream`, `run_process_tee` | Exposed |
| Basic HTTP | `http_get`, `http_post`, `http_request`, `http_download` | Exposed |

| Surface | Rhai helpers | Status |
| --- | --- | --- |
| Config | `config_raw`, `config_effective`, `config_get` | Exposed |
| Tasks | `run_task`, `tasks_list`, `task_resolve`, `task_info`, `catalog_tasks` | Exposed |
| Container lifecycle | `container_up`, `container_down`, `container_down_all`, `container_shell`, `container_shell` (with service), `container_exec` | Exposed |
| Container inspection | `container_status`, `container_status_all`, `container_logs`, `container_stats_all` | Exposed |
| Container data | `container_data_list`, `container_data_export`, `container_data_import`, `container_data_pull_production` | Exposed |
| Container reset/eject | `container_reset`, `container_eject` | Exposed |
| Docs checks | `docs_check_links`, `docs_check_json_examples`, `docs_check_headings`, `docs_check_paths`, `docs_check_contains`, `docs_check_forbidden`, `docs_check_index`, `docs_check_next_action`, `docs_check_workflow_paths`, `docs_add_log_index` | Exposed |
| Bundles | `bundle_list`, `bundle_inspect`, `bundle_export` | Exposed |
| Services | `service_list`, `service_extract` | Exposed |
| Gateway | `gateway_status`, `gateway_setup_tls`, `gateway_up`, `gateway_down` | Exposed |
| Doctor | `doctor` | Exposed |
| Scan | `scan_god_files`, `scan_generated_assets`, `scan_generated_in_src`, `scan_duplicate_blocks`, `scan_comment_ratio`, `scan_attention_markers`, `scan_stale_suppressions` | Exposed |
| Cache | `cache_inspect`, `cache_invalidate` | Exposed |
| Contracts | `contracts_check_json`, `contracts_validate_selection` | Exposed |
| Deploy | `deploy_model`, `deploy_export_render`, `deploy_export_railway` | Exposed |
| System | `system_status`, `system_logs` | Exposed |
| Demo | `demo_list`, `demo_inspect`, `demo_history` | Exposed |
| Changelog | `changelog_validate`, `changelog_extract` | Exposed |
| Test | `test_plan` | Exposed |
| Unlock | `unlock` | Exposed |

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

- `run_process(...)` captures output and returns it after exit
- `run_process_stream(...)` streams output live and does not capture it
- `run_process_tee(...)` streams output live and also returns captured
  `stdout` / `stderr`
- all three accept an optional options map with `cwd`, `env`, and `stdin_file`

Command/report helpers return the same JSON payload as their CLI `--json`
counterpart, converted into a Rhai map/array value.

Config helpers expose the manifest view Effigy uses:

- `config_raw()` returns the parsed root `effigy.toml` only.
- `config_effective()` returns the composed, include-merged, bundle-expanded
  manifest plus composition metadata.
- `config_get(path)` returns a value from the effective manifest, using dot
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

`http_download(...)` returns:

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

- `run_task_json(task, args)` — like `run_task` but parses the task output as JSON
  and returns a Rhai dynamic value.

### Config with Default

- `config_get_or(path, default)` — like `config_get` but returns `default` instead
  of `()` when the path is missing or null.

### HTTP Convenience

- `http_post(url, body)` — POST with a plain string body; equivalent to
  `http_post(url, #{ body: "..." })`.

### Envfile Detail

- `env_file_get_detail(path, key)` — returns a map with `file_exists`, `key_exists`,
  and `value` fields so scripts can distinguish missing files from missing keys.

### Container Service Targeting

- `container_shell(name, command)` — shell in the default service
- `container_shell(name, service, command)` — shell in a specific service
- `container_down_all()` — stop all containers (equivalent to `effigy container --all down`)

### Crypto and Random

- `generate_jwt_env_keys()` — returns a map with `private_key` and `public_key`
- `generate_random_base64(size)` — returns a secure random base64 string

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

- `run_process(...)` captures output and returns it after exit
- `run_process_stream(...)` streams output live and does not capture it
- `run_process_tee(...)` streams output live and also returns captured
  `stdout` / `stderr`
- all three accept an optional options map with `cwd`, `env`, and `stdin_file`

### Effigy escape hatches

`run_effigy(...)` returns:

```rhai
#{
  status: 0,
  success: true,
  output: "...",
  error: "",
  rendered_output: "",
}
```

`run_effigy_json(...)` returns the parsed JSON payload directly as a Rhai
map/array value.

### Envfile detail

`env_file_get_detail(...)` returns:

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
docs_check_headings(#{
  paths: ["docs/README.md"],
  required_headings: ["Overview", "Next"],
});

container_logs("stack", #{ service: "postgres" });

scan_god_files(#{
  threshold: 900,
  show_warnings: true,
});

let response = http_post("https://api.acme.test/v1/dev/error-smoke");
if response["status"] != 500 {
  throw response["body"];
}

let matches = search_files("crates/api/src/routes", "StatusCode::BAD_REQUEST", #{ glob: "*.rs" });
if matches["success"] {
  throw matches["stdout"];
}
```

`container_logs(..., #{ follow: true })` is rejected from Rhai. Follow mode is
terminal-attached and should stay CLI-first.
