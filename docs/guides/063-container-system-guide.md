# 063 - Containers for Local Dev

Use this guide when a repo wants a host-clean local environment with:

- databases, queues, or other backing services
- a workspace container for Linux-native app work
- local domains and gateway routing
- explicit lifecycle control through `effigy container ...`

This is the practical command guide for the container commands.

Use:
- this guide for the direct `effigy container ...` commands and container
  manifest shape
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)
  for the model behind `system`, `workspace`, and `dev`
- [`067-catalog-services-reference.md`](./067-catalog-services-reference.md)
  for the shipped service catalog inputs

## Start Here

Shortest path:

1. declare a named container in `effigy.toml`
2. run `effigy container up`
3. use `effigy container shell` or a task such as `effigy dev`
4. run `effigy container down` when you want the environment gone

Common commands:

```sh
effigy container up
effigy container <NAME> up
effigy container down
effigy container <NAME> down
effigy container status --all
effigy container stats --all
effigy container <NAME> status
effigy container <NAME> logs
effigy container <NAME> shell
effigy container <NAME> data list
effigy container <NAME> data export <VOLUME> <PATH>
effigy container <NAME> data import <VOLUME> <PATH>
effigy container <NAME> data pull-production
effigy container <NAME> reset --keep-data
effigy container <NAME> eject
```

Default resolution:

- when `<NAME>` is omitted, Effigy resolves `[containers].default`
- if no default exists, Effigy fails clearly instead of guessing

Useful flags:

- `--attach` / `--detach` override manifest startup mode for `up`
- `--service <NAME>` focuses `logs` or `shell` on one service
- `--all` turns `status` and `stats` into cross-project views
- `--command <CMD>` runs one command string inside the service via `sh -lc`
- `--keep-data` preserves persistent named volumes during `reset`
- `--json` returns machine-readable payloads for non-interactive paths

## When To Use `container`

Use `container` when you want to:

- bring a declared local environment up or down directly
- inspect logs or open a shell in one container service
- export, import, reset, or eject environment data
- operate a simple compose-backed repo without the fuller `system` setup

If the question is instead:

- how should `dev`, `system`, and `workspace` fit together?
  - use [`064`](./064-system-workspace-and-dev-contract.md)
- which service inputs does a catalog support?
  - use [`067`](./067-catalog-services-reference.md)

## Manifest Shapes

Two shapes are supported.

- prefer catalog-driven generated compose for normal use
- use a repo-owned `compose_file` only when the repo genuinely needs direct
  compose ownership

### Catalog-Driven

```toml
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "effigy"
project_name = "my-app-dev"
primary_service = "app"

[containers.web.services.app]
catalog = "workspace-rust-bun"

[containers.web.services.db]
catalog = "postgres"

[containers.web.services.cache]
catalog = "redis"

[containers.web.dns]
domain = "my-app.test"
tls = true

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"
detach_timeout_secs = 10

[containers.web.host]
mounts = ["./:/workspace"]
```

Effigy generates runtime-owned compose output under:

- `.effigy/runtime/compose/.effigy-compose.generated.yml`
- `.effigy/runtime/compose/.effigy-catalog/<service>/...`

Treat that directory as runtime output, not repo-owned source.

### Repo-Owned `compose_file`

```toml
[containers.web]
driver = "colima"
startup = "attached"
profile = "effigy"
compose_file = "infra/dev/docker-compose.yml"
project_name = "my-app-dev"
primary_service = "app"

[containers.web.host]
ports = ["8080:80", "3306:3306"]
mounts = ["./:/workspace"]
```

Use this when:

- the repo already owns its own compose file
- the repo has taken local ownership through `effigy container <name> eject`
- the generated catalog path is not sufficient

## Core Rules

Current v1 rules:

- `driver` is effectively Colima-only
- `primary_service` is required
- `compose_file`, when used, must stay repo-relative
- repo-relative `host.mounts` may not escape the repo root unless they use the
  structured `external = true` form
- generated-compose data lifecycle stays on the product-owned path instead of
  widening direct `compose_file` ownership

## External Host Mounts

`[containers.<name>.host].mounts` accepts:

- legacy `"source:target[:options]"` strings for repo-relative sources
- structured tables for out-of-repo mounts

```toml
[containers.web.host]
mounts = [
  ".:/workspace",
  { host = "${PERSONAL_SSH_CONFIG}",
    container = "/home/dev/.ssh/config",
    external = true,
    options = ["ro"] },
]
```

Rules:

- `host` supports `${VAR}` and `~`
- without `external = true`, absolute resolved paths are rejected
- with `external = true`, the path may be absolute or repo-relative
- the source must exist when policy loads
- `options` is passed through to the compose layer unchanged

Use this with `effigy.local.toml` for per-machine mounts.

## Runtime Behavior

`effigy container up` means:

1. ensure the named Colima profile is running
2. bring the compose environment up
3. wait for declared environment readiness when present
4. either return immediately or attach, based on startup mode

Attached mode behavior:

- attached mode is the default when `startup = "attached"`
- Effigy owns that session lifecycle
- Ctrl+C follows the normal shutdown path
- `on_task_exit = "stop"` is the default shutdown posture
- interactive runs use Effigy's tabbed session runtime
- non-interactive runs fall back to stream-mode output

`effigy container down` is intentionally narrow:

- it tears down the compose environment
- it does not stop the Colima profile itself

Cross-project views:

- `effigy container status --all`
- `effigy container stats --all`

## Data Lifecycle

Generated-compose environments support:

- `data list`
- `data export`
- `data import`
- `data pull-production`
- `reset --keep-data`
- `eject`

These stay on the generated-compose path so Effigy can keep the runtime data
behavior predictable.

## DNS and Gateway

`[containers.<name>.dns]` integrates with the gateway:

- route registration and cleanup happen during container lifecycle
- `tls = true` works with `effigy gateway setup-tls`
- when TLS is enabled, plain HTTP requests are redirected to HTTPS

TCP catalog services such as postgres, mariadb, redis, and memcached also get
deterministic loopback aliases. That means host and container code can use the
same stable names without hand-written `/etc/hosts` edits.

## Host Runtime Fallback

On hosts with Docker CLI installed, Effigy uses `docker compose`.

When Colima exists but `docker` is not on `PATH`, Effigy falls back to:

- `colima nerdctl -- compose`

with the profile started under `containerd`.

## Current Limits

Still intentionally narrow:

- no broad multi-driver abstraction
- no richer per-service health DSL yet
- direct `compose_file` ownership remains narrower than generated-compose

## Related

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)

## Next Step

Use this page when deciding whether a repo should stay on direct
`effigy container ...` commands or move to the broader `system` and
`workspace` model.
