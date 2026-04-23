# 063 - Container System Guide

Effigy's shipped container surface gives a repo an explicit way to define named
local environments without making `effigy dev` globally special.

The current v1 goal is still explicit rather than magical:

- declare one or more named Colima-backed container environments in
  `effigy.toml`
- bring them up and down through `effigy container ...`
- project local domains through the shipped gateway when the manifest declares
  `[containers.<name>.dns]`
- keep host-facing ports, repo mounts, and the primary interactive service
  explicit
- support bounded generated-compose data lifecycle and shared backing services
  on the product-owned path
- make attached sessions shut down the environment on owner exit by default
- let repos expose named container sessions through ordinary task names
  without embedding raw compose commands

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Target movement: host-installed web-service sprawl -> manifest-driven local
  container environments with explicit lifecycle ownership

## Command Surface

```sh
effigy container up
effigy container <NAME> up
effigy container down
effigy container <NAME> down
effigy container status --all
effigy container stats --all
effigy container <NAME> status
effigy container <NAME> data list
effigy container <NAME> data export <VOLUME> <PATH>
effigy container <NAME> data import <VOLUME> <PATH>
effigy container <NAME> data pull-production
effigy container <NAME> logs
effigy container <NAME> shell
effigy container <NAME> reset --keep-data
effigy container <NAME> eject
```

Default-resolution rule:

- when `<NAME>` is omitted, Effigy resolves `[containers].default`
- if no default exists, Effigy fails clearly instead of guessing

Useful flags:

- `--attach` / `--detach` override the manifest startup mode for `up`
- `--service <NAME>` focuses `logs` or `shell` on one explicit service
- `--all` turns `status` and `stats` into cross-project views
- `--command <CMD>` runs one shell command string inside the service via
  `sh -lc`
- `--keep-data` preserves generated-compose persistent named volumes during
  `reset`
- `--json` returns machine-readable payloads for non-interactive paths

## Local Dev Chain

For repos using the fuller `v0.3` local-dev surface, `container` now sits in a
clearer operator chain:

1. use `effigy service` to inspect or extract bundled service fragments
2. use `effigy container up` to bring the local environment to ready state
3. use `effigy gateway` when the manifest declares local domains or TLS-backed
   routes
4. use `effigy exec ...` for one ad-hoc command inside the dev container
5. use a repo-owned managed task such as `effigy dev` when the repo wants one
   named session front door

The container command is the middle of that chain, not the whole story.

## Task Composition

Repos can now expose one attached container session through an ordinary task
alias:

```toml
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[tasks."dev:services"]
workspace = "app"
```

Rules:

- `workspace = "app"` resolves through `systems.default` unless the task sets
  an explicit `system = "<name>"`
- the workspace then resolves its backing container from
  `[systems.<name>.workspaces.<workspace>]`
- this keeps `effigy dev` repo-owned instead of making `dev` globally special
- the task path uses the same attached container runtime as
  `effigy container up`

## Manifest Contract

Two shapes are supported. Use catalog-driven services (the modern path)
whenever possible; fall back to an explicit `compose_file` only when the repo
needs direct compose ownership.

### Catalog-Driven (Generated Compose)

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

Effigy generates the compose file under `.effigy/runtime/compose/`:

- `.effigy/runtime/compose/.effigy-compose.generated.yml`
- `.effigy/runtime/compose/.effigy-catalog/<service>/Dockerfile` (when a
  catalog service ships a Dockerfile)
- `.effigy/runtime/compose/.effigy-catalog/<service>/` (catalog-owned config)

This directory is runtime-only: it is rewritten every time the manifest
changes, and should be ignored by git.

Catalog services currently ship for: `workspace-rust-bun`, `postgres`,
`pgweb`, `mariadb`, `redis`, `memcached`, `mailpit`, `minio`, `elasticsearch`,
`phpmyadmin`, `nginx`, and `php-fpm`. See the catalog services reference for
the supported input surface for each.

### User-Owned `compose_file` (Escape Hatch)

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

Use this when the repo already maintains its own compose file or has escaped
the catalog path via `effigy container <name> eject`.

Current v1 rules (both shapes):

- `driver` is effectively Colima-only
- `compose_file` (when used) must stay repo-relative
- `primary_service` is required
- `host.mounts` must stay repo-relative and may not escape the repo root
- host access is explicit through declared `ports` on the user-owned path;
  catalog-driven services own their own published-port metadata
- readiness may be declared through one environment `health.check`
- gateway route ownership stays explicit through `[containers.<name>.dns]`
- the richer data lifecycle commands (`data list`, `data export`, etc.) stay
  on the generated-compose path instead of widening direct `compose_file`
  ownership

## Runtime Behavior

`effigy container up` does not mean only "start a VM".

It means:

1. ensure the named Colima profile is running
2. bring the compose environment up
3. wait for the declared environment health gate when present
4. either return immediately (`detached`) or attach to the live session

Attached-session rule:

- attached mode is the default when `startup = "attached"`
- Effigy owns the lifecycle of that session
- Ctrl+C routes through the same shutdown path as a normal owner exit
- `on_task_exit = "stop"` is the default shutdown posture
- interactive terminals now use Effigy's multi-tab session runtime with:
  - an `overview` tab that refreshes status and shutdown policy
  - a primary-service log tab derived from `primary_service`
