# 004 - Underlay Reference Deploy Model Example

This example shows what `deploy.model.v1` should look like for the shipped
`underlay-reference` repo.

It is not a provider export. It is the neutral model that provider adapters
should consume.

Source repo:

- `/Users/tom/Dev/projects/underlay-reference`

Relevant bundle inputs:

- `host = "acme.test"`
- `project_name = "underlay-reference-dev"`
- `workspace_subdir = "underlay-reference"`
- `databases = ["acme"]`
- `dirs.api = "acme-api"`
- `dirs.front = "acme-front"`
- `dirs.admin = "acme-admin"`
- `dirs.client = "acme-client"`
- `dirs.ui = "acme-ui"`

## Example model

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
      "name": "admin",
      "role": "static",
      "runtime": "node",
      "source_root": "acme-admin",
      "build": {
        "command": "bun x vite build"
      },
      "output": {
        "kind": "directory",
        "path": "build",
        "fallback": "index.html"
      },
      "domains": [
        "admin.acme.test"
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
        "command": "cargo build"
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
    },
    {
      "name": "jobs",
      "role": "worker",
      "runtime": "rust",
      "source_root": "acme-api",
      "build": {
        "command": "cargo build"
      },
      "start": {
        "command": "cargo run -p acme-jobs"
      },
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
        "api",
        "jobs"
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
      "host": "admin.acme.test",
      "service": "admin",
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
        "api",
        "jobs"
      ],
      "required": true,
      "source": "operator",
      "notes": "Managed Postgres connection string"
    }
  ],
  "warnings": []
}
```

## What this example proves

This example locks a few important decisions down:

- `front` and `admin` export as static services, not fake long-running web
  processes
- static services carry their deployable output path directly in the model
- static services also carry their SPA fallback file directly in the model
- `api` is the only default public web process
- the shared Underlay API health probe promotes as `/v1/health`
- `db:migrate` is the first promoted release-hook seam
- `jobs` is optional but real when the API package exposes a `jobs` task
- local-only helper services do not leak into the production model
- the first export lane can stay honest with warning entries instead of
  inventing production detail

## What this example still leaves open

This example does not yet settle:

- whether provider adapters should attach default runtime env for Bun or Rust

Those are the next design questions the implementation lane should answer.

## Next Task

Use this example as the first implementation fixture for:

- `effigy deploy model --json`
- model-shape tests
- first Render and Railway export expectations
