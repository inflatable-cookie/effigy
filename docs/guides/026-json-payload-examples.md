# 026 - JSON Payload Examples

This guide provides realistic pretty-printed JSON payload samples for Effigy command contracts.

All examples assume canonical JSON mode:

```sh
effigy --json <command>
```

At runtime, these payloads are returned inside the top-level `effigy.command.v1` envelope in `result` (or in `error.details` for certain failures).


## Vision Alignment

- Primary tags: `CONTRACT`, `RELEASE`
- Target movement: payload examples remain trustworthy fixtures for schema-aware consumers and release validation.

## 1) Envelope Example (`effigy.command.v1`)

```json
{
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "ok": true,
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

## 2) Tasks (`effigy.tasks.v1`)

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
      "workspace",
      "task:build"
    ]
  }
}
```

## 3) Doctor (`effigy.doctor.v1`)

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

The same normalization applies to `scan.generated-assets` and `scan.attention-markers` when those scanners are enabled for doctor.

## 4) Doctor Explain (`effigy.doctor.explain.v1`)

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

## 5) Test Plan (`effigy.test.plan.v1`)

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

## 6) Test Results (`effigy.test.results.v1`)

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

## 7) Watch (`effigy.watch.v1`)

```json
{
  "schema": "effigy.watch.v1",
  "schema_version": 1,
  "ok": true,
  "runs": 1
}
```

## 8) Scan God Files (`effigy.scan.god-files.v1`)

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

## 9) Scan Generated Assets (`effigy.scan.generated-assets.v1`)

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

## 10) Init (`effigy.init.v1`)

```json
{
  "schema": "effigy.init.v1",
  "schema_version": 1,
  "ok": true,
  "path": "/workspace/app/effigy.toml",
  "dry_run": false,
  "written": true,
  "overwritten": false,
  "content": "# Baseline effigy.toml scaffold (phase 1)\n\n[tasks]\nping = \"printf ok\"\n"
}
```

## 11) Migrate (`effigy.migrate.v1`)

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

## 10) Config (`effigy.config.v1`)

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

## 11) Unlock (`effigy.unlock.v1`)

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

## 12) Completion (`effigy.completion.v1`)

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

## 13) Completion Candidates (`effigy.completion.candidates.v1`)

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
    "farmyard/api",
    "farmyard/build"
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
    "farmyard/api",
    "farmyard/build"
  ]
}
```

## 14) Scan Attention Markers (`effigy.scan.attention-markers.v1`)

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

## 15) Task Run (`effigy.task.run.v1`)

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

## Related Guides

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
