# 026 - JSON Payload Examples

This guide provides realistic pretty-printed JSON payload samples for Effigy command contracts.

Use it when you already know JSON mode exists and now need to see what a real
payload looks like before wiring assertions or integrations.

All examples assume canonical JSON mode:

```sh
effigy --json <command>
```

At runtime, these payloads are returned inside the top-level `effigy.command.v1`
envelope in `result` (or in `error.details` for certain failures). That
envelope also carries shared `binary` metadata so tooling can distinguish the
shipped semver from a stamped local build.

## Start Here

Use this page in two passes:

1. confirm the outer envelope shape in `effigy.command.v1`
2. jump straight to the payload family you are integrating against

Start with the family that matches your job:

- task discovery or routing:
  `Tasks`, `Task Status`, `Doctor Explain`, `Task Run`
- health, repo checks, or diagnostics:
  `Doctor` and the `Scan *` payloads
- test automation:
  `Test Plan`, `Test Results`, `Watch`
- bootstrap and setup:
  `Bootstrap`, `Deploy Model`, `Init`, `Migrate`, `Config`, `Unlock`
- secret declarations and diagnostics:
  `Secrets`
- layered state, seed, and migration planning:
  `State Stack Lineage`
- shell completion or editor integration:
  `Completion`, `Completion Candidates`
- agent-facing repo context:
  `Graph Status`, `Graph Explore`, `Graph Context`, `Graph Affected`, `Graph Watch`

Companion references:

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`076-code-graph-and-agent-workflows.md`](./076-code-graph-and-agent-workflows.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)

### 4) Graph Status (`effigy.graph.status.v1`)

```json
{
  "schema": "effigy.graph.status.v1",
  "schema_version": 1,
  "command": "graph status",
  "repo_root": "/workspace/app",
  "payload": {
    "ready": true,
    "index_present": true,
    "db_path": "/workspace/app/.effigy/graph/graph.db",
    "storage_schema_version": 2,
    "counts": {
      "files": 3184,
      "symbols": 30800,
      "edges": 138885,
      "references": 62856,
      "diagnostics": 6,
      "extractors": 5,
      "index_runs": 12
    },
    "stale_paths": [],
    "new_paths": [],
    "changed_paths": [],
    "deleted_paths": [],
    "skipped_paths": [],
    "failed_paths": [],
    "extractors": [
      {
        "id": "rust",
        "version": "1.0.0",
        "languages": ["rust"],
        "capabilities": ["symbols", "references", "calls", "imports"]
      }
    ]
  }
}
```

### 5) Graph Explore (`effigy.graph.explore.v1`)

```json
{
  "schema": "effigy.graph.explore.v1",
  "schema_version": 1,
  "command": "graph explore",
  "repo_root": "/workspace/app",
  "payload": {
    "query": "trace release orchestrator",
    "index": {
      "freshness": {
        "stale": false,
        "stale_paths": []
      },
      "counts": {
        "files": 3184,
        "symbols": 30800,
        "edges": 138885,
        "references": 62856,
        "diagnostics": 6,
        "extractors": 5,
        "index_runs": 12
      }
    },
    "summary": "Release orchestration is owned in src/runner/release_command.rs with supporting docs in docs/guides/051-release-orchestration.md.",
    "primary": [
      {
        "kind": "symbol",
        "record_id": "symbol:rust:crate::runner::run_release",
        "path": "src/runner/release_command.rs",
        "language_id": "rust",
        "name": "run_release",
        "rank": 1,
        "score": 18,
        "reasons": [
          "query term matched symbol name",
          "release path matched request language and path bias"
        ],
        "snippet": "pub fn run_release(...) { ... }",
        "snippet_truncated": false
      }
    ],
    "excerpts": [
      {
        "path": "src/runner/release_command.rs",
        "language_id": "rust",
        "name": "run_release",
        "role": "primary-owner",
        "section_kind": "function",
        "completeness": "complete-section",
        "score": 18,
        "reasons": [
          "primary owner excerpt"
        ],
        "text": "pub fn run_release(...) { ... }",
        "truncated": false
      }
    ],
    "relations": [
      {
        "kind": "file",
        "path": "docs/guides/051-release-orchestration.md",
        "name": null,
        "reason": "bounded doc neighbor for release orchestration query"
      }
    ],
    "overflow": {
      "omitted_items": 2,
      "omitted_files": 1,
      "omitted_symbols": 1,
      "omitted_docs": 0,
      "byte_budget": 12288,
      "used_bytes": 6104
    },
    "guidance": [
      "trust excerpts for first-pass orientation",
      "use rg for exact token verification before editing"
    ]
  }
}
```

### 6) Graph Context (`effigy.graph.context.v1`)

```json
{
  "schema": "effigy.graph.context.v1",
  "schema_version": 1,
  "command": "graph context",
  "repo_root": "/workspace/app",
  "payload": {
    "request": "trace release orchestrator",
    "freshness": {
      "stale": false,
      "stale_paths": []
    },
    "items": [
      {
        "kind": "symbol",
        "record_id": "symbol:rust:crate::runner::run_release",
        "path": "src/runner/release_command.rs",
        "language_id": "rust",
        "name": "run_release",
        "range": {
          "start": { "line": 14, "column": 0, "byte": 322 },
          "end": { "line": 81, "column": 1, "byte": 2114 }
        },
        "rank": 1,
        "score": 18,
        "reasons": [
          "query term matched symbol name",
          "release path matched request language and path bias"
        ],
        "provenance": {
          "extractor_id": "rust",
          "extractor_version": "1.0.0",
          "source_path": "src/runner/release_command.rs",
          "confidence": "syntactic",
          "detail": "tree-sitter pass"
        },
        "snippet": "pub fn run_release(...) { ... }",
        "snippet_truncated": false
      }
    ],
    "overflow": {
      "omitted_items": 3,
      "omitted_files": 1,
      "omitted_symbols": 2,
      "omitted_docs": 0,
      "byte_budget": 4096,
      "used_bytes": 3012
    },
    "notes": [
      "language filter: rust",
      "path filter: src/runner"
    ]
  }
}
```

### 7) Graph Affected (`effigy.graph.affected.v1`)

```json
{
  "schema": "effigy.graph.affected.v1",
  "schema_version": 1,
  "command": "graph affected",
  "repo_root": "/workspace/app",
  "payload": {
    "changed_paths": ["src/runner/graph_command.rs"],
    "freshness": {
      "stale": false,
      "stale_paths": []
    },
    "depth": 2,
    "affected_files": [
      {
        "path": "src/runner/graph_command.rs",
        "language_id": "rust",
        "confidence": "exact",
        "reasons": [
          "changed input path"
        ]
      },
      {
        "path": "src/tests/runner_tests/runner_core_tests/graph_tests.rs",
        "language_id": "rust",
        "confidence": "exact",
        "reasons": [
          "incoming `contains` reaches changed node"
        ]
      }
    ],
    "likely_test_files": [
      "src/tests/runner_tests/runner_core_tests/graph_tests.rs"
    ],
    "likely_test_tasks": [
      {
        "name": "test",
        "kind": "task",
        "path": "effigy.toml",
        "confidence": "heuristic",
        "reasons": [
          "manifest task name suggests validation coverage"
        ]
      }
    ],
    "notes": [
      "affected output is bounded evidence, not exhaustive proof"
    ]
  }
}
```

### 8) Graph Watch Event (`effigy.graph.watch.event.v1`)

`graph watch --json` is newline-delimited event output, not a one-shot envelope.

```json
{
  "schema": "effigy.graph.watch.event.v1",
  "schema_version": 1,
  "command": "graph watch",
  "repo_root": "/workspace/app",
  "payload": {
    "kind": "refresh",
    "debounce_ms": 1000,
    "changed_paths": ["src/lib.rs"],
    "dirty": false,
    "refresh_duration_ms": 231,
    "index": {
      "indexed_files": 3184,
      "extractor_count": 5,
      "counts": {
        "files": 3184,
        "symbols": 30801,
        "edges": 138887,
        "references": 62857,
        "diagnostics": 6,
        "extractors": 5,
        "index_runs": 13
      },
      "stale_paths": [],
      "new_paths": [],
      "changed_paths": ["src/lib.rs"],
      "deleted_paths": [],
      "skipped_paths": [],
      "failed_paths": []
    },
    "notes": []
  }
}
```

## How To Use This Guide

- treat the samples here as shape examples, not a promise that every field
  ordering choice matters
- use [`017-json-output-contracts.md`](./017-json-output-contracts.md) for the
  formal schema and contract rules
- use this guide when you want realistic payloads before writing assertions,
  adapters, or fixtures

## Envelope and Core Routing

### 1) Envelope Example (`effigy.command.v1`)

```json
{
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "ok": true,
  "binary": {
    "name": "effigy",
    "version": "0.5.0",
    "active_version": "v0.5.0+local.abc123",
    "display_version": "v0.5.0+local.abc123"
  },
  "command": {
    "kind": "tasks",
    "name": "tasks"
  },
  "result": {
    "schema": "effigy.tasks.v1",
    "schema_version": 1,
    "root": "/workspace/app",
    "catalog_tasks": [],
    "managed_profiles": [],
    "builtin_tasks": []
  },
  "error": null
}
```

`binary.version` is the stable release semver. `binary.active_version` and
`binary.display_version` can include local build identity such as
`+local.<hash>`.

