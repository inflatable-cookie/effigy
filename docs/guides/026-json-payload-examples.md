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
  `Tasks`, `Task Status`, `Doctor Explain`, `Task Run`, `Task Plan`
- health, repo checks, or diagnostics:
  `Doctor` and the `Scan *` payloads
- test automation:
  `Test Plan`, `Test Results`
- bootstrap and setup:
  `Bootstrap`, `Deploy Model`, `Init`, `Config`, `Unlock`
- secret declarations and diagnostics:
  `Secrets`
- machine-local dependency links:
  `Dependency Status`, `Cargo Dependency Link`, `Bun Dependency Link`, and
  Cargo/Bun unlink
- layered state, seed, and migration planning:
  `State Stack Lineage`
- shell completion or editor integration:
  `Completion`, `Completion Candidates`
- scoped container hosts and retirement:
  `Container Hosts`, `Container Retire`

Companion references:

- [`017-json-output-contracts.md`](017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](024-ci-and-automation-recipes.md)

### Dependency Status (`effigy.deps.status.v1`)

```json
{
  "schema": "effigy.deps.status.v1",
  "schema_version": 1,
  "command": "deps status",
  "repo_root": "/workspace/app",
  "manager": "cargo",
  "detected_managers": ["cargo"],
  "summary": {
    "total": 1,
    "missing": 0,
    "healthy": 1,
    "drifted": 0,
    "conflict": 0,
    "warnings": 0,
    "errors": 0
  },
  "links": [
    {
      "manager": "cargo",
      "desired": {
        "key": {
          "manager": "cargo",
          "consumer_repo": "/workspace/app",
          "library_path": "/workspace/signal"
        },
        "mechanism": "cargo-patch",
        "consumer_roots": [
          { "canonical_path": "/workspace/app" }
        ],
        "packages": [
          {
            "name": "signal-core",
            "local_path": "/workspace/signal/crates/signal-core",
            "committed_sources": [
              {
                "kind": "git",
                "identity": "https://github.com/example/signal.git"
              }
            ]
          }
        ],
        "cargo_resolutions": [
          {
            "consumer_root": "/workspace/app",
            "package": "signal-core",
            "committed_source": {
              "kind": "git",
              "identity": "https://github.com/example/signal.git"
            },
            "local_path": "/workspace/signal/crates/signal-core"
          }
        ],
        "cargo_ownership": {
          "config_created_by_effigy": true,
          "cargo_dir_created_by_effigy": true
        }
      },
      "observed": {
        "state": "healthy",
        "packages": [
          {
            "name": "signal-core",
            "local_path": "/workspace/signal/crates/signal-core",
            "committed_sources": [
              {
                "kind": "git",
                "identity": "https://github.com/example/signal.git"
              }
            ]
          }
        ],
        "drift": []
      },
      "plan": null,
      "verification": {
        "status": "passed",
        "evidence": [
          {
            "package": "signal-core",
            "committed_sources": [
              {
                "kind": "git",
                "identity": "https://github.com/example/signal.git"
              }
            ],
            "expected_source": "/workspace/signal/crates/signal-core",
            "observed_source": "/workspace/signal/crates/signal-core",
            "methods": ["cargo-metadata"]
          }
        ]
      }
    }
  ]
}
```

### Cargo Dependency Link (`effigy.deps.link.v1`)

```json
{
  "schema": "effigy.deps.link.v1",
  "schema_version": 1,
  "command": "deps link cargo",
  "repo_root": "/workspace/app",
  "manager": "cargo",
  "library_path": "/workspace/signal",
  "dry_run": false,
  "report": {
    "outcome": "applied",
    "applied_files": [
      "/workspace/app/.cargo/config.toml",
      "/workspace/app/.gitignore",
      "/workspace/app/.effigy/local/dependency-links.json"
    ],
    "plan": {
      "desired": {
        "key": {
          "manager": "cargo",
          "consumer_repo": "/workspace/app",
          "library_path": "/workspace/signal"
        },
        "mechanism": "cargo-patch",
        "consumer_roots": [{ "canonical_path": "/workspace/app" }],
        "packages": [
          {
            "name": "signal-core",
            "local_path": "/workspace/signal/crates/signal-core",
            "committed_sources": [
              { "kind": "git", "identity": "https://github.com/example/signal.git" }
            ]
          }
        ],
        "cargo_ownership": {
          "config_created_by_effigy": true,
          "cargo_dir_created_by_effigy": true
        }
      },
      "operation": {
        "action": "link",
        "dry_run": false,
        "key": {
          "manager": "cargo",
          "consumer_repo": "/workspace/app",
          "library_path": "/workspace/signal"
        },
        "changes": [
          {
            "target": "/workspace/app/.cargo/config.toml",
            "action": "create",
            "description": "refresh the Effigy-managed Cargo patch block",
            "before": null,
            "after": "# >>> effigy deps cargo /workspace/signal >>>\n[patch.\"https://github.com/example/signal.git\"]\n\"signal-core\" = { path = \"/workspace/signal/crates/signal-core\" }\n# <<< effigy deps cargo /workspace/signal <<<\n"
          }
        ],
        "warnings": [
          "Cargo verification or builds may rewrite affected Cargo.lock entries to local path sources; do not commit that linked lock state"
        ]
      },
      "expected_resolutions": [
        {
          "consumer_root": "/workspace/app",
          "package": "signal-core",
          "committed_source": {
            "kind": "git",
            "identity": "https://github.com/example/signal.git"
          },
          "local_path": "/workspace/signal/crates/signal-core"
        }
      ],
      "affected_lockfiles": ["/workspace/app/Cargo.lock"],
      "lockfile_guard_packages": [],
      "remaining_linked_packages": [],
      "remove_empty_directories": []
    },
    "verification": {
      "status": "passed",
      "evidence": [
        {
          "package": "signal-core",
          "consumer_root": "/workspace/app",
          "committed_sources": [
            { "kind": "git", "identity": "https://github.com/example/signal.git" }
          ],
          "expected_source": "/workspace/signal/crates/signal-core",
          "observed_source": "/workspace/signal/crates/signal-core",
          "methods": ["cargo-metadata", "cargo-tree"]
        }
      ]
    },
    "rollback": {
      "attempted": false,
      "restored": [],
      "failures": []
    },
    "errors": []
  }
}
```

