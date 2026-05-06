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

## Host prerequisites (macOS)

Container features are host-dependent. Install only what the features you use
require.

### Containers (required)

Effigy’s v1 container driver is **Colima**.

```bash
brew install colima
```

Effigy starts the configured Colima **profile** on demand (from
`[containers.<name>].profile`). You can still manage it directly when needed:

```bash
colima list
colima stop --profile <profile>
```

Docker is optional. If you want `docker` available on your host (or your team
standardizes on it), install it via Homebrew:

```bash
brew install docker docker-compose
```

### HTTPS gateway routes (optional)

If you use local domains with `tls = true`, install `mkcert` and run the
one-time trust-store install:

```bash
brew install mkcert
mkcert -install
```

Effigy also provides `effigy gateway setup-tls` as the “do the right thing”
helper for mkcert-backed TLS.

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
- `reset` preserves persistent named volumes by default
- `--wipe-data` deletes persistent named volumes during `reset`
- `--keep-data` remains accepted as a compatibility alias for reset's default
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

## Task Activation

Container-backed tasks now use two distinct lifecycle models.

### Public Shell Sessions

Interactive public shell/session surfaces include:

- `effigy dev`
- `effigy workspace`
- tasks that end in `stay_in_shell = true`

These are session-owned lifecycles:

- Effigy prepares the runtime for an interactive shell or handoff
- public gateway routes are reconciled before the shell opens
- the session decides shutdown behavior on exit
- `on_task_exit` and shell ownership matter here

This is the right model when the user is being dropped into a live workspace.

### Non-Shell Container Tasks

Non-interactive container-backed tasks include:

- explicit tasks with `run_in = "container"`
- deferred requests with `[defer].run_in = "container"`
- bootstrap `run` steps that route into a container but do not open a shell

These use shared task activation instead of session ownership:

- Effigy auto-starts the runtime when needed
- sibling-service bring-up and exec-readiness recovery run before dispatch
- public gateway/runtime route registration is reconciled when the container
  declares a gateway surface
- if the task had to start the runtime, or the runtime was already under an
  active task lease, Effigy refreshes a temporary host-container lease

Default lease behavior:

- lease duration defaults to 5 minutes
- the lease is refreshed on reuse
- the runtime shuts down after the lease expires unless the user explicitly
  keeps it up through `effigy container up`, `effigy dev`, or another owned
  session

The goal is one warm-runtime contract for non-shell tasks instead of separate
behavior for deferred versus explicit container routing.

## Data Lifecycle

Generated-compose environments support:

- `data list`
- `data export`
- `data import`
- `data pull-production`
- data-safe `reset`
- explicit `reset --wipe-data`
- `eject`

These stay on the generated-compose path so Effigy can keep the runtime data
behavior predictable.

`data import`, `data pull-production`, and `reset --wipe-data` are guarded
because they can overwrite or delete local generated-compose data. In a real
interactive terminal, Effigy asks for confirmation and defaults to no. In
non-interactive or JSON mode it does not prompt; automation must pass `--yes`
when the data change is intentional.

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
