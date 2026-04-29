# Config Shapes

Realistic snippets for the config sections an agent is likely to author or
modify. Not a full reference — see `docs/guides/025-command-reference-matrix.md`
for the complete schema.

Effigy splits config across:

- `effigy.toml` — project-level (catalogs, systems, containers, release).
- `tasks/effigy.tasks.toml` — task definitions (kept separate so task
  changes don't churn the project manifest).

## `[tasks]`

Tasks can be shell strings, refs to other tasks, or Rhai scripts. Examples:

```toml
[tasks]
# Shell string
"fmt:check" = "cargo fmt --all -- --check"

# Aggregator: chain of task refs
"qa:ci:fast" = [
  { task = "qa:ci:test" },
  { task = "qa:ci:doc" },
  { task = "qa:released-surface" },
  { task = "qa:ci:json" },
]

# Rhai script
"link:local" = [{ rhai = "scripts/rhai/install-local-bin-links.rhai" }]

# Mixed chain
"bootstrap:local" = [
  { task = "install:local" },
  { task = "link:local" },
]

# Task with explicit run block (for richer config)
[tasks."smoke:release"]
run = [{ rhai = "scripts/rhai/check-release-smoke.rhai" }]
```

## `[systems.<name>]`

Systems group containers and workspaces. One system can have many workspaces.

```toml
[systems.release]
default_workspace = "linux"

[systems.release.workspaces.linux]
container = "linux-release"
```

## `[containers.<name>]`

Container definitions live in catalogs (often imported from a shared catalog
crate). Repo-local containers go here:

```toml
[containers.linux-release]
image = "ghcr.io/inflatable-cookie/effigy-linux-release:latest"
volumes = [
  { source = ".", target = "/workspace", mode = "rw" },
]
```

For Effigy-defined catalogs (`workspace-rust-bun`, `php-fpm`, `node`), import
from the workspace catalog crate rather than redefining.

## `[bootstrap]`

First-run setup steps, executed by `effigy bootstrap`:

```toml
[bootstrap]
steps = [
  { run = "cargo build --bin effigy" },
  { run = "mkdir -p .local-install/bin" },
  { task = "link:local" },
]
```

## `[release]`

Release configuration: gates, manifest path, distribution targets.

```toml
[release]
manifest_path = "release/manifest.toml"
gates = [
  "fmt",
  "clippy",
  "test",
  "docs",
  "json-contracts",
  "released-surface",
]
```

The actual release manifest (`release/manifest.toml`) is separate and tracks
version, changelog cutoff, distribution channels.

## Catalog imports

Catalogs let you share container/system definitions across repos:

```toml
[catalog]
imports = [
  { crate = "effigy-containers", catalog = "workspace-rust-bun" },
]
```

After importing, reference the container by its catalog name:

```toml
[systems.dev.workspaces.main]
container = "workspace-rust-bun"
```