### 2) Tasks (`effigy.tasks.v1`)

```json
{
  "schema": "effigy.tasks.v1",
  "schema_version": 1,
  "root": "/workspace/app",
  "catalog_tasks": [
    {
      "catalog": "api",
      "task": "build",
      "run": "cargo run -p api --bin build"
    }
  ],
  "managed_profiles": [
    {
      "task": "dev",
      "profile": "default",
      "parent_task": "dev",
      "invocation": "effigy dev",
      "run": "api,worker"
    }
  ],
  "builtin_tasks": [
    {
      "task": "doctor",
      "description": "Built-in remedial health checks for environment, manifests, and task references"
    }
  ],
  "catalogs": [
    {
      "alias": "api",
      "root": "/workspace/app/services/api",
      "manifest": "/workspace/app/services/api/effigy.toml"
    }
  ],
  "precedence": [
    "explicit catalog alias prefix",
    "relative/absolute catalog path prefix",
    "unprefixed nearest in-scope catalog by cwd",
    "unprefixed shallowest catalog from workspace root"
  ],
  "resolve": {
    "status": "ok",
    "selector": "api/build",
    "catalog": "api",
    "task": "build",
    "mode": "explicit_prefix",
    "evidence": [
      "selected catalog via explicit prefix `api`"
    ],
    "lock_scopes": [
      "task:build"
    ]
  }
}
```

### 3) Deploy Model (`deploy.model.v1`)