### Bun Dependency Link (`effigy.deps.link.v1`)

```json
{
  "schema": "effigy.deps.link.v1",
  "schema_version": 1,
  "command": "deps link bun",
  "repo_root": "/workspace/app",
  "manager": "bun",
  "library_path": "/workspace/poodle",
  "dry_run": true,
  "report": {
    "outcome": "dry-run",
    "applied_processes": [],
    "plan": {
      "desired": {
        "key": {
          "manager": "bun",
          "consumer_repo": "/workspace/app",
          "library_path": "/workspace/poodle"
        },
        "mechanism": "bun-link",
        "consumer_roots": [{ "canonical_path": "/workspace/app" }],
        "packages": [
          {
            "name": "@inflatable-cookie/poodle-core",
            "local_path": "/workspace/poodle/packages/core",
            "committed_sources": [{ "kind": "registry", "identity": "1.2.3" }]
          },
          {
            "name": "@inflatable-cookie/poodle-protocol",
            "local_path": "/workspace/poodle/packages/protocol",
            "committed_sources": [{ "kind": "registry", "identity": "1.2.3" }]
          }
        ]
      },
      "operation": {
        "action": "link",
        "dry_run": true,
        "key": {
          "manager": "bun",
          "consumer_repo": "/workspace/app",
          "library_path": "/workspace/poodle"
        },
        "changes": [],
        "warnings": [
          "Bun process intents explicitly use --no-save; package.json and Bun lockfiles must remain byte-for-byte unchanged"
        ]
      },
      "packages": [
        {
          "name": "@inflatable-cookie/poodle-core",
          "local_path": "/workspace/poodle/packages/core",
          "depth": "direct",
          "committed_version": "1.2.3",
          "registration": "absent",
          "consumer_link": "registry"
        },
        {
          "name": "@inflatable-cookie/poodle-protocol",
          "local_path": "/workspace/poodle/packages/protocol",
          "depth": "transitive",
          "committed_version": "1.2.3",
          "registration": "absent",
          "consumer_link": "registry"
        }
      ],
      "process_intents": [
        {
          "action": "register",
          "packages": ["@inflatable-cookie/poodle-core"],
          "cwd": "/workspace/poodle/packages/core",
          "program": "bun",
          "args": ["link", "--no-save"]
        },
        {
          "action": "register",
          "packages": ["@inflatable-cookie/poodle-protocol"],
          "cwd": "/workspace/poodle/packages/protocol",
          "program": "bun",
          "args": ["link", "--no-save"]
        },
        {
          "action": "link-consumer",
          "packages": ["@inflatable-cookie/poodle-core", "@inflatable-cookie/poodle-protocol"],
          "cwd": "/workspace/app",
          "program": "bun",
          "args": ["link", "@inflatable-cookie/poodle-core", "@inflatable-cookie/poodle-protocol", "--no-save"]
        }
      ],
      "symlink_intents": [],
      "physical_preconditions": [],
      "state_preconditions": [],
      "immutable_files": [
        { "path": "/workspace/app/package.json", "contents": [123, 125, 10] },
        { "path": "/workspace/app/bun.lock", "contents": null },
        { "path": "/workspace/app/bun.lockb", "contents": null }
      ]
    },
    "immutable_files": [
      { "path": "/workspace/app/package.json", "unchanged": true },
      { "path": "/workspace/app/bun.lock", "unchanged": true },
      { "path": "/workspace/app/bun.lockb", "unchanged": true }
    ],
    "verification": { "status": "not-run", "evidence": [] },
    "peer_diagnostics": [],
    "rollback": {
      "attempted": false,
      "restored_consumer_links": [],
      "removed_registrations": [],
      "restored_files": [],
      "failures": []
    },
    "errors": []
  }
}
```

### Bun Committed Dependency Pin (`effigy.deps.pin.v1`)

```json
{
  "schema": "effigy.deps.pin.v1",
  "schema_version": 1,
  "command": "deps pin bun",
  "operation": "pin",
  "manager": "bun",
  "repo_root": "/workspace/app",
  "manifest_path": "/workspace/app/package.json",
  "library_path": "/workspace/poodle",
  "dry_run": true,
  "outcome": "dry-run",
  "packages": [
    {
      "name": "@inflatable-cookie/poodle-core",
      "local_path": "/workspace/poodle/packages/core",
      "depth": "transitive",
      "before": null,
      "after": "file:../poodle/packages/core",
      "action": "add"
    }
  ],
  "writes": [],
  "warnings": [
    {
      "code": "lockfile-enumeration-fallback",
      "message": "`bun` dependency process failed in `/workspace/app` with exit status 1: Error loading lockfile: InvalidPackageInfo; pin planning used read-only package inventory from `/workspace/app/bun.lock`"
    },
    {
      "code": "checkout-topology-portability",
      "message": "pin for `@inflatable-cookie/poodle-core` escapes the consumer repository; CI and teammates need the same relative checkout topology",
      "package": "@inflatable-cookie/poodle-core"
    }
  ],
  "verification": {
    "status": "not-run",
    "install_pending": false,
    "immutable_files": []
  },
  "errors": [],
  "next_actions": [
    "re-run `effigy deps pin bun /workspace/poodle` without `--dry-run`",
    "then run `bun install` and review the resulting lockfile separately"
  ]
}
```

