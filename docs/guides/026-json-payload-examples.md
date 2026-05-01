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
  `Tasks`, `Doctor Explain`, `Task Run`
- health, repo checks, or diagnostics:
  `Doctor` and the `Scan *` payloads
- test automation:
  `Test Plan`, `Test Results`, `Watch`
- bootstrap and setup:
  `Bootstrap`, `Deploy Model`, `Init`, `Migrate`, `Config`, `Unlock`
- shell completion or editor integration:
  `Completion`, `Completion Candidates`

Companion references:

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)

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
    "version": "0.3.1",
    "active_version": "v0.3.1+local.abc123",
    "display_version": "v0.3.1+local.abc123"
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
    "name": "underlay-reference",
    "bundle": "underlay",
    "project_name": "underlay-reference-dev",
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
  "warnings": []
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
  "warnings": []
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

### 14) Init (`effigy.init.v1`, `effigy.init.list.v1`)

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

Notes:
- starters may emit more than one file (for example `underlay` emits one
  `effigy.toml` plus one starter-owned Rhai fragment); each file has its own
  `target`/`path`/`contents`/`existed`/`written` entry
- `guidance` carries optional post-emission text the starter wants operators
  to see (for example environment variables to export or a follow-up command)
- `dry_run: true` keeps `written: false` on every file entry; `--force` is
  the only way to set `overwritten: true`

`effigy init --list`:

```json
{
  "schema": "effigy.init.list.v1",
  "schema_version": 1,
  "starters": [
    { "name": "minimal", "description": "Minimal Effigy scaffold" },
    { "name": "underlay", "description": "Underlay dev-contract starter" },
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

### 18) Completion (`effigy.completion.v1`)

```json
{
  "schema": "effigy.completion.v1",
  "schema_version": 1,
  "ok": true,
  "shell": "bash",
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

## Next Step

After using one of these examples, run the real command with
`effigy --json <command>` in your target repo and tighten your integration
around the fields that are actually required for the workflow.