- non-interactive runs fall back to a stream-mode overview plus primary-service
  log follow so task aliases can still run honestly in plain subprocess
  environments

Startup interruption rule:

- Ctrl+C during `effigy container up` (attached or detached) surfaces a
  `[info] shutdown requested; stopping container cleanly...` acknowledgement
  and unwinds through compose teardown plus gateway deregistration before
  returning to the terminal
- this applies even while compose is still pulling or creating images; the
  detached bring-up path used to ignore the stop flag during that window

Current `down` behavior is intentionally narrow:

- it tears down the compose environment
- it does not stop the Colima profile itself

Cross-project views:

- `effigy container status --all` shows running Effigy-managed environments
  across repos
- `effigy container stats --all` adds bounded CPU and memory usage for those
  running environments when runtime stats are available

Generated-compose data lifecycle:

- `data list` shows Effigy-managed named volumes with persistent versus
  ephemeral classification
- `data export` and `data import` move bounded generated-compose volumes
  through the product surface
- `data pull-production` runs one repo-owned production-pull hook through the
  container data contract
- `reset --keep-data` keeps persistent named volumes while still removing
  ephemeral ones
- `eject` gives the repo an explicit way to take compose ownership locally

## Host Runtime Fallback

On hosts with the Docker CLI installed, Effigy uses `docker compose`.

On hosts where Colima exists but `docker` is not on `PATH`, Effigy falls back
to `colima nerdctl -- compose` and starts the profile with
`--runtime containerd`.

That matters on clean laptops where the user wants Colima but does not want a
separate Docker Desktop-style dependency just to operate local web services.

## Consumer Proof

The first real consumer proof for this surface used `contact-patch`:

- named/default container registry added under `[containers]`
- detached bring-up proved against the repo's existing `docker-compose.yml`
- running status and graceful teardown were exercised on the real machine
- repo-owned task composition is now also proven through
  `tasks."dev:services".workspace = "app"` plus
  `[systems.dev.workspaces.app].container = "web"`
- the proof also exposed and closed two host-level gaps:
  - fallback to `colima nerdctl` when `docker` is absent
  - Colima running-state detection needed to accept lowercase `running`

The `108` widening proof also showed one remaining environment-shaped edge:

- that edge is now closed:
  startup-phase stop requests and nested log-follow subprocess trees now route
  through one reliable shutdown path, and the real `contact-patch` proof exits
  cleanly under timed `SIGINT`

Gateway integration is now part of the shipped path:

- `[containers.<name>.dns]` registers and removes route-table entries through
  container lifecycle
- `[containers.<name>.dns].port` lets one route choose a specific declared host
  port
- `[containers.<name>.dns].tls = true` works with `effigy gateway setup-tls`
  and the route-owned certificate path

### TCP DNS Aliases and Loopback IPs

Catalog-driven services that speak a TCP protocol (postgres, mariadb, redis,
memcached) automatically get a TCP-level gateway alias on a deterministic
loopback IP. No manifest flag is required: Effigy inspects the catalog type
and allocates one loopback IP per project/service identity.

Example resulting route visible in `effigy gateway status`:

```
- db.my-app.test -> tcp 127.1.0.1:5432 -> 127.0.0.1:15432 [source=container, project=my-app-dev, tls=off]
```

What this means operationally:

- `db.my-app.test:5432` resolves on the host without needing a `/etc/hosts`
  edit; the gateway's local resolver answers the alias
- the service is reachable at a stable loopback IP (`127.1.0.1`) even if the
  published host port (`15432`) shifts between projects
- each project+service identity gets its own IP from the pool `127.1.0.1`
  through `127.1.0.50`, so two repos can run `postgres` at the same time
  without port collisions
- allocations are persisted at `~/.effigy/gateway/loopback-ips.json`; delete
  that file only when you intentionally want to re-assign identities
- the alias hostname is also patched into the primary service container's
  `/etc/hosts` so app code inside the dev container can reach `db.my-app.test`
  over the same alias it uses on the host

Loopback IP allocation runs through the gateway even when the system uses a
user-owned `compose_file`; declare `[containers.<name>.dns]` to opt in.

Effigy itself now also uses this surface for local Linux release rehearsal:

- `release:linux:env` opens the named `linux-release` container as an attached
  operator session when manual inspection is needed
- `release:linux:rehearse` brings the same container up, builds the Linux
  binary inside Ubuntu 22.04, runs `smoke:release`, checks the GLIBC floor,
  copies the binary into `.effigy/linux-release/artifacts/`, and tears the
  environment down again

## Current Limits

This is now a stronger v1 operator surface, but it is not the final container
product shape.

Still explicitly open:

- no broad multi-driver abstraction
- no richer per-service health DSL yet
- generated-compose-only data lifecycle and shared-service widening; direct
  `compose_file` ownership stays intentionally narrower

## Related

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)
- [`docs/roadmaps/g02/006-colima-container-environment-contract.md`](../roadmaps/g02/006-colima-container-environment-contract.md)