```json
{
  "schema": "deploy.model.v1",
  "schema_version": 1,
  "app": {
    "name": "workspace-app-reference",
    "bundle": "workspace-app",
    "project_name": "workspace-app-reference-dev",
    "source_root": "."
  },
  "services": [
    {
      "name": "front",
      "role": "static",
      "runtime": "node",
      "source_root": "acme-front",
      "build": {
        "command": "bun x vite build"
      },
      "output": {
        "kind": "directory",
        "path": "build",
        "fallback": "200.html"
      },
      "domains": [
        "acme.test"
      ],
      "env": {},
      "secret_refs": [],
      "volumes": [],
      "warnings": []
    },
    {
      "name": "api",
      "role": "web",
      "runtime": "rust",
      "source_root": "acme-api",
      "build": {
        "command": "cargo build --release"
      },
      "start": {
        "command": "cargo run -p acme-api"
      },
      "release": {
        "command": "cargo run -p acme-db --bin migrate_dev_db"
      },
      "health": {
        "kind": "http",
        "path": "/v1/health"
      },
      "port": 41001,
      "domains": [
        "api.acme.test"
      ],
      "env": {},
      "secret_refs": [
        "DATABASE_URL"
      ],
      "volumes": [],
      "warnings": []
    }
  ],
  "backing_services": [
    {
      "name": "postgres",
      "kind": "postgres",
      "mode": "managed",
      "required": true,
      "consumers": [
        "api"
      ],
      "warnings": []
    }
  ],
  "domains": [
    {
      "host": "acme.test",
      "service": "front",
      "tls": "provider_managed"
    },
    {
      "host": "api.acme.test",
      "service": "api",
      "tls": "provider_managed"
    }
  ],
  "secrets": [
    {
      "name": "DATABASE_URL",
      "services": [
        "api"
      ],
      "required": true,
      "source": "operator",
      "notes": "Managed Postgres connection string for primary database `acme`"
    }
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/acowtancy-uat/latest-apply.json",
  "written_history_path": ".effigy/reports/state/acowtancy-uat/history/20260508T143013Z-apply-acowtancy-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

### 3) Task Status (`effigy.tasks-status.v1`)

```json
{
  "schema": "effigy.tasks-status.v1",
  "schema_version": 1,
  "resolved_selector": "catalog_a/build",
  "selected_catalog_root": "/workspace/app/catalog_a",
  "state": "succeeded",
  "currently_declared": true,
  "active": null,
  "latest": {
    "status_key": "catalog-a-build-2db43d3e62fd7311",
    "identity": {
      "repo_root": "/workspace/app",
      "selected_catalog_root": "/workspace/app/catalog_a",
      "resolved_selector": "catalog_a/build",
      "resolved_task_name": "build"
    },
    "state": "succeeded",
    "stage": "finishing",
    "execution_surface": "direct-cli",
    "runtime_route": {
      "route": "host"
    },
    "started_at": "20260510T190000Z",
    "finished_at": "20260510T190005Z",
    "duration_ms": 5000,
    "lock_scopes": [
      "task:catalog_a/build"
    ],
    "outcome": {
      "summary": "task completed"
    },
    "latest_report_path": ".effigy/reports/tasks/catalog-a-build-2db43d3e62fd7311/latest.json",
    "history_report_path": ".effigy/reports/tasks/catalog-a-build-2db43d3e62fd7311/history/20260510t190005z-succeeded.json"
  },
  "stale_active": null,
  "warnings": [],
  "routing": {
    "repo_root": "/workspace/app",
    "catalog_alias": "catalog_a",
    "catalog_root": "/workspace/app/catalog_a",
    "catalog_manifest_path": "/workspace/app/catalog_a/effigy.toml",
    "selection_mode": "explicit-prefix",
    "evidence": [
      "selected catalog `catalog_a` by explicit prefix"
    ]
  }
}
```

### 4) Task Status Inventory (`effigy.tasks-status-all.v1`)

```json
{
  "schema": "effigy.tasks-status-all.v1",
  "schema_version": 1,
  "scope_root": "/workspace/app",
  "catalog_scopes": [
    {
      "alias": "root",
      "root": "/workspace/app",
      "manifest": "/workspace/app/effigy.toml",
      "depth": 0
    },
    {
      "alias": "catalog_a",
      "root": "/workspace/app/catalog_a",
      "manifest": "/workspace/app/catalog_a/effigy.toml",
      "depth": 1
    }
  ],
  "counts_by_state": {
    "running": 1,
    "succeeded": 1,
    "unknown": 1,
    "failed": 1
  },
  "warnings": [],
  "rows": [
    {
      "selector": "test",
      "selected_catalog_root": "root",
      "state": "running",
      "currently_declared": true,
      "last_updated": "20260510T190000Z",
      "route": "host",
      "active": {
        "status_key": "root-test-4f5e64bf373f4d3c",
        "identity": {
          "repo_root": "/workspace/app",
          "selected_catalog_root": "/workspace/app",
          "resolved_selector": "test",
          "resolved_task_name": "test"
        },
        "state": "running",
        "stage": "executing",
        "execution_surface": "direct-cli",
        "runtime_route": {
          "route": "host"
        },
        "owner_pid": 12345,
        "started_at": "20260510T185955Z",
        "updated_at": "20260510T190000Z",
        "lock_scopes": [
          "task:test"
        ],
        "active_record_path": ".effigy/runtime/tasks/active/root-test-4f5e64bf373f4d3c.json"
      },
      "latest": null,
      "stale_active": null,
      "warnings": []
    },
    {
      "selector": "catalog_a/build",
      "selected_catalog_root": "catalog_a",
      "state": "succeeded",
      "currently_declared": true,
      "last_updated": "20260510T185930Z",
      "route": "host",
      "active": null,
      "latest": {
        "status_key": "catalog-a-build-2db43d3e62fd7311",
        "identity": {
          "repo_root": "/workspace/app",
          "selected_catalog_root": "/workspace/app/catalog_a",
          "resolved_selector": "catalog_a/build",
          "resolved_task_name": "build"
        },
        "state": "succeeded",
        "stage": "finishing",
        "execution_surface": "direct-cli",
        "runtime_route": {
          "route": "host"
        },
        "started_at": "20260510T185925Z",
        "finished_at": "20260510T185930Z",
        "duration_ms": 5000,
        "lock_scopes": [
          "task:catalog_a/build"
        ],
        "outcome": {
          "summary": "task completed"
        },
        "latest_report_path": ".effigy/reports/tasks/catalog-a-build-2db43d3e62fd7311/latest.json",
        "history_report_path": ".effigy/reports/tasks/catalog-a-build-2db43d3e62fd7311/history/20260510t185930z-succeeded.json"
      },
      "stale_active": null,
      "warnings": []
    },
    {
      "selector": "idle",
      "selected_catalog_root": "root",
      "state": "unknown",
      "currently_declared": true,
      "last_updated": null,
      "route": null,
      "active": null,
      "latest": null,
      "stale_active": null,
      "warnings": []
    },
    {
      "selector": "old-task",
      "selected_catalog_root": "root",
      "state": "failed",
      "currently_declared": false,
      "no_longer_declared": true,
      "last_updated": "20260510T185900Z",
      "route": "host",
      "active": null,
      "latest": {
        "status_key": "root-old-task-1513d23936e3152d",
        "identity": {
          "repo_root": "/workspace/app",
          "selected_catalog_root": "/workspace/app",
          "resolved_selector": "old-task",
          "resolved_task_name": "old-task"
        },
        "state": "failed",
        "stage": "finishing",
        "execution_surface": "direct-cli",
        "runtime_route": {
          "route": "host"
        },
        "started_at": "20260510T185855Z",
        "finished_at": "20260510T185900Z",
        "duration_ms": 5000,
        "lock_scopes": [
          "task:old-task"
        ],
        "outcome": {
          "summary": "old task failed"
        },
        "latest_report_path": ".effigy/reports/tasks/root-old-task-1513d23936e3152d/latest.json",
        "history_report_path": ".effigy/reports/tasks/root-old-task-1513d23936e3152d/history/20260510t185900z-failed.json"
      },
      "stale_active": null,
      "warnings": []
    }
  ]
}
```

### 4) Deploy Export (`effigy.deploy.export.v1`)

```json
{
  "schema": "effigy.deploy.export.v1",
  "schema_version": 1,
  "provider": "railway",
  "plan": true,
  "path": "/workspace/app/infra/railway",
  "files": [
    "services/front/railway.toml",
    "services/admin/railway.toml",
    "services/api/railway.toml",
    "report.json"
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/acowtancy-uat/latest-apply.json",
  "written_history_path": ".effigy/reports/state/acowtancy-uat/history/20260508T143014Z-apply-acowtancy-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

### Deploy Plan (`effigy.deploy.plan.v1`)

```json
{
  "schema": "effigy.deploy.plan.v1",
  "schema_version": 1,
  "env": "uat",
  "provider": "railway",
  "app": {
    "name": "acowtancy",
    "project_name": "acowtancy-uat"
  },
  "code": {
    "requested_ref": "branch:main",
    "resolved_commit": "abc1234"
  },
  "release_policy": {
    "mode": "optional",
    "required": false,
    "gates_required": false
  },
  "state": {
    "stack": "uat",
    "lineage_id": "acowtancy-uat-Uat-structure-baseline-seed-legacy-import",
    "planned_report_path": ".effigy/reports/state/acowtancy-uat/latest-plan.json"
  },
  "artifact_policy": {
    "mode": "digest-preferred",
    "blockers": []
  },
  "provider_preflight": {
    "status": "planned",
    "checks": [
      {
        "name": "project",
        "status": "pending",
        "target": "acowtancy-uat"
      }
    ]
  },
  "hooks": [
    {
      "stage": "after_deploy",
      "task": "deploy:uat:smoke"
    }
  ],
  "health_checks": [
    {
      "service": "api",
      "kind": "http",
      "path": "/v1/health"
    }
  ],
  "warnings": [],
  "blockers": []
}
```

### Deploy Apply (`effigy.deploy.apply.v1`)

```json
{
  "schema": "effigy.deploy.apply.v1",
  "schema_version": 1,
  "deployment_id": "20260510T183000Z-acowtancy-uat-abc1234",
  "env": "uat",
  "provider": "railway",
  "status": "succeeded",
  "started_at": "2026-05-10T18:30:00Z",
  "finished_at": "2026-05-10T18:37:42Z",
  "code": {
    "requested_ref": "branch:main",
    "resolved_commit": "abc1234"
  },
  "state": {
    "status": "succeeded",
    "lineage_id": "acowtancy-uat-Uat-structure-baseline-seed-legacy-import",
    "apply_report_path": ".effigy/reports/state/acowtancy-uat/latest-apply.json"
  },
  "provider_operation": {
    "status": "succeeded",
    "provider_deployment_id": "railway-deploy-123",
    "services": [
      "front",
      "admin",
      "api",
      "jobs"
    ]
  },
  "hooks": [
    {
      "stage": "after_deploy",
      "task": "deploy:uat:smoke",
      "status": "succeeded"
    }
  ],
  "health_checks": [
    {
      "service": "api",
      "status": "succeeded",
      "path": "/v1/health"
    }
  ],
  "written_report_path": ".effigy/reports/deploy/uat/latest.json",
  "written_history_path": ".effigy/reports/deploy/uat/history/20260510T183000Z-acowtancy-uat-abc1234.json"
}
```

## Health and Diagnostics

### 5) Doctor (`effigy.doctor.v1`)

```json
{
  "schema": "effigy.doctor.v1",
  "schema_version": 1,
  "ok": false,
  "summary": {
    "errors": 1,
    "warnings": 1,
    "fixes_applied": 0
  },
  "sections": [
    {
      "check_id": "scan.god-files",
      "severity": "error",
      "findings": [
        {
          "severity": "warning",
          "evidence": "274 code lines (340 total) [warning] src/ui/dashboard.tsx"
        },
        {
          "severity": "error",
          "evidence": "512 code lines (588 total) [high] src/server/router.ts"
        }
      ]
    }
  ],
  "findings": [
    {
      "id": "scan.god-files",
      "level": "warning",
      "message": "274 code lines (340 total) [warning] src/ui/dashboard.tsx"
    },
    {
      "id": "scan.god-files",
      "level": "error",
      "message": "512 code lines (588 total) [high] src/server/router.ts"
    },
    {
      "id": "catalogs.discovered",
      "level": "warning",
      "message": "Discovered 3 catalogs across workspace"
    }
  ],
  "fixes": [],
  "root_resolution": {
    "invocation_cwd": "/workspace/app",
    "resolved_root": "/workspace/app",
    "mode": "nearest-marker"
  }
}
```

`effigy doctor` preserves scanner-backed warning/high/critical findings in its own report model even when plain `effigy scan god-files` text output hides warning rows by default.

The same normalization applies to `scan.duplicate-blocks`, `scan.comment-ratio`, `scan.generated-assets`, `scan.generated-in-src`, and `scan.attention-markers` when those scanners are enabled for doctor.

### 4) Doctor Explain (`effigy.doctor.explain.v1`)

```json
{
  "schema": "effigy.doctor.explain.v1",
  "schema_version": 1,
  "request": {
    "task": "api/build",
    "args": [
      "--",
      "--watch"
    ]
  },
  "root_resolution": {
    "invocation_cwd": "/workspace/app",
    "resolved_root": "/workspace/app",
    "mode": "nearest-marker"
  },
  "selection": {
    "status": "ok",
    "catalog": "api",
    "task": "build",
    "mode": "explicit_prefix",
    "evidence": [
      "selected catalog by explicit task prefix"
    ]
  },
  "candidates": [
    {
      "catalog": "api",
      "path": "/workspace/app/services/api/effigy.toml",
      "matched": true
    }
  ],
  "ambiguity_candidates": [],
  "deferral": {
    "considered": false,
    "selected": false,
    "source": null
  },
  "reasoning": {
    "selection": "selected catalog by explicit task prefix",
    "deferral": "selection resolved without deferral"
  }
}
```

## Test and Watch Payloads

### 5) Test Plan (`effigy.test.plan.v1`)

```json
{
  "schema": "effigy.test.plan.v1",
  "schema_version": 1,
  "request": "test",
  "root": "/workspace/app",
  "runtime": "text",
  "requested_suite": null,
  "passthrough": [],
  "targets": [
    {
      "name": "api",
      "root": "/workspace/app/services/api",
      "suite_source": "auto-detect",
      "cargo_env_match": "prefix-aware",
      "available_suites": [
        "vitest",
        "cargo-nextest"
      ],
      "fallback_chain": [
        "vitest markers detected",
        "cargo-nextest fallback available"
      ],
      "plans": [
        {
          "suite": "vitest",
          "command": "bun x vitest run",
          "evidence": [
            "package marker: package.json",
            "local binary detected: node_modules/.bin/vitest"
          ]
        }
      ]
    }
  ]
}
```

### 6) Test Results (`effigy.test.results.v1`)

```json
{
  "schema": "effigy.test.results.v1",
  "schema_version": 1,
  "ok": false,
  "requested_suite": "vitest",
  "passthrough": [
    "user-service"
  ],
  "targets": [
    {
      "name": "api/vitest",
      "root": "/workspace/app/services/api",
      "runner": "vitest",
      "cargo_env_match": "prefix-aware",
      "command": "bun x vitest run user-service",
      "success": false,
      "code": 1,
      "duration_ms": 893
    }
  ],
  "failures": [
    {
      "name": "api/vitest",
      "code": 1
    }
  ],
  "hint": {
    "kind": "selected-suite-filter-no-match",
    "message": "No targets matched the selected suite in one or more catalogs"
  }
}
```

### 7) Watch (`effigy.watch.v1`)

```json
{
  "schema": "effigy.watch.v1",
  "schema_version": 1,
  "ok": true,
  "runs": 1
}
```

## Setup, Bootstrap, and Config

### 8) Bootstrap (`effigy.bootstrap.v1`)

```json
{
  "schema": "effigy.bootstrap.v1",
  "schema_version": 1,
  "ok": true,
  "phase": "executed",
  "repo_url": "git@github.com:inflatable-cookie/loophole.git",
  "repo_name": "loophole",
  "destination": "/workspace/sandboxes/loophole",
  "destination_source": "cwd-default",
  "branch": "main",
  "root": {
    "repo": "git@github.com:inflatable-cookie/loophole.git",
    "repo_name": "loophole",
    "destination": "/workspace/sandboxes/loophole",
    "destination_source": "cwd-default",
    "requested_branch": "main",
    "repo_state": "cloned",
    "update_strategy": "branch"
  },
  "root_repo_state": "cloned",
  "manifest_found": true,
  "manifest": {
    "path": "/workspace/sandboxes/loophole/effigy.toml",
    "file_found": true,
    "bootstrap_contract_found": true
  },
  "submodules": {
    "policy": "recursive",
    "applied": true
  },
  "children": [
    {
      "path": "aura",
      "destination": "/workspace/sandboxes/loophole/aura",
      "repo": "git@github.com:inflatable-cookie/aura.git",
      "requested_branch": "main",
      "required": true,
      "repo_state": "cloned",
      "setup": [
        "install"
      ],
      "warning": null
    },
    {
      "path": "chorus",
      "destination": "/workspace/sandboxes/loophole/chorus",
      "repo": "git@github.com:inflatable-cookie/chorus.git",
      "requested_branch": null,
      "required": false,
      "repo_state": "failed",
      "setup": [],
      "warning": "optional child `chorus` failed: bootstrap destination remote mismatch: expected `git@github.com:inflatable-cookie/chorus.git`, found `git@github.com:someone-else/chorus.git`"
    }
  ],
  "setup": {
    "root": [
      "bootstrap:local",
      "doctor"
    ],
    "children": [
      {
        "path": "aura",
        "repo": "git@github.com:inflatable-cookie/aura.git",
        "required": true,
        "repo_state": "cloned",
        "setup": [
          "install"
        ],
        "warning": null
      },
      {
        "path": "chorus",
        "repo": "git@github.com:inflatable-cookie/chorus.git",
        "required": false,
        "repo_state": "failed",
        "setup": [],
        "warning": "optional child `chorus` failed: bootstrap destination remote mismatch: expected `git@github.com:inflatable-cookie/chorus.git`, found `git@github.com:someone-else/chorus.git`"
      }
    ]
  },
  "start": {
    "requested": true,
    "task": "aura/dev",
    "ran": true
  },
  "warnings": [
    "optional child `chorus` failed: bootstrap destination remote mismatch: expected `git@github.com:inflatable-cookie/chorus.git`, found `git@github.com:someone-else/chorus.git`"
  ],
  "display": "bootstrapped git@github.com:inflatable-cookie/loophole.git -> /workspace/sandboxes/loophole"
}
```

Use `phase = "plan"` payloads when you need destination/branch/start intent
without mutation. Use `phase = "executed"` when you need to assert what was
actually cloned, updated, or started.

Read these fields first:

- `root` for the root checkout decision and final root repo state
- `manifest` to distinguish “no `effigy.toml`” from “manifest exists but has no
  `[bootstrap]` contract”
- `children` for per-child destination, branch, repo state, and warning detail
- `start` for whether bring-up launch was requested and whether it actually ran

## Scan Payloads

### 9) Scan God Files (`effigy.scan.god-files.v1`)

```json
{
  "schema": "effigy.scan.god-files.v1",
  "schema_version": 1,
  "scan": "god-files",
  "format": "text",
  "root": "/workspace/app",
  "thresholds": {
    "warn": 250,
    "high": 400,
    "critical": 700
  },
  "scanned_files": 38,
  "skipped_generated": 4,
  "finding_count": 2,
  "fail_on_findings": false,
  "respect_gitignore": true,
  "output_path": null,
  "findings": [
    {
      "path": "src/server/router.ts",
      "code_lines": 512,
      "total_lines": 588,
      "severity": "high"
    },
    {
      "path": "src/ui/dashboard.tsx",
      "code_lines": 274,
      "total_lines": 340,
      "severity": "warning"
    }
  ],
  "text": "God Files\n\nroot: /workspace/app\nthresholds: warn=250 high=400 critical=700\nscanned-files: 38  skipped-generated: 4  findings: 2\nseverity-counts: critical=0 high=1 warning=1\nwarning-rows-hidden: 1  use --show-warnings to list them\n\nFindings\nhigh  512 code lines (588 total)  src/server/router.ts"
}
```

### 10) Scan Duplicate Blocks (`effigy.scan.duplicate-blocks.v1`)

```json
{
  "schema": "effigy.scan.duplicate-blocks.v1",
  "schema_version": 1,
  "scan": "duplicate-blocks",
  "format": "text",
  "root": "/workspace/app",
  "thresholds": {
    "warn": 20,
    "high": 40,
    "critical": 80,
    "min_occurrences": 2
  },
  "scanned_files": 41,
  "candidate_blocks": 182,
  "finding_count": 2,
  "fail_on_findings": false,
  "respect_gitignore": true,
  "output_path": null,
  "findings": [
    {
      "severity": "high",
      "block_lines": 46,
      "occurrences": 2,
      "fingerprint": "9d17d7041dd1d8b2",
      "snippet": "pub fn build_input(payload: Value) -> Result<Input, AppError> { let title = payload.title(); let slug = payload.slug();",
      "locations": [
        {
          "path": "src/admin/build_input.rs",
          "start_line": 7,
          "end_line": 52
        },
        {
          "path": "src/user/build_input.rs",
          "start_line": 9,
          "end_line": 54
        }
      ]
    },
    {
      "severity": "warning",
      "block_lines": 24,
      "occurrences": 2,
      "fingerprint": "2a35fc8ea8211136",
      "snippet": "import type { PageLoad } from \"./$types\"; import { error } from \"@sveltejs/kit\";",
      "locations": [
        {
          "path": "src/routes/learn/+page.ts",
          "start_line": 1,
          "end_line": 24
        },
        {
          "path": "src/routes/revise/+page.ts",
          "start_line": 1,
          "end_line": 24
        }
      ]
    }
  ],
  "text": "Duplicate Blocks\n\nroot: /workspace/app\nthresholds: warn=20 high=40 critical=80 min-occurrences=2\nscanned-files: 41  candidate-blocks: 182  findings: 2\nseverity-counts: critical=0 high=1 warning=1\nwarning-rows-hidden: 1  use --show-warnings to list them\n\nFindings\nhigh  46 lines  2 occurrences  pub fn build_input(payload: Value) -> Result<Input, AppError> { let title = payload.title(); let slug = payload.slug();  [src/admin/build_input.rs:7-52, src/user/build_input.rs:9-54]"
}
```

### 11) Scan Comment Ratio (`effigy.scan.comment-ratio.v1`)

```json
{
  "schema": "effigy.scan.comment-ratio.v1",
  "schema_version": 1,
  "scan": "comment-ratio",
  "format": "text",
  "root": "/workspace/app",
  "thresholds": {
    "warn": 1.5,
    "high": 2.0,
    "critical": 3.0,
    "min_code_lines": 20
  },
  "scanned_files": 41,
  "candidate_files": 28,
  "finding_count": 2,
  "fail_on_findings": false,
  "respect_gitignore": true,
  "output_path": null,
  "findings": [
    {
      "path": "src/dev_server.rs",
      "code_lines": 60,
      "comment_lines": 144,
      "ratio": 2.4,
      "severity": "high"
    },
    {
      "path": "src/lib.rs",
      "code_lines": 38,
      "comment_lines": 57,
      "ratio": 1.5,
      "severity": "warning"
    }
  ],
  "text": "Comment Ratio\n\nroot: /workspace/app\nthresholds: warn=1.50 high=2.00 critical=3.00 min-code-lines=20\nscanned-files: 41  candidate-files: 28  findings: 2\nseverity-counts: critical=0 high=1 warning=1\nwarning-rows-hidden: 1  use --show-warnings to list them\n\nFindings\nhigh  ratio=2.40  144 comment / 60 code  src/dev_server.rs"
}
```

### 12) Scan Generated Assets (`effigy.scan.generated-assets.v1`)

```json
{
  "schema": "effigy.scan.generated-assets.v1",
  "schema_version": 1,
  "scan": "generated-assets",
  "format": "text",
  "root": "/workspace/app",
  "thresholds": {
    "warn": 1000000,
    "high": 5000000,
    "critical": 20000000
  },
  "scanned_files": 6,
  "finding_count": 2,
  "fail_on_findings": false,
  "respect_gitignore": true,
  "output_path": null,
  "findings": [
    {
      "path": "dist/app.min.js",
      "bytes": 1840000,
      "severity": "warning",
      "reason": "vendor-or-build-path"
    },
    {
      "path": "vendor/runtime.wasm",
      "bytes": 6200000,
      "severity": "high",
      "reason": "vendor-or-build-path"
    }
  ],
  "text": "Generated Assets\n\nroot: /workspace/app\nthresholds-bytes: warn=1000000 high=5000000 critical=20000000\nscanned-files: 6  findings: 2\nseverity-counts: critical=0 high=1 warning=1\nwarning-rows-hidden: 1  use --show-warnings to list them\n\nFindings\nhigh  6.2 MB  vendor/runtime.wasm  [vendor-or-build-path]"
}
```

### 13) Scan Generated In Src (`effigy.scan.generated-in-src.v1`)

```json
{
  "schema": "effigy.scan.generated-in-src.v1",
  "schema_version": 1,
  "scan": "generated-in-src",
  "format": "text",
  "root": "/workspace/app",
  "thresholds": {
    "warn": 1,
    "high": 20000,
    "critical": 200000
  },
  "source_roots": [
    "src/**",
    "app/**",
    "lib/**",
    "crates/**",
    "packages/*/src/**"
  ],
  "scanned_files": 18,
  "candidate_files": 2,
  "finding_count": 2,
  "fail_on_findings": false,
  "respect_gitignore": true,
  "output_path": null,
  "findings": [
    {
      "path": "src/generated/client.generated.ts",
      "category": "generated-path",
      "severity": "warning",
      "reason": "generated-path-component",
      "size_bytes": 6400
    },
    {
      "path": "src/graphql/schema.generated.ts",
      "category": "generated-filename",
      "severity": "high",
      "reason": "filename-marker",
      "size_bytes": 52000
    }
  ],
  "text": "Generated In Src\n\nroot: /workspace/app\nthresholds-bytes: warn=1 high=20000 critical=200000\nsource-roots: src/**, app/**, lib/**, crates/**, packages/*/src/**\nscanned-files: 18  candidate-files: 2  findings: 2\nseverity-counts: critical=0 high=1 warning=1\nwarning-rows-hidden: 1  use --show-warnings to list them\n\nFindings\nhigh  52.0 KB  src/graphql/schema.generated.ts  [generated-filename] [filename-marker]"
}
```

### 14) Init (`effigy.init.v1`, `effigy.init.checklist.v1`, `effigy.init.actions.v1`, `effigy.init.list.v1`)

Baseline managed init (`effigy init --check --json` / `--apply --json` / `--repair --json`):

```json
{
  "schema": "effigy.init.v1",
  "schema_version": 1,
  "ok": true,
  "mode": "check",
  "status": "ok",
  "changed": false,
  "needs_changes": true,
  "checks": [
    {
      "id": "manifest.effigy_toml",
      "path": "/workspace/app/effigy.toml",
      "status": "missing",
      "action": "create_file",
      "description": "Create a baseline effigy.toml scaffold at the repo root."
    },
    {
      "id": "agents_md.effigy_contract",
      "path": "/workspace/app/AGENTS.md",
      "status": "would_update",
      "action": "insert_managed_block",
      "description": "Insert or refresh the managed Effigy agent contract block."
    }
  ],
  "text": "Effigy init check: needs changes\n..."
}
```

Checklist inventory (`effigy init --checklist --json`):

```json
{
  "schema": "effigy.init.checklist.v1",
  "schema_version": 1,
  "ok": true,
  "mode": "checklist",
  "repo_root": "/workspace/app",
  "has_changes": true,
  "summary": {
    "total_jobs": 8,
    "applicable": 5,
    "already_satisfied": 2,
    "not_applicable": 1
  },
  "jobs": [
    {
      "id": "manifest.effigy_toml",
      "category": "baseline",
      "execution_kind": "apply",
      "safety_class": "safe_apply",
      "applicability": "applicable",
      "can_run_noninteractive": true,
      "summary": "Create the root effigy.toml scaffold.",
      "reason": "effigy.toml is missing",
      "recommended_command": "effigy init --apply"
    },
    {
      "id": "graph_status.inspect",
      "category": "graph",
      "execution_kind": "inspect",
      "safety_class": "safe_check",
      "applicability": "applicable",
      "can_run_noninteractive": true,
      "summary": "Inspect local graph freshness before code-understanding work.",
      "reason": "graph surface is always available",
      "recommended_command": "effigy graph status --json"
    }
  ]
}
```

Selected action execution (`effigy init --apply-actions <ID>[,<ID>...] --json`):

```json
{
  "schema": "effigy.init.actions.v1",
  "schema_version": 1,
  "ok": true,
  "mode": "apply_actions",
  "selected_action_ids": [
    "manifest.effigy_toml",
    "graph_status.inspect"
  ],
  "changed": true,
  "outcomes": [
    {
      "id": "manifest.effigy_toml",
      "status": "applied",
      "summary": "Create a baseline effigy.toml scaffold at the repo root.",
      "reason": "file created",
      "command": "effigy init --apply",
      "output": null
    },
    {
      "id": "graph_status.inspect",
      "status": "inspected",
      "summary": "Inspect local graph freshness before code-understanding work.",
      "reason": "command executed",
      "command": "effigy graph status --json",
      "output": "{\n  \"schema\": \"effigy.graph.status.v1\",\n  ...\n}"
    }
  ]
}
```

Notes:
- `effigy.init.v1` now covers the baseline managed setup flow and still also
  covers explicit starter emission
- checklist mode is a wider setup inventory than baseline `--check`
- action execution reports per-action `applied`, `inspected`, `guided`,
  `blocked`, `skipped`, or `failed` outcomes

Starter list / starter emission still use the starter-oriented init contracts.

`effigy init <name>` / `effigy init --dry-run` / `effigy init --force`:

```json
{
  "schema": "effigy.init.v1",
  "schema_version": 1,
  "ok": true,
  "starter": "minimal",
  "dry_run": false,
  "written": true,
  "overwritten": false,
  "files": [
    {
      "target": "effigy.toml",
      "path": "/workspace/app/effigy.toml",
      "contents": "# Baseline effigy.toml scaffold (phase 1)\n\n[tasks]\nping = \"printf ok\"\n",
      "existed": false,
      "written": true
    }
  ],
  "guidance": null
}
```

`effigy init --list`:

```json
{
  "schema": "effigy.init.list.v1",
  "schema_version": 1,
  "starters": [
    { "name": "minimal", "description": "Minimal Effigy scaffold" },
    { "name": "northstar", "description": "Northstar-profile starter" }
  ]
}
```

### 15) Migrate (`effigy.migrate.v1`)

```json
{
  "schema": "effigy.migrate.v1",
  "schema_version": 1,
  "ok": true,
  "source": "/workspace/app/package.json",
  "manifest": "/workspace/app/effigy.toml",
  "apply": false,
  "written": false,
  "added": [
    {
      "name": "test",
      "run": "vitest run"
    }
  ],
  "conflicts": [
    {
      "name": "build",
      "run": "npm run compile",
      "reason": "task already exists"
    }
  ]
}
```

### 16) Config (`effigy.config.v1`)

```json
{
  "schema": "effigy.config.v1",
  "schema_version": 1,
  "ok": true,
  "mode": "reference",
  "minimal": false,
  "target": null,
  "runner": null,
  "text": "effigy.toml Reference\n\n[defer]\nrun = \"my-process {request} {args}\"\n"
}
```

### 17) Unlock (`effigy.unlock.v1`)

```json
{
  "schema": "effigy.unlock.v1",
  "schema_version": 1,
  "ok": true,
  "root": "/workspace/app",
  "removed": [
    "workspace"
  ],
  "missing": [],
  "all": false
}
```

## Completion

### 18) Completion (`effigy.completion.v2`)

```json
{
  "schema": "effigy.completion.v2",
  "schema_version": 1,
  "ok": true,
  "shell": "bash",
  "action": "export",
  "prompted_shell": false,
  "prompted_action": false,
  "script": "# bash completion for effigy\n...",
  "commands": [
    "help",
    "tasks",
    "doctor",
    "test",
    "watch",
    "init",
    "migrate",
    "config",
    "unlock",
    "cache",
    "completion"
  ]
}
```

## Completion Candidates

### 19) Completion Candidates (`effigy.completion.candidates.v1`)

Warm-hit example:

```json
{
  "schema": "effigy.completion.candidates.v1",
  "schema_version": 1,
  "ok": true,
  "repo": "/workspace",
  "prefix": "farm",
  "cache_hit": true,
  "cache_state": "hit",
  "cache_age_ms": 14,
  "cache_ttl_ms": 2000,
  "effective_cache_ttl_ms": 2000,
  "cache_ttl_source": "default",
  "manifest_count": 3,
  "candidates": [
    "catalog_a/api",
    "catalog_a/build"
  ]
}
```

Miss example (invalid env policy fallback):

```json
{
  "schema": "effigy.completion.candidates.v1",
  "schema_version": 1,
  "ok": true,
  "repo": "/workspace",
  "prefix": "farm",
  "cache_hit": false,
  "cache_state": "miss_initial",
  "cache_age_ms": null,
  "cache_ttl_ms": null,
  "effective_cache_ttl_ms": 2000,
  "cache_ttl_source": "env_invalid",
  "manifest_count": 3,
  "candidates": [
    "catalog_a/api",
    "catalog_a/build"
  ]
}
```

## More Scan Payloads

### 20) Scan Attention Markers (`effigy.scan.attention-markers.v1`)

```json
{
  "schema": "effigy.scan.attention-markers.v1",
  "schema_version": 1,
  "ok": true,
  "scan": "attention-markers",
  "format": "text",
  "root": "/workspace/app",
  "patterns": {
    "warning": ["TODO", "REVIEW", "NOTE", "placeholder"],
    "high": ["FIXME", "HACK", "@deprecated", "workaround"],
    "critical": ["BUG", "SECURITY", "remove before release"]
  },
  "scanned_files": 142,
  "matched_lines": 3,
  "finding_count": 3,
  "fail_on_findings": false,
  "respect_gitignore": true,
  "output_path": null,
  "findings": [
    {
      "path": "src/app.ts",
      "line": 18,
      "category": "deferred-work",
      "severity": "warning",
      "marker": "TODO",
      "snippet": "// TODO: split render path"
    },
    {
      "path": "src/api/router.ts",
      "line": 91,
      "category": "deferred-work",
      "severity": "high",
      "marker": "FIXME",
      "snippet": "// FIXME: remove fallback before merge"
    },
    {
      "path": "src/legacy.rs",
      "line": 12,
      "category": "deprecation",
      "severity": "high",
      "marker": "@deprecated",
      "snippet": "#[deprecated(note = \"use new_api\")]"
    }
  ],
  "text": "Attention Markers\n\nroot: /workspace/app\nmarkers: warning=4 high=4 critical=3\nscanned-files: 142  matched-lines: 3  findings: 3\nseverity-counts: critical=0 high=2 warning=1\nwarning-rows-hidden: 1  use --show-warnings to list them\n\nFindings\nhigh  src/api/router.ts:91  deferred-work  [FIXME]  // FIXME: remove fallback before merge\nhigh  src/legacy.rs:12  deprecation  [@deprecated]  #[deprecated(note = \"use new_api\")]"
}
```

### 21) Scan Stale Suppressions (`effigy.scan.stale-suppressions.v1`)

```json
{
  "schema": "effigy.scan.stale-suppressions.v1",
  "schema_version": 1,
  "ok": true,
  "scan": "stale-suppressions",
  "format": "text",
  "root": "/workspace/app",
  "patterns": {
    "warning": ["@ts-ignore", "@ts-expect-error", "type: ignore", "eslint-disable-next-line"],
    "high": ["#[allow(", "#[expect(", "rubocop:disable", "swiftlint:disable"],
    "critical": ["nolint", "#[allow(warnings)]", "shellcheck disable=", "eslint-disable"]
  },
  "scanned_files": 142,
  "matched_lines": 2,
  "finding_count": 2,
  "fail_on_findings": false,
  "respect_gitignore": true,
  "output_path": null,
  "findings": [
    {
      "path": "src/app.ts",
      "line": 18,
      "category": "lint-disable",
      "severity": "warning",
      "marker": "eslint-disable-next-line",
      "snippet": "// eslint-disable-next-line no-console"
    },
    {
      "path": "src/legacy.rs",
      "line": 12,
      "category": "type-ignore",
      "severity": "high",
      "marker": "#[allow(",
      "snippet": "#[allow(dead_code)]"
    }
  ],
  "text": "Stale Suppressions\n\nroot: /workspace/app\nmarkers: warning=4 high=4 critical=4\nscanned-files: 142  matched-lines: 2  findings: 2\nseverity-counts: critical=0 high=1 warning=1\nwarning-rows-hidden: 1  use --show-warnings to list them\n\nFindings\nhigh  src/legacy.rs:12  type-ignore  [#[allow(]  #[allow(dead_code)]"
}
```

## Execution Payloads

### 22) Task Run (`effigy.task.run.v1`)

```json
{
  "schema": "effigy.task.run.v1",
  "schema_version": 1,
  "ok": true,
  "task": "build",
  "command": "cargo run -p api --bin build",
  "exit_code": 0,
  "stdout": "build-ok",
  "stderr": "",
  "duration_ms": 214
}
```

Failure variant:

```json
{
  "schema": "effigy.task.run.v1",
  "schema_version": 1,
  "ok": false,
  "task": "fail",
  "command": "sh -lc 'printf fail-out; printf fail-err >&2; exit 9'",
  "exit_code": 9,
  "stdout": "fail-out",
  "stderr": "fail-err",
  "duration_ms": 32
}
```

## Artifact Payloads

### 23) Artifact Inspect (`effigy.artifact.inspect.v1`)

```json
{
  "schema": "effigy.artifact.inspect.v1",
  "schema_version": 1,
  "source": "oci://ghcr.io/acme/private-data:uat",
  "kind": "sql-dump",
  "staged_root": "/workspace/app/.effigy/local/artifacts/oci-ghcr-io-acme-private-data-uat-7f3a9b",
  "primary_files": ["dump.sql.gz"],
  "metadata": {
    "schema": "effigy.artifact.v1",
    "kind": "sql-dump",
    "source_type": "oci",
    "source_ref": "oci://ghcr.io/acme/private-data:uat",
    "digest": "sha256:7f3a9b..."
  }
}
```

Use this when you need to confirm what Effigy resolved before passing the
staged artifact into a seed or apply task.

### 24) Artifact Stage (`effigy.artifact.stage.v1`)

```json
{
  "schema": "effigy.artifact.stage.v1",
  "schema_version": 1,
  "source": "./data/legacy.sql.gz",
  "kind": "sql-dump",
  "staged_root": "/workspace/app/.effigy/local/artifacts/local-data-legacy-sql-gz-9c4e2d",
  "primary_files": ["legacy.sql.gz"],
  "metadata": {
    "schema": "effigy.artifact.v1",
    "kind": "sql-dump",
    "source_type": "local",
    "source_path": "./data/legacy.sql.gz"
  }
}
```

Staging is deterministic: the same source produces the same staged root path.

### 25) Artifact Capture (`effigy.artifact.capture.v1`)

Planned capture (no `--push`):

```json
{
  "schema": "effigy.artifact.capture.v1",
  "schema_version": 1,
  "source": "./dumps/uat.sql.gz",
  "ref": "oci://ghcr.io/acme/uat-content:2026-05-06",
  "pushed": false,
  "staged_root": "/workspace/app/.effigy/local/artifacts/capture-dumps-uat-sql-gz-1a2b3c",
  "primary_files": ["uat.sql.gz"],
  "metadata": {
    "schema": "effigy.artifact.v1",
    "kind": "sql-dump",
    "source_type": "local",
    "source_path": "./dumps/uat.sql.gz",
    "environment": "uat"
  }
}
```

Pushed capture:

```json
{
  "schema": "effigy.artifact.capture.v1",
  "schema_version": 1,
  "source": "./dumps/uat.sql.gz",
  "ref": "oci://ghcr.io/acme/uat-content:2026-05-06",
  "pushed": true,
  "digest": "sha256:8e5d2f...",
  "staged_root": "/workspace/app/.effigy/local/artifacts/capture-dumps-uat-sql-gz-1a2b3c",
  "primary_files": ["uat.sql.gz"],
  "metadata": {
    "schema": "effigy.artifact.v1",
    "kind": "sql-dump",
    "source_type": "local",
    "source_path": "./dumps/uat.sql.gz",
    "environment": "uat"
  }
}
```

Capture is two-phase by default: stage locally first, then publish with `--push`.
Digest-pinned refs are invalid push destinations.

## State Stack Payloads

### 26) State Stack Lineage (`effigy.state-stack.lineage.v1`)

This is the `result` payload for:

```sh
effigy --json state plan ./state/acowtancy-uat.toml
```

```json
{
  "schema": "effigy.state-stack.lineage.v1",
  "schema_version": 1,
  "lineage_id": "acowtancy-uat:Uat:structure+baseline-seed+legacy-content+uat-capture",
  "stack_name": "acowtancy-uat",
  "environment": "uat",
  "created_at": "planned",
  "layers": [
    {
      "index": 0,
      "key": "structure",
      "role": "structure",
      "apply_mode": "task",
      "environment_policy": "all",
      "source": "farmyard:db:migrate",
      "artifact_source": null,
      "hook": "farmyard:db:migrate",
      "snapshot_identity": null
    },
    {
      "index": 1,
      "key": "baseline-seed",
      "role": "baseline-seed",
      "apply_mode": "sql",
      "environment_policy": "all",
      "source": "./seed/static.sql",
      "artifact_source": {
        "source_ref": "./seed/static.sql",
        "kind": "sql-dump"
      },
      "hook": null,
      "snapshot_identity": "static-reference-data@2026-05-08"
    },
    {
      "index": 2,
      "key": "legacy-content",
      "role": "legacy-import",
      "apply_mode": "artifact",
      "environment_policy": "all",
      "source": "oci://ghcr.io/acowtancy/content:legacy-2026-05-08",
      "artifact_source": {
        "source_ref": "oci://ghcr.io/acowtancy/content:legacy-2026-05-08",
        "kind": "app-specific"
      },
      "hook": "acowtancy:migrate:apply-legacy-content",
      "snapshot_identity": "old-site-db@2026-05-08"
    },
    {
      "index": 3,
      "key": "uat-capture",
      "role": "uat-capture",
      "apply_mode": "artifact",
      "environment_policy": "non-production",
      "source": "oci://ghcr.io/acowtancy/content:uat-authored-2026-05-08",
      "artifact_source": {
        "source_ref": "oci://ghcr.io/acowtancy/content:uat-authored-2026-05-08",
        "kind": "content-overlay"
      },
      "hook": "acowtancy:migrate:apply-uat-overlay",
      "snapshot_identity": "uat-freeze@2026-05-08"
    }
  ],
  "artifact_reports": [
    {
      "layer_key": "baseline-seed",
      "source_ref": "./seed/static.sql",
      "artifact_kind": "sql-dump",
      "operation": "planned-resolve"
    },
    {
      "layer_key": "legacy-content",
      "source_ref": "oci://ghcr.io/acowtancy/content:legacy-2026-05-08",
      "artifact_kind": "app-specific",
      "operation": "planned-resolve"
    },
    {
      "layer_key": "uat-capture",
      "source_ref": "oci://ghcr.io/acowtancy/content:uat-authored-2026-05-08",
      "artifact_kind": "content-overlay",
      "operation": "planned-resolve"
    }
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/acowtancy-uat/plan.json",
  "written_history_path": ".effigy/reports/state/acowtancy-uat/history/20260508T143012Z-plan-acowtancy-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

The state plan payload is lineage-only. `planned-resolve` means Effigy has
validated the layer source as an artifact reference for later execution; it has
not staged, pulled, applied, captured, or run app-owned hooks.
`written_report_path` and `written_history_path` are present only when
`--write-report` is used.

### 27) State Stack Apply (`effigy.state-stack.apply.v1`)

Plan-only apply:

```json
{
  "schema": "effigy.state-stack.apply.v1",
  "schema_version": 1,
  "ok": true,
  "executed": false,
  "stack_name": "acowtancy-uat",
  "environment": "uat",
  "lineage_id": "acowtancy-uat:Uat:structure+baseline-seed+legacy-content",
  "layers": [
    {
      "index": 0,
      "key": "structure",
      "role": "structure",
      "apply_mode": "task",
      "source": "farmyard:db:migrate",
      "status": "would-execute"
    },
    {
      "index": 1,
      "key": "baseline-seed",
      "role": "baseline-seed",
      "apply_mode": "sql",
      "source": "./seed/static.sql",
      "target": "app",
      "status": "would-import"
    },
    {
      "index": 2,
      "key": "legacy-content",
      "role": "legacy-import",
      "apply_mode": "artifact",
      "source": "./legacy.sql",
      "status": "would-stage"
    }
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/acowtancy-uat/latest-capture.json",
  "written_history_path": ".effigy/reports/state/acowtancy-uat/history/20260508T143015Z-capture-acowtancy-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

Executed task-only apply:

```json
{
  "schema": "effigy.state-stack.apply.v1",
  "schema_version": 1,
  "ok": true,
  "executed": true,
  "stack_name": "acowtancy-uat",
  "environment": "uat",
  "lineage_id": "acowtancy-uat:Uat:structure+baseline-seed+legacy-content",
  "layers": [
    {
      "index": 0,
      "key": "structure",
      "role": "structure",
      "apply_mode": "task",
      "source": "farmyard:db:migrate",
      "status": "executed",
      "output": "migration complete"
    },
    {
      "index": 1,
      "key": "baseline-seed",
      "role": "baseline-seed",
      "apply_mode": "sql",
      "source": "./seed/static.sql",
      "target": "app",
      "status": "imported",
      "sql_report": {
        "schema": "effigy.state-stack.sql-import.v1",
        "schema_version": 1,
        "ok": true,
        "target": "app",
        "source": "./seed/static.sql",
        "artifact_reports": [
          {
            "metadata": {
              "schema": "effigy.artifact.v1",
              "kind": "sql-dump",
              "source_type": "local",
              "source": "./seed/static.sql",
              "digest": null,
              "staged_root": "/workspace/app/.effigy/local/artifacts/static-sql-8bc721",
              "primary_files": [
                "/workspace/app/.effigy/local/artifacts/static-sql-8bc721/static.sql"
              ],
              "environment_label": null
            },
            "metadata_path": "/workspace/app/.effigy/local/artifacts/static-sql-8bc721/effigy-artifact.json"
          }
        ],
        "staged": [
          {
            "target": "app",
            "source_path": "./seed/static.sql",
            "staged_path": "/workspace/app/.effigy/local/db-seeds/app--static.sql"
          }
        ]
      }
    },
    {
      "index": 2,
      "key": "legacy-content",
      "role": "legacy-import",
      "apply_mode": "artifact",
      "source": "./legacy.sql",
      "status": "staged",
      "artifact_report": {
        "schema": "effigy.artifact.stage.v1",
        "schema_version": 1,
        "ok": true,
        "metadata_path": "/workspace/app/.effigy/local/artifacts/legacy-sql-9c4e2d/effigy-artifact.json",
        "metadata": {
          "schema": "effigy.artifact.v1",
          "kind": "sql-dump",
          "source_type": "local",
          "source": "./legacy.sql",
          "digest": null,
          "staged_root": "/workspace/app/.effigy/local/artifacts/legacy-sql-9c4e2d",
          "primary_files": [
            "/workspace/app/.effigy/local/artifacts/legacy-sql-9c4e2d/legacy.sql"
          ],
          "environment_label": null
        },
        "farmyard_handoff": null
      }
    }
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/acowtancy-uat/latest-capture.json",
  "written_history_path": ".effigy/reports/state/acowtancy-uat/history/20260508T143016Z-capture-acowtancy-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

`state apply --yes` executes `apply_mode = "task"` layers, stages
`apply_mode = "artifact"` layers, and imports `apply_mode = "sql"` layers
through the existing database seed/import plumbing. When a successful layer
declares `hook`, Effigy then runs that repo-owned task with a structured
`EFFIGY_STATE_APPLY_CONTEXT` handoff plus narrow layer env vars. Apply reports
update `latest-apply.json` and timestamped history. Capture, manual,
checkpoint, and app-specific payload semantics remain reported as
`unsupported` by the apply adapter. `state apply --skip-layer <KEY>` leaves the
layer in the report with `status = "skipped"` and does not execute its
task/import/stage step.

### 28) State Stack Capture (`effigy.state-stack.capture.v1`)

This payload is the state-level report shape for `effigy state capture`. The
capture boundary stays separate from app-owned diff/reconciliation logic.

Planned UAT overlay capture:

```json
{
  "schema": "effigy.state-stack.capture.v1",
  "schema_version": 1,
  "ok": true,
  "executed": false,
  "stack_name": "acowtancy-uat",
  "source_environment": "uat",
  "capture_role": "uat-capture",
  "capture_mode": "uat-overlay",
  "parent_lineage_id": "acowtancy-uat:Uat:structure+baseline-seed+legacy-content",
  "created_at": "planned",
  "produced_layers": [
    {
      "key": "uat-capture-2026-05-08",
      "role": "uat-capture",
      "apply_mode": "artifact",
      "environment_policy": "non-production",
      "artifact_kind": "app-specific",
      "source_ref": "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
      "snapshot_identity": "uat-authored-content@2026-05-08",
      "depends_on": ["legacy-content"],
      "hook": "acowtancy:migrate:apply-uat-capture"
    }
  ],
  "capture_artifacts": [
    {
      "layer_key": "uat-capture-2026-05-08",
      "operation": "planned-capture",
      "ref": "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
      "digest": null,
      "artifact_report": null
    }
  ],
  "tasks": [
    {
      "name": "acowtancy:migrate:capture-uat-overlay",
      "status": "planned",
      "output": null,
      "error": null
    }
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/acowtancy-uat/latest-capture.json",
  "written_history_path": ".effigy/reports/state/acowtancy-uat/history/20260508T143017Z-capture-acowtancy-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

Executed capture tasks receive a versioned JSON context file. The path is also
available in `EFFIGY_STATE_CAPTURE_CONTEXT`.

```json
{
  "schema": "effigy.state-stack.capture-context.v1",
  "schema_version": 1,
  "stack_name": "acowtancy-uat",
  "parent_lineage_id": "acowtancy-uat:Uat:structure+baseline-seed+legacy-content",
  "capture_role": "uat-capture",
  "capture_mode": "uat-overlay",
  "source_environment": "uat",
  "key": "uat-capture-2026-05-08",
  "source": "captures/uat-overlay.json",
  "destination_ref": "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08"
}
```

### 28b) State Capture Set (`effigy.state-stack.capture-set.v1`)

This payload is the aggregate report shape for `effigy state capture-set`, which
runs multiple named capture profiles with one shared key.

```json
{
  "schema": "effigy.state-stack.capture-set.v1",
  "schema_version": 1,
  "ok": true,
  "executed": true,
  "stack": "legacy-source",
  "key": "20260513-070000",
  "created_at": "2026-05-13T07:00:00Z",
  "profiles": ["db", "media"],
  "captures": [
    {
      "profile": "db",
      "ok": true,
      "report": {
        "schema": "effigy.state-stack.capture.v1",
        "schema_version": 1,
        "ok": true,
        "executed": true,
        "stack_name": "acowtancy-legacy-source",
        "source_environment": "dev",
        "capture_role": "full-capture",
        "capture_mode": "full-snapshot",
        "parent_lineage_id": "acowtancy-legacy-source:Dev:legacy-source-root",
        "created_at": "planned",
        "produced_layers": [],
        "capture_artifacts": [],
        "tasks": [],
        "warnings": []
      }
    },
    {
      "profile": "media",
      "ok": true,
      "report": {
        "schema": "effigy.state-stack.capture.v1",
        "schema_version": 1,
        "ok": true,
        "executed": true,
        "stack_name": "acowtancy-legacy-source",
        "source_environment": "dev",
        "capture_role": "full-capture",
        "capture_mode": "full-snapshot",
        "parent_lineage_id": "acowtancy-legacy-source:Dev:legacy-source-root",
        "created_at": "planned",
        "produced_layers": [],
        "capture_artifacts": [],
        "tasks": [],
        "warnings": []
      }
    }
  ],
  "written_report_path": ".effigy/reports/state/legacy-source/latest-capture-set.json",
  "written_history_path": ".effigy/reports/state/legacy-source/history/20260513T070000Z-capture-set-20260513-070000.json"
}
```

Staged local capture:

```json
{
  "schema": "effigy.state-stack.capture.v1",
  "schema_version": 1,
  "ok": true,
  "executed": true,
  "stack_name": "acowtancy-uat",
  "source_environment": "uat",
  "capture_role": "uat-capture",
  "capture_mode": "uat-overlay",
  "parent_lineage_id": "acowtancy-uat:Uat:structure+baseline-seed+legacy-content",
  "created_at": "planned",
  "produced_layers": [
    {
      "key": "uat-capture-2026-05-08",
      "role": "uat-capture",
      "apply_mode": "artifact",
      "environment_policy": "non-production",
      "artifact_kind": "app-specific",
      "source_ref": "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
      "snapshot_identity": "uat-authored-content@planned",
      "depends_on": ["legacy-content"],
      "hook": "acowtancy:migrate:apply-uat-capture"
    }
  ],
  "capture_artifacts": [
    {
      "layer_key": "uat-capture-2026-05-08",
      "operation": "captured-local",
      "ref": "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
      "artifact_report": {
        "schema": "effigy.artifact.capture.v1",
        "schema_version": 1,
        "ok": true,
        "metadata_path": "/workspace/app/.effigy/local/artifacts/capture-uat-overlay-json-a1b2c3/effigy-artifact.json",
        "metadata": {
          "schema": "effigy.artifact.v1",
          "kind": "app-specific",
          "source_type": "local",
          "source": "/workspace/app/captures/uat-overlay.json",
          "digest": null,
          "staged_root": "/workspace/app/.effigy/local/artifacts/capture-uat-overlay-json-a1b2c3",
          "primary_files": [
            "/workspace/app/.effigy/local/artifacts/capture-uat-overlay-json-a1b2c3/uat-overlay.json"
          ],
          "environment_label": "uat"
        },
        "destination": {
          "source": "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
          "reference": "ghcr.io/acowtancy/content:uat-capture-2026-05-08",
          "planned": true,
          "pushed": false,
          "digest": null,
          "descriptor": null
        },
        "farmyard_handoff": null
      }
    }
  ],
  "tasks": [
    {
      "name": "acowtancy:migrate:capture-uat-overlay",
      "status": "executed",
      "context_path": ".effigy/state/capture-context/acowtancy-uat/uat-capture-2026-05-08.json",
      "output": "capture complete"
    }
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/acowtancy-uat/latest-capture.json",
  "written_history_path": ".effigy/reports/state/acowtancy-uat/history/20260508T143016Z-capture-acowtancy-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

Explicitly pushed capture:

```json
{
  "schema": "effigy.state-stack.capture.v1",
  "schema_version": 1,
  "ok": true,
  "executed": true,
  "stack_name": "acowtancy-uat",
  "source_environment": "uat",
  "capture_role": "uat-capture",
  "capture_mode": "uat-overlay",
  "parent_lineage_id": "acowtancy-uat:Uat:structure+baseline-seed+legacy-content",
  "created_at": "planned",
  "produced_layers": [
    {
      "key": "uat-capture-2026-05-08",
      "role": "uat-capture",
      "apply_mode": "artifact",
      "environment_policy": "non-production",
      "artifact_kind": "app-specific",
      "source_ref": "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
      "snapshot_identity": "uat-authored-content@planned",
      "depends_on": ["legacy-content"],
      "hook": "acowtancy:migrate:apply-uat-capture"
    }
  ],
  "capture_artifacts": [
    {
      "layer_key": "uat-capture-2026-05-08",
      "operation": "pushed",
      "ref": "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
      "digest": "sha256:8e5d2f...",
      "artifact_report": {
        "schema": "effigy.artifact.capture.v1",
        "schema_version": 1,
        "ok": true,
        "metadata_path": "/workspace/app/.effigy/local/artifacts/capture-uat-overlay-json-a1b2c3/effigy-artifact.json",
        "metadata": {
          "schema": "effigy.artifact.v1",
          "kind": "app-specific",
          "source_type": "local",
          "source": "/workspace/app/captures/uat-overlay.json",
          "digest": null,
          "staged_root": "/workspace/app/.effigy/local/artifacts/capture-uat-overlay-json-a1b2c3",
          "primary_files": [
            "/workspace/app/.effigy/local/artifacts/capture-uat-overlay-json-a1b2c3/uat-overlay.json"
          ],
          "environment_label": "uat"
        },
        "destination": {
          "source": "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
          "reference": "ghcr.io/acowtancy/content:uat-capture-2026-05-08",
          "planned": false,
          "pushed": true,
          "digest": "sha256:8e5d2f...",
          "descriptor": {
            "reference": "ghcr.io/acowtancy/content:uat-capture-2026-05-08",
            "redacted_reference": "ghcr.io/acowtancy/content:uat-capture-2026-05-08",
            "digest": "sha256:8e5d2f...",
            "media_type": "application/vnd.oci.image.manifest.v1+json",
            "size": 123
          }
        },
        "farmyard_handoff": null
      }
    }
  ],
  "tasks": [],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/acowtancy-uat/latest-capture.json",
  "written_history_path": ".effigy/reports/state/acowtancy-uat/history/20260508T143017Z-capture-acowtancy-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

### 29) State Stack History (`effigy.state-stack.history.v1`)

This payload is the `result` for:

```sh
effigy --json state history uat --kind capture --limit 5
```

```json
{
  "schema": "effigy.state-stack.history.v1",
  "schema_version": 1,
  "stack_name": "uat",
  "reports": [
    {
      "kind": "capture",
      "schema": "effigy.state-stack.capture.v1",
      "path": ".effigy/reports/state/uat/history/20260508T110000Z-capture-uat.json",
      "created_at": "20260508T110000Z",
      "parent_lineage_id": "uat:lineage:base",
      "ok": true,
      "executed": true,
      "summary": "1 produced layer(s)"
    },
    {
      "kind": "plan",
      "schema": "effigy.state-stack.lineage.v1",
      "path": ".effigy/reports/state/uat/plan.json",
      "created_at": "20260508T100000Z",
      "lineage_id": "uat:lineage:base",
      "summary": "3 planned layer(s)"
    }
  ],
  "warnings": [
    "ignored malformed state report .effigy/reports/state/uat/history/broken.json: expected ident at line 1 column 2"
  ]
}
```

History lookup is read-only. It scans report JSON files and treats malformed
files as warnings so manual cleanup or old report layouts do not corrupt hidden
state.

### 30) Secrets (`effigy.secrets.v1`)

This payload is the `result` for:

```sh
effigy --json secrets list
effigy --json secrets doctor
effigy --json secrets init
effigy --json secrets get database_url
effigy --json secrets set database_url
effigy --json secrets unset database_url
effigy --json secrets change-passphrase
effigy --json secrets export --format env --output .effigy/runtime/secrets/local.env --yes
```

```json
{
  "schema": "effigy.secrets.v1",
  "schema_version": 1,
  "ok": true,
  "repo_root": "/workspace/app",
  "declared": true,
  "backend": "effigy-vault",
  "vault": {
    "path": ".effigy/secrets/local.vault",
    "identity": "ssh-agent",
    "unlock": "key-and-passphrase"
  },
  "external": null,
  "keys": [
    {
      "name": "database_url",
      "required": true,
      "targets": [
        "tasks",
        "containers"
      ],
      "description": "Application database connection URL"
    },
    {
      "name": "render_api_key",
      "required": false,
      "targets": [
        "deploy"
      ],
      "description": "Render API key for deployment checks and apply"
    }
  ],
  "warnings": [],
  "blockers": [],
  "vault_state": {
    "status": "unlocked",
    "path": "/workspace/app/.effigy/secrets/local.vault",
    "stored_keys": [
      "database_url"
    ],
    "missing_required": [],
    "undeclared_stored": []
  }
}
```

`effigy.secrets.v1` reports names, targets, required flags, backend, vault
metadata, and safe vault state. It does not contain secret values, value hashes,
decrypted vault contents, or injected environment.

If a repo has no `[secrets]` section, `declared` is `false`, `keys` is empty,
and the command succeeds. If a declared backend is missing required config,
`secrets doctor` reports blockers and returns a failed command result.

Mutation commands add safe operation metadata:

```json
{
  "schema": "effigy.secrets.v1",
  "schema_version": 1,
  "ok": true,
  "repo_root": "/workspace/app",
  "declared": true,
  "backend": "effigy-vault",
  "vault": {
    "path": ".effigy/secrets/local.vault",
    "identity": "ssh-agent",
    "unlock": "key-and-passphrase"
  },
  "external": null,
  "keys": [
    {
      "name": "database_url",
      "required": true,
      "targets": [
        "tasks",
        "containers"
      ],
      "description": "Application database connection URL"
    }
  ],
  "warnings": [],
  "blockers": [],
  "action": "set",
  "name": "database_url",
  "vault_path": "/workspace/app/.effigy/secrets/local.vault",
  "changed": true,
  "summary": "stored declared secret"
}
```

`secrets get` intentionally returns one decrypted value. `secrets
change-passphrase` preserves stored values while re-encrypting the vault with a
new passphrase.

`secrets export` is an explicit plaintext compatibility bridge. It requires
`--yes`, writes only to a file, refuses repo-root `.env`, and never includes
secret values in JSON or text output. Export metadata adds `action = "export"`,
`format = "env"`, `output`, `keys_exported`, `changed`, and a plaintext warning.

## Notes

- Field sets can grow with new optional keys while retaining schema compatibility.
- Use `jq` in CI to assert required fields instead of strict full-document equality.

## Expected Outcome

After this guide, you should be able to:

- find a realistic example close to the payload you need
- distinguish envelope fields from command-specific payload fields
- write safer assertions against stable required keys instead of full-document
  equality

## Related Guides

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`072-artifact-commands-guide.md`](./072-artifact-commands-guide.md)

## Next Step

After using one of these examples, run the real command with
`effigy --json <command>` in your target repo and tighten your integration
around the fields that are actually required for the workflow.