### Cargo Dependency Unlink (`effigy.deps.unlink.v1`)

```json
{
  "schema": "effigy.deps.unlink.v1",
  "schema_version": 1,
  "command": "deps unlink cargo",
  "repo_root": "/workspace/app",
  "manager": "cargo",
  "library_path": "/workspace/signal",
  "dry_run": false,
  "report": {
    "outcome": "unlinked",
    "applied_files": [
      "/workspace/app/.cargo/config.toml",
      "/workspace/app/.effigy/local/dependency-links.json"
    ],
    "removed_directories": ["/workspace/app/.cargo"],
    "plan": {
      "desired": null,
      "operation": {
        "action": "unlink",
        "dry_run": false,
        "key": {
          "manager": "cargo",
          "consumer_repo": "/workspace/app",
          "library_path": "/workspace/signal"
        },
        "changes": [],
        "warnings": [
          "unlink apply must re-resolve affected lockfiles from committed git sources without Git restore commands"
        ]
      },
      "expected_resolutions": [
        {
          "consumer_root": "/workspace/app",
          "package": "signal-core",
          "committed_source": {
            "kind": "git",
            "identity": "https://github.com/example/signal.git"
          },
          "local_path": "/workspace/signal/crates/signal-core"
        }
      ],
      "affected_lockfiles": ["/workspace/app/Cargo.lock"],
      "lockfile_guard_packages": ["signal-core"],
      "remaining_linked_packages": [],
      "remove_empty_directories": ["/workspace/app/.cargo"]
    },
    "verification": {
      "status": "passed",
      "evidence": [
        {
          "package": "signal-core",
          "consumer_root": "/workspace/app",
          "committed_sources": [
            { "kind": "git", "identity": "https://github.com/example/signal.git" }
          ],
          "expected_source": "https://github.com/example/signal.git",
          "observed_source": "https://github.com/example/signal.git",
          "methods": ["cargo-metadata", "cargo-tree"]
        }
      ]
    },
    "lockfiles": [
      {
        "path": "/workspace/app/Cargo.lock",
        "before_state": "active-links",
        "after_state": "clean",
        "remaining_linked_packages": []
      }
    ],
    "rollback": {
      "attempted": false,
      "restored": [],
      "failures": []
    },
    "errors": []
  }
}
```

### Bun Dependency Unlink (`effigy.deps.unlink.v1`)

```json
{
  "schema": "effigy.deps.unlink.v1",
  "schema_version": 1,
  "command": "deps unlink bun",
  "repo_root": "/workspace/app",
  "manager": "bun",
  "library_path": "/workspace/poodle",
  "dry_run": false,
  "report": {
    "outcome": "unlinked",
    "removed_consumer_links": [
      "/workspace/app/node_modules/@inflatable-cookie/poodle-core",
      "/workspace/app/node_modules/@inflatable-cookie/poodle-protocol"
    ],
    "applied_processes": [
      {
        "action": "unregister",
        "packages": ["@inflatable-cookie/poodle-core"],
        "cwd": "/workspace/poodle/packages/core",
        "program": "bun",
        "args": ["unlink", "--no-save"]
      },
      {
        "action": "unregister",
        "packages": ["@inflatable-cookie/poodle-protocol"],
        "cwd": "/workspace/poodle/packages/protocol",
        "program": "bun",
        "args": ["unlink", "--no-save"]
      }
    ],
    "immutable_files": [
      { "path": "/workspace/app/package.json", "unchanged": true },
      { "path": "/workspace/app/bun.lock", "unchanged": true },
      { "path": "/workspace/app/bun.lockb", "unchanged": true }
    ],
    "verification": {
      "status": "passed",
      "evidence": [
        {
          "package": "@inflatable-cookie/poodle-core",
          "consumer_root": "/workspace/app/node_modules/@inflatable-cookie/poodle-core",
          "expected_source": "missing",
          "observed_source": "missing",
          "methods": ["bun-consumer-unlink"]
        },
        {
          "package": "@inflatable-cookie/poodle-protocol",
          "consumer_root": "/workspace/app/node_modules/@inflatable-cookie/poodle-protocol",
          "expected_source": "missing",
          "observed_source": "missing",
          "methods": ["bun-consumer-unlink"]
        }
      ]
    },
    "rollback": {
      "attempted": false,
      "relinked_consumer_packages": [],
      "restored_registrations": [],
      "restored_files": [],
      "failures": []
    },
    "errors": []
  }
}
```

## How To Use This Guide

- treat the samples here as shape examples, not a promise that every field
  ordering choice matters
