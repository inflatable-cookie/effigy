# Rhai Host Surface Audit

This audit tracks Effigy features that are available to `.rhai` scripts without
recursively launching `effigy`.

Policy:

- Prefer typed helpers such as `container_exec(...)` and `bundle_export(...)`.
- Do not use `run_process("effigy", ...)` in first-party Rhai scripts.
- `run_process("effigy", ...)` and `run_process_stream("effigy", ...)` are
  rejected at runtime; add a typed host helper instead.
- First-party `run_process(...)` and `run_process_stream(...)` use is covered
  by a static allowlist test. New entries should be rare and justified.
- Treat `run_effigy(...)` and `run_effigy_json(...)` as escape hatches only.
- Keep long-running interactive flows CLI-first unless they gain a script-safe mode.

## Exposed Helpers

### Low-Level Runtime Helpers

| Surface | Rhai helpers | Status |
| --- | --- | --- |
| Logging and context | `log`, `log_warn`, `env`, `stop_requested`, `now_utc`, `process_id`, `sleep_ms` | Exposed |
| Path and string utilities | `path_join`, `path_file_name`, `trim_string`, `string_contains`, `string_starts_with`, `string_ends_with`, `replace_string`, `split_lines`, `shell_quote_string` | Exposed |
| File and directory operations | `make_temp_dir`, `read_file`, `read_lines`, `write_file`, `append_file`, `write_lines`, `copy_file`, `copy_if_missing`, `env_file_entries`, `env_file_get`, `env_file_remove`, `env_file_set`, `move_path`, `replace_in_file`, `path_exists`, `is_dir`, `is_file`, `is_symlink`, `list_dir`, `create_dir`, `remove_path`, `create_symlink`, `search_files` | Exposed |
| Structured data | `json_parse`, `json_stringify`, `toml_parse`, `toml_stringify` | Exposed |
| Host subprocess execution | `run_process`, `run_process_stream` | Exposed |
| Basic HTTP | `http_get`, `http_post`, `http_request`, `http_download` | Exposed |

| Surface | Rhai helpers | Status |
| --- | --- | --- |
| Config | `config_raw`, `config_effective`, `config_get` | Exposed |
| Tasks | `run_task`, `tasks_list`, `task_resolve`, `task_info`, `catalog_tasks` | Exposed |
| Container lifecycle | `container_up`, `container_down`, `container_shell`, `container_exec` | Exposed |
| Container inspection | `container_status`, `container_status_all`, `container_logs`, `container_stats_all` | Exposed |
| Container data | `container_data_list`, `container_data_export`, `container_data_import`, `container_data_pull_production` | Exposed |
| Container reset/eject | `container_reset`, `container_eject` | Exposed |
| Docs checks | `docs_check_links`, `docs_check_json_examples`, `docs_check_headings`, `docs_check_paths`, `docs_check_contains`, `docs_check_forbidden`, `docs_check_index`, `docs_check_next_action`, `docs_check_workflow_paths`, `docs_add_log_index` | Exposed |
| Bundles | `bundle_list`, `bundle_inspect`, `bundle_export` | Exposed |
| Services | `service_list`, `service_extract` | Exposed |
| Gateway | `gateway_status`, `gateway_setup_tls`, `gateway_up`, `gateway_down` | Exposed |
| Doctor | `doctor` | Exposed |
| Scan | `scan_god_files`, `scan_large_files`, `scan_generated`, `scan_generated_assets`, `scan_generated_in_src`, `scan_duplicate_blocks`, `scan_comment_ratio`, `scan_attention_markers`, `scan_stale_suppressions` | Exposed |
| Cache | `cache_inspect`, `cache_invalidate` | Exposed |

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