- use [`017-json-output-contracts.md`](017-json-output-contracts.md) for the
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
      "run": "cargo run -p api --bin build",
      "group": "Development",
      "description": "Build the api binary from the workspace"
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
  "selectors": [
    {
      "selector": "build",
      "kind": "task",
      "source": "catalog",
      "catalog": "root"
    },
    {
      "selector": "api/build",
      "kind": "task",
      "source": "catalog",
      "catalog": "api"
    },
    {
      "selector": "dev admin",
      "kind": "managed-profile",
      "source": "catalog",
      "catalog": "root"
    },
    {
      "selector": "doctor",
      "kind": "builtin",
      "source": "builtin",
      "catalog": null
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

### Task visibility metadata

`effigy.tasks.v1` catalog task rows include `visibility` (`public` or
`internal`), `visibility_reason` (`declared`, `referenced-only` or `default`),
and `group` (string or null). JSON includes every task; text defaults to public
tasks. Internal tasks use the ordinary task-run and task-plan payloads.

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
  "written_report_path": ".effigy/reports/state/example-app-uat/latest-apply.json",
  "written_history_path": ".effigy/reports/state/example-app-uat/history/20260508T143013Z-apply-example-app-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

### 3) Task Status (`effigy.tasks-status.v3`)

```json
{
  "schema": "effigy.tasks-status.v3",
  "schema_version": 3,
  "task": {
    "selector": "catalog_a/build",
    "catalog": "catalog_a",
    "name": "build",
    "surface": "published"
  },
  "state": "running",
  "current": {
    "evidence_source": "checkout-journal",
    "run_id": "agqvgrlm6bercdndodnbeu4tb4",
    "state": "running",
    "phase": "starting",
    "pid": 12345,
    "started_at_epoch_ms": 1792398204551
  },
  "latest": {
    "evidence_source": "checkout-journal",
    "run_id": "agqvgq2xz7c4ytj6fsz3s5s4ka",
    "state": "succeeded",
    "phase": "closed",
    "exit_code": 0,
    "cleanup": "complete",
    "started_at_epoch_ms": 1792398100000,
    "finished_at_epoch_ms": 1792398105000,
    "duration_ms": 5000
  },
  "history": [
    {
      "evidence_source": "checkout-journal",
    "run_id": "agqvgrlm6bercdndodnbeu4tb4",
      "state": "running",
      "phase": "starting",
      "pid": 12345,
      "started_at_epoch_ms": 1792398204551
    },
    {
      "evidence_source": "checkout-journal",
    "run_id": "agqvgq2xz7c4ytj6fsz3s5s4ka",
      "state": "succeeded",
      "phase": "closed",
      "exit_code": 0,
      "cleanup": "complete",
      "started_at_epoch_ms": 1792398100000,
      "finished_at_epoch_ms": 1792398105000,
      "duration_ms": 5000
    }
  ],
  "next": null,
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

A run whose owner exited without a result reads `"state": "interrupted"` with
`"next": "effigy catalog_a/build"`; rerunning the task settles it first.

### 4) Task Status Inventory (`effigy.tasks-status-all.v3`)

```json
{
  "schema": "effigy.tasks-status-all.v3",
  "schema_version": 2,
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
    "never-run": 1,
    "running": 1,
    "succeeded": 1
  },
  "warnings": [],
  "rows": [
    {
      "selector": "test",
      "selected_catalog_root": "root",
      "state": "running",
      "currently_declared": true,
      "current": {
        "run_id": "agqvgrlm6bercdndodnbeu4tb4",
        "state": "running",
        "phase": "starting",
        "pid": 12345,
        "started_at_epoch_ms": 1792398204551
      },
      "warnings": []
    },
    {
      "selector": "catalog_a/build",
      "selected_catalog_root": "catalog_a",
      "state": "succeeded",
      "currently_declared": true,
      "latest": {
        "run_id": "agqvgq2xz7c4ytj6fsz3s5s4ka",
        "state": "succeeded",
        "phase": "closed",
        "exit_code": 0,
        "cleanup": "complete",
        "started_at_epoch_ms": 1792398100000,
        "finished_at_epoch_ms": 1792398105000,
        "duration_ms": 5000
      },
      "warnings": []
    },
    {
      "selector": "idle",
      "selected_catalog_root": "root",
      "state": "never-run",
      "currently_declared": true,
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
  "provider_provenance": {
    "kind": "git",
    "url": "git@github.com:inflatable-cookie/effigy-provider-railway.git",
    "requested_ref": "main",
    "requested_rev": "0f3c9a4c1b7d2e8f5a6b3c9d0e1f2a3b4c5d6e7f",
    "commit": "0f3c9a4c1b7d2e8f5a6b3c9d0e1f2a3b4c5d6e7f",
    "resolution": "cached"
  },
  "files": [
    "services/front/railway.toml",
    "services/admin/railway.toml",
    "services/api/railway.toml",
    "report.json"
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/example-app-uat/latest-apply.json",
  "written_history_path": ".effigy/reports/state/example-app-uat/history/20260508T143014Z-apply-example-app-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

### Deploy Plan (`effigy.deploy.plan.v1`)

```json
{
  "schema": "effigy.deploy.plan.v1",
  "schema_version": 1,
  "env": "uat",
  "provider": "railway",
  "provider_provenance": {
    "kind": "git",
    "url": "git@github.com:inflatable-cookie/effigy-provider-railway.git",
    "requested_ref": "main",
    "requested_rev": "0f3c9a4c1b7d2e8f5a6b3c9d0e1f2a3b4c5d6e7f",
    "commit": "0f3c9a4c1b7d2e8f5a6b3c9d0e1f2a3b4c5d6e7f",
    "resolution": "cached"
  },
  "app": {
    "name": "example-app",
    "project_name": "example-app-uat"
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
    "lineage_id": "example-app-uat-Uat-structure-baseline-seed-legacy-import",
    "planned_report_path": ".effigy/reports/state/example-app-uat/latest-plan.json"
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
        "target": "example-app-uat"
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
  "deployment_id": "20260510T183000Z-example-app-uat-abc1234",
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
    "lineage_id": "example-app-uat-Uat-structure-baseline-seed-legacy-import",
    "apply_report_path": ".effigy/reports/state/example-app-uat/latest-apply.json"
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
  "written_history_path": ".effigy/reports/deploy/uat/history/20260510T183000Z-example-app-uat-abc1234.json"
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

The same normalization applies to `scan.duplicate-blocks` when that scanner is enabled for doctor.

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

## Test Payloads

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
      "runtime": {
        "target": "container",
        "container": "api-dev"
      },
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
            "package.json includes vitest dependency/script evidence"
          ]
        }
      ]
    }
  ]
}
```

Each target's `runtime` resolves the owning catalog's declared container
runtime with the same binding grammar tasks use: `target` is `host`,
`container` (with `container` naming the target), or `unusable` (with
`reason`) when a declared target cannot host suites. Catalogs without a
declared runtime target report `host`, and suites carrying nested effigy
re-entries keep their own routing.

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
      "outcome": "failed",
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

Every target names its `outcome`: `passed` when the suite ran and succeeded,
`failed` when it reported a failure, `cancelled` when the operator stopped the
interactive run before the suite reported a result, and `not_started` when the
suite never ran. Only `passed` sets `success`; `cancelled` and `not_started`
carry no success and no suite exit code in `code`. A cancelled interactive run
exits `130`, the interrupted convention a QA-group member records as
`cancelled`.

## Setup, Bootstrap, and Config

### 8) Bootstrap (`effigy.bootstrap.v1`)

```json
{
  "schema": "effigy.bootstrap.v1",
  "schema_version": 1,
  "ok": true,
  "phase": "executed",
  "repo_url": "git@github.com:contact-patch/contact-patch.git",
  "destination": "/workspace/sandboxes/contact-patch",
  "destination_source": "cwd-default",
  "branch": null,
  "root_repo_state": "cloned",
  "manifest_found": true,
  "manifest": {
    "path": "/workspace/sandboxes/contact-patch/effigy.toml",
    "file_found": true,
    "bootstrap_contract_found": true
  },
  "sources": [
    {
      "name": "book",
      "git": "git@github.com:contact-patch/book.git",
      "rev": "4a1b0c8f2d9e4f0b6a3c5d7e9f1a2b3c4d5e6f70",
      "path": "/workspace/sandboxes/book",
      "state": "cloned",
      "head": "4a1b0c8f2d9e4f0b6a3c5d7e9f1a2b3c4d5e6f70",
      "dirty": false,
      "detail": null
    }
  ],
  "submodules": {
    "requested": false,
    "applied": false,
    "drift": []
  },
  "setup": {
    "selectors": [
      "workspace:js:prepare"
    ],
    "ran": [
      "workspace:js:prepare"
    ]
  },
  "start": {
    "requested": true,
    "selector": "dev",
    "ran": true
  },
  "warnings": []
}
```

`phase = "plan"` payloads carry `manifest_found`, `manifest_unreadable` and a
`plan` object instead of the executed-only keys:

```json
{
  "schema": "effigy.bootstrap.v1",
  "schema_version": 1,
  "ok": true,
  "phase": "plan",
  "repo_url": null,
  "destination": "/workspace/contact-patch",
  "destination_source": "existing-checkout",
  "branch": null,
  "manifest_found": true,
  "manifest_unreadable": null,
  "plan": {
    "clone": null,
    "sources": [
      {
        "name": "book",
        "git": "git@github.com:contact-patch/book.git",
        "rev": "4a1b0c8f2d9e4f0b6a3c5d7e9f1a2b3c4d5e6f70",
        "path": "/workspace/book",
        "state": "missing",
        "head": null,
        "dirty": false,
        "detail": "no checkout at /workspace/book; `effigy bootstrap` clones it at the pin",
        "drift": null
      }
    ],
    "submodules": false,
    "setup": ["workspace:js:prepare"],
    "start": "dev",
    "start_requested": true
  }
}
```

Use `phase = "plan"` when you need the resolved destination, sources, setup and
start without mutation. Use `phase = "executed"` when you need to assert what was
actually cloned, updated, reported as drift or run.

Read these fields first:

- `root_repo_state` for the checkout decision (`cloned`, `updated`, `existing`)
- `manifest` to distinguish "no `effigy.toml`" from "manifest exists but has no
  `[bootstrap]` contract"
- `sources[]` for each pinned source's `state`, observed `head`, local `dirty`
  flag and `detail`; `drift` states are reported, never repaired
- `submodules` for whether recursive init was requested, applied, and which
  submodules drifted
- `setup` and `start` for which selectors were declared and which actually ran

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
    "warn": 1000,
    "high": 1800,
    "critical": 3000
  },
  "thresholds_source": "language-defaults",
  "scanned_files": 38,
  "skipped_generated": 4,
  "finding_count": 1,
  "fail_on_findings": false,
  "respect_gitignore": true,
  "output_path": null,
  "findings": [
    {
      "path": "src/server/router.ts",
      "code_lines": 1912,
      "total_lines": 2088,
      "severity": "high",
      "language": "ts"
    }
  ],
  "text": "God Files\n\nroot: /workspace/app\nthresholds: language-defaults (see `effigy scan god-files --help`)\nscanned-files: 38  skipped-generated: 4  findings: 1\nseverity-counts: critical=0 high=1 warning=0\n\nFindings\nhigh  1912 code lines (2088 total) [ts]  src/server/router.ts"
}
```

### 13) Scan Duplicate Blocks (`effigy.scan.duplicate-blocks.v1`)

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

### 14) Init (`effigy.init.v1`, `effigy.init.checklist.v1`, `effigy.init.actions.v1`, `effigy.init.list.v1`)

Baseline managed init (`effigy init --check --json` / `--apply --json` / `--repair --json`):

```json
{
  "schema": "effigy.init.v1",
  "schema_version": 1,
  "ok": true,
  "mode": "check",
  "status": "needs_changes",
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
      "action": "upsert_block",
      "detail": null
    },
    {
      "id": "skill.codex_project",
      "path": ".agents/skills/effigy",
      "status": "present",
      "action": "inspect_skill_guidance",
      "detail": "no project-local skill is required; installed Effigy guidance found at canonical root /Users/example/.agents/skills/effigy"
    }
  ],
  "text": "Effigy init check: needs_changes\n..."
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
      "id": "skill.codex_project",
      "category": "baseline",
      "execution_kind": "apply",
      "safety_class": "safe_apply",
      "applicability": "already_satisfied",
      "can_run_noninteractive": true,
      "summary": "Create or refresh a repo-local Effigy skill snapshot (explicit opt-in).",
      "reason": "no project-local skill is required; installed Effigy guidance found at canonical root /Users/example/.agents/skills/effigy; matching maintained files may be replaced",
      "recommended_command": "effigy init --apply-actions skill.codex_project"
    },
    {
      "id": "secrets_surface.inspect",
      "category": "secrets",
      "execution_kind": "inspect",
      "safety_class": "safe_check",
      "applicability": "applicable",
      "can_run_noninteractive": true,
      "summary": "Inspect declared secret keys without reading values.",
      "reason": "repo declares `[secrets]`",
      "recommended_command": "effigy secrets doctor"
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
    "secrets_surface.inspect"
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
      "id": "secrets_surface.inspect",
      "status": "inspected",
      "summary": "Inspect declared secret keys without reading values.",
      "reason": "command executed",
      "command": "effigy secrets doctor",
      "output": "Secrets: declared keys are readable; values are not shown."
    }
  ]
}
```

Notes:
- `effigy.init.v1` now covers the baseline managed setup flow and still also
  covers explicit starter emission
- checklist mode is a wider setup inventory than baseline `--check`
- `skill.codex_project` is already satisfied by default; select it explicitly
  to create or refresh the optional project-local snapshot
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

### 17) Lock conflict (`effigy.lock-wait.v2`)

Returned as `error.details` (exit 75) when the task's gate, or a shared lock
it declares, is held by a live run, with or without `--lock-wait-ms` /
`EFFIGY_LOCK_WAIT_MS`. The holder comes from the gate probe; holding the gate
is the holder's liveness, so there is nothing to unlock.

```json
{
  "schema": "effigy.lock-wait.v2",
  "schema_version": 2,
  "scope": "task:dev",
  "task": "dev",
  "holder": {
    "pid": 1234,
    "run": "agqvgrlm6bercdndodnbeu4tb4",
    "start": "1792398204:551",
    "state": "running"
  },
  "holder_started_at_epoch_ms": 1792398204551,
  "wait_timeout_ms": 150,
  "waited_ms": 150,
  "status_command": "effigy tasks status dev",
  "next": [
    "inspect the live run with `effigy tasks status dev`",
    "wait for it with `effigy --lock-wait-ms 600000 dev`"
  ]
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
    "version",
    "bootstrap",
    "bundle",
    "changelog",
    "config",
    "container",
    "contracts",
    "defer",
    "deploy",
    "deps",
    "help",
    "doctor",
    "distribution",
    "docs",
    "exec",
    "gateway",
    "init",
    "release",
    "scan",
    "secrets",
    "service",
    "state",
    "tasks",
    "test",
    "workspace",
    "fmt",
    "fmt-rust"
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

A failing task that already produced output — including a Rhai script that
logs and then throws — keeps this same payload. The command envelope stays
failed and carries the payload on `error.details`, so automation can read the
exact captured diagnostic:

```json
{
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "ok": false,
  "command": { "kind": "task", "name": "fail-rhai" },
  "result": null,
  "error": {
    "kind": "refused",
    "type": "RunnerError",
    "message": "command failed (json output available)",
    "details": {
      "schema": "effigy.task.run.v1",
      "schema_version": 1,
      "ok": false,
      "task": "fail-rhai",
      "command": "env EFFIGY_INTERNAL_SUPPRESS_HEADER='1' ... effigy script run --file 'scripts/fail.rhai'",
      "exit_code": 1,
      "stdout": "diagnostic line one\n",
      "stderr": "[error] Task failed\n  Runtime error: boom (line 3, position 1)\n"
    }
  }
}
```

### 22b) Task Plan (`effigy.task.plan.v1`)

```json
{
  "schema": "effigy.task.plan.v1",
  "schema_version": 1,
  "ok": true,
  "executed": false,
  "task": "build",
  "selector": "api/build",
  "command": "cargo run -p api --bin build",
  "cwd": "/workspace/app/api",
  "catalog": {
    "alias": "api",
    "root": "/workspace/app/api",
    "manifest": "/workspace/app/api/effigy.toml"
  },
  "runtime": {
    "target": "container",
    "container": "api-dev",
    "service": "api",
    "root": "/workspace/app/api"
  }
}
```

`executed` is always false. This payload is produced by
`effigy --json <selector> --plan`. `effigy --json <selector>` without `--plan`
still runs the task and returns `effigy.task.run.v1`.

The `runtime` section is additive and resolves through the same binding path
as execution. `target` is `host`, `container`, or `inline-container`; a
container target carries the resolved `container`, primary `service`, and the
owning catalog `root`. Host-scoped tasks report `"target": "host"`.

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
effigy --json state plan ./state/example-app-uat.toml
```

```json
{
  "schema": "effigy.state-stack.lineage.v1",
  "schema_version": 1,
  "lineage_id": "example-app-uat:Uat:structure+baseline-seed+legacy-content+uat-capture",
  "stack_name": "example-app-uat",
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
      "source": "oci://ghcr.io/example-app/content:legacy-2026-05-08",
      "artifact_source": {
        "source_ref": "oci://ghcr.io/example-app/content:legacy-2026-05-08",
        "kind": "app-specific"
      },
      "hook": "example-app:migrate:apply-legacy-content",
      "snapshot_identity": "old-site-db@2026-05-08"
    },
    {
      "index": 3,
      "key": "uat-capture",
      "role": "uat-capture",
      "apply_mode": "artifact",
      "environment_policy": "non-production",
      "source": "oci://ghcr.io/example-app/content:uat-authored-2026-05-08",
      "artifact_source": {
        "source_ref": "oci://ghcr.io/example-app/content:uat-authored-2026-05-08",
        "kind": "content-overlay"
      },
      "hook": "example-app:migrate:apply-uat-overlay",
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
      "source_ref": "oci://ghcr.io/example-app/content:legacy-2026-05-08",
      "artifact_kind": "app-specific",
      "operation": "planned-resolve"
    },
    {
      "layer_key": "uat-capture",
      "source_ref": "oci://ghcr.io/example-app/content:uat-authored-2026-05-08",
      "artifact_kind": "content-overlay",
      "operation": "planned-resolve"
    }
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/example-app-uat/plan.json",
  "written_history_path": ".effigy/reports/state/example-app-uat/history/20260508T143012Z-plan-example-app-uat-Uat-structure-baseline-seed-legacy-c.json"
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
  "stack_name": "example-app-uat",
  "environment": "uat",
  "lineage_id": "example-app-uat:Uat:structure+baseline-seed+legacy-content",
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
  "written_report_path": ".effigy/reports/state/example-app-uat/latest-capture.json",
  "written_history_path": ".effigy/reports/state/example-app-uat/history/20260508T143015Z-capture-example-app-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

Executed task-only apply:

```json
{
  "schema": "effigy.state-stack.apply.v1",
  "schema_version": 1,
  "ok": true,
  "executed": true,
  "stack_name": "example-app-uat",
  "environment": "uat",
  "lineage_id": "example-app-uat:Uat:structure+baseline-seed+legacy-content",
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
  "written_report_path": ".effigy/reports/state/example-app-uat/latest-capture.json",
  "written_history_path": ".effigy/reports/state/example-app-uat/history/20260508T143016Z-capture-example-app-uat-Uat-structure-baseline-seed-legacy-c.json"
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
  "stack_name": "example-app-uat",
  "source_environment": "uat",
  "capture_role": "uat-capture",
  "capture_mode": "uat-overlay",
  "parent_lineage_id": "example-app-uat:Uat:structure+baseline-seed+legacy-content",
  "created_at": "planned",
  "produced_layers": [
    {
      "key": "uat-capture-2026-05-08",
      "role": "uat-capture",
      "apply_mode": "artifact",
      "environment_policy": "non-production",
      "artifact_kind": "app-specific",
      "source_ref": "oci://ghcr.io/example-app/content:uat-capture-2026-05-08",
      "snapshot_identity": "uat-authored-content@2026-05-08",
      "depends_on": ["legacy-content"],
      "hook": "example-app:migrate:apply-uat-capture"
    }
  ],
  "capture_artifacts": [
    {
      "layer_key": "uat-capture-2026-05-08",
      "operation": "planned-capture",
      "ref": "oci://ghcr.io/example-app/content:uat-capture-2026-05-08",
      "digest": null,
      "artifact_report": null
    }
  ],
  "tasks": [
    {
      "name": "example-app:migrate:capture-uat-overlay",
      "status": "planned",
      "output": null,
      "error": null
    }
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/example-app-uat/latest-capture.json",
  "written_history_path": ".effigy/reports/state/example-app-uat/history/20260508T143017Z-capture-example-app-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

Executed capture tasks receive a versioned JSON context file. The path is also
available in `EFFIGY_STATE_CAPTURE_CONTEXT`.

```json
{
  "schema": "effigy.state-stack.capture-context.v1",
  "schema_version": 1,
  "stack_name": "example-app-uat",
  "parent_lineage_id": "example-app-uat:Uat:structure+baseline-seed+legacy-content",
  "capture_role": "uat-capture",
  "capture_mode": "uat-overlay",
  "source_environment": "uat",
  "key": "uat-capture-2026-05-08",
  "source": "captures/uat-overlay.json",
  "destination_ref": "oci://ghcr.io/example-app/content:uat-capture-2026-05-08"
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
        "stack_name": "example-app-legacy-source",
        "source_environment": "dev",
        "capture_role": "full-capture",
        "capture_mode": "full-snapshot",
        "parent_lineage_id": "example-app-legacy-source:Dev:legacy-source-root",
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
        "stack_name": "example-app-legacy-source",
        "source_environment": "dev",
        "capture_role": "full-capture",
        "capture_mode": "full-snapshot",
        "parent_lineage_id": "example-app-legacy-source:Dev:legacy-source-root",
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
  "stack_name": "example-app-uat",
  "source_environment": "uat",
  "capture_role": "uat-capture",
  "capture_mode": "uat-overlay",
  "parent_lineage_id": "example-app-uat:Uat:structure+baseline-seed+legacy-content",
  "created_at": "planned",
  "produced_layers": [
    {
      "key": "uat-capture-2026-05-08",
      "role": "uat-capture",
      "apply_mode": "artifact",
      "environment_policy": "non-production",
      "artifact_kind": "app-specific",
      "source_ref": "oci://ghcr.io/example-app/content:uat-capture-2026-05-08",
      "snapshot_identity": "uat-authored-content@planned",
      "depends_on": ["legacy-content"],
      "hook": "example-app:migrate:apply-uat-capture"
    }
  ],
  "capture_artifacts": [
    {
      "layer_key": "uat-capture-2026-05-08",
      "operation": "captured-local",
      "ref": "oci://ghcr.io/example-app/content:uat-capture-2026-05-08",
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
          "source": "oci://ghcr.io/example-app/content:uat-capture-2026-05-08",
          "reference": "ghcr.io/example-app/content:uat-capture-2026-05-08",
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
      "name": "example-app:migrate:capture-uat-overlay",
      "status": "executed",
      "context_path": ".effigy/state-captures/capture-context/example-app-uat/uat-capture-2026-05-08.json",
      "output": "capture complete"
    }
  ],
  "warnings": [],
  "written_report_path": ".effigy/reports/state/example-app-uat/latest-capture.json",
  "written_history_path": ".effigy/reports/state/example-app-uat/history/20260508T143016Z-capture-example-app-uat-Uat-structure-baseline-seed-legacy-c.json"
}
```

Explicitly pushed capture:

```json
{
  "schema": "effigy.state-stack.capture.v1",
  "schema_version": 1,
  "ok": true,
  "executed": true,
  "stack_name": "example-app-uat",
  "source_environment": "uat",
  "capture_role": "uat-capture",
  "capture_mode": "uat-overlay",
  "parent_lineage_id": "example-app-uat:Uat:structure+baseline-seed+legacy-content",
  "created_at": "planned",
  "produced_layers": [
    {
      "key": "uat-capture-2026-05-08",
      "role": "uat-capture",
      "apply_mode": "artifact",
      "environment_policy": "non-production",
      "artifact_kind": "app-specific",
      "source_ref": "oci://ghcr.io/example-app/content:uat-capture-2026-05-08",
      "snapshot_identity": "uat-authored-content@planned",
      "depends_on": ["legacy-content"],
      "hook": "example-app:migrate:apply-uat-capture"
    }
  ],
  "capture_artifacts": [
    {
      "layer_key": "uat-capture-2026-05-08",
      "operation": "pushed",
      "ref": "oci://ghcr.io/example-app/content:uat-capture-2026-05-08",
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
          "source": "oci://ghcr.io/example-app/content:uat-capture-2026-05-08",
          "reference": "ghcr.io/example-app/content:uat-capture-2026-05-08",
          "planned": false,
          "pushed": true,
          "digest": "sha256:8e5d2f...",
          "descriptor": {
            "reference": "ghcr.io/example-app/content:uat-capture-2026-05-08",
            "redacted_reference": "ghcr.io/example-app/content:uat-capture-2026-05-08",
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
  "written_report_path": ".effigy/reports/state/example-app-uat/latest-capture.json",
  "written_history_path": ".effigy/reports/state/example-app-uat/history/20260508T143017Z-capture-example-app-uat-Uat-structure-baseline-seed-legacy-c.json"
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

Schema change log (v0.15): `vault.identity` and `vault.unlock` were removed
from list and doctor payloads. They only labelled inert manifest settings;
`vault.path` and passphrase behavior are unchanged. The schema id stays at v1.

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
    "path": ".effigy/secrets/local.vault"
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
    "path": ".effigy/secrets/local.vault"
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

### External Skill Task Inventory (`effigy.skill.tasks.v1`)

```json
{
  "schema": "effigy.skill.tasks.v1",
  "schema_version": 1,
  "source": {
    "root": "/opt/skills/northstar",
    "manifest": "/opt/skills/northstar/effigy.toml",
    "evidence": [
      "operator selected `/opt/skills/northstar`",
      "canonical source root `/opt/skills/northstar`",
      "direct manifest `/opt/skills/northstar/effigy.toml`"
    ]
  },
  "catalog": {
    "alias": "northstar",
    "selectors": ["northstar/check", "northstar/refresh"]
  }
}
```

### External Skill Task Run (`effigy.skill.run.v1`)

```json
{
  "schema": "effigy.skill.run.v1",
  "schema_version": 1,
  "source": {
    "root": "/opt/skills/northstar",
    "manifest": "/opt/skills/northstar/effigy.toml",
    "evidence": ["canonical source root `/opt/skills/northstar`"]
  },
  "target": {
    "root": "/workspace/app",
    "resolution_mode": "Explicit",
    "evidence": ["resolved via explicit --repo override"],
    "repo_override": "/workspace/app"
  },
  "invocation_cwd": "/workspace/tools",
  "execution_cwd": "/workspace/app",
  "catalog_alias": "northstar",
  "selector": "northstar/check",
  "exit_status": 0,
  "task_output": {
    "schema": "effigy.task.run.v1",
    "schema_version": 1,
    "selector": "northstar/check",
    "task": "check",
    "command": "cargo check",
    "cwd": "/workspace/app",
    "ok": true,
    "exit_code": 0,
    "stdout": "",
    "stderr": ""
  }
}
```

### Container Hosts (`effigy.container.hosts.v1`)

`scope.kind` is `primary`, `worktree` (linked Git worktree), or
`ephemeral-clone` (full clone marked `effigy.runtimeScope = ephemeral`).
Worktree and ephemeral-clone scopes rewrite declared domains the same way.

```json
{
  "schema": "effigy.container.hosts.v1",
  "schema_version": 1,
  "ok": true,
  "checkout": "/worktrees/worker",
  "scope": {
    "kind": "worktree",
    "token": "abcdef0123456789abcdef0123456789",
    "host_key": "abcdef01",
    "shared_runtime_identity": false
  },
  "base_domain": {
    "declared": "app.test",
    "effective": "app-wabcdef01.test",
    "inferred": false,
    "warning": null
  },
  "cookie_domain": "app-wabcdef01.test",
  "relying_party_id": "app-wabcdef01.test",
  "routes": [
    {
      "kind": "http",
      "declared": "app.test",
      "effective": "app-wabcdef01.test",
      "tls": true,
      "service": "web",
      "origin": "https://app-wabcdef01.test"
    },
    {
      "kind": "tcp",
      "declared": "postgres.app.test",
      "effective": "postgres.app-wabcdef01.test",
      "tls": false,
      "service": "db",
      "origin": null
    }
  ]
}
```

### Container Retire (`effigy.container.retire.v1`)

```json
{
  "schema": "effigy.container.retire.v1",
  "schema_version": 1,
  "ok": true,
  "idempotent": true,
  "scope": {
    "token": "abcdef0123456789abcdef0123456789",
    "checkout": "/worktrees/worker",
    "compose_kind": "generated"
  },
  "skip_reason": null,
  "removed": [],
  "retained": [],
  "mismatch": [],
  "remaining": [],
  "record_removed": false
}
```

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

- [`017-json-output-contracts.md`](017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)
- [`072-artifact-commands-guide.md`](072-artifact-commands-guide.md)

## Next Step

After using one of these examples, run the real command with
`effigy --json <command>` in your target repo and tighten your integration
around the fields that are actually required for the workflow.
