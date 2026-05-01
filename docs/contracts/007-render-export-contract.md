# 007 - Render Export Contract

This contract defines the first provider-export target for
`deploy.model.v1`.

It does not implement Render export yet. It defines the file shape, mapping
rules, and block conditions the first Render adapter must obey.

Provider basis:

- Render Blueprints use a root `render.yaml` file.
- Blueprint services can be `web`, `worker`, or `cron`.
- static sites are modeled as `type: web` with `runtime: static`.
- web services can declare `healthCheckPath`.
- release-style migration hooks map to `preDeployCommand`.
- managed Postgres resources live in the root `databases` list.
- service env vars can reference managed Postgres values with `fromDatabase`.

## First command boundary

The first Render export surface should stay file-oriented and inspectable.

Planned command direction:

```bash
effigy deploy export render
effigy deploy export render --json
effigy deploy export render --plan
```

The adapter should consume the already-derived `deploy.model.v1` model. It
must not reopen bundle heuristics or inspect provider-specific repo state
directly.

## First generated file set

The first generated Render export should produce:

- `render.yaml`

The export command may also emit a machine-facing report, but the generated
artifact bundle itself should stay minimal in the first batch.

## Root file shape

The first file should use the Render Blueprint root fields:

- `services`
- `databases`

Do not generate:

- projects/environments
- environment groups
- preview-environment config
- provider-owned scaling or plan overrides unless the model grows that truth

## Service mapping

### `static` services

Map a `deploy.model.v1` static service to a Render static site:

- `type: web`
- `runtime: static`
- `rootDir`
  - from `service.source_root`
- `buildCommand`
  - from `service.build.command`
- `staticPublishPath`
  - from `service.source_root` plus `service.output.path`
- `routes`
  - from `service.output.fallback` when SPA fallback behavior is required
- `domains`
  - from `service.domains`

If a static service does not carry an explicit output path, export must block.
If a static service needs SPA fallback behavior and the model does not carry
`output.fallback`, export must block.

### `web` services

Map a `deploy.model.v1` web service to a Render web service:

- `type: web`
- `runtime`
  - from `service.runtime`
- `rootDir`
  - from `service.source_root`
- `buildCommand`
  - from `service.build.command`
- `startCommand`
  - from `service.start.command`
- `preDeployCommand`
  - from `service.release.command` when present
- `healthCheckPath`
  - from `service.health.path` when `kind = "http"`
- `domains`
  - from `service.domains`

### `worker` services

Map a `deploy.model.v1` worker service to a Render background worker:

- `type: worker`
- `runtime`
  - from `service.runtime`
- `rootDir`
  - from `service.source_root`
- `buildCommand`
  - from `service.build.command`
- `startCommand`
  - from `service.start.command`
- `preDeployCommand`
  - omitted in the first batch

### `cron` services

Do not implement `cron` in the first Render batch.

If the model includes any `cron` service, export must block with an explicit
unsupported-role error until the model also carries schedule truth.

## Backing-service mapping

### `postgres`

Map a required managed `postgres` backing service to one Render Postgres
resource in the top-level `databases` list.

The first adapter should treat managed Postgres as provider-owned and not ask
the operator for a `DATABASE_URL` secret when that secret can be satisfied from
the generated Render Postgres resource.

### Other backing services

Do not implement other backing-service kinds in the first Render batch.

If the model includes:

- `mariadb`
- `redis`
- `memcached`
- `object_storage`

export must block unless a later contract explicitly adds them.

## Secret mapping

The first Render adapter should split secret handling into two cases.

### Provider-satisfied refs

When a service references `DATABASE_URL` and the export also generates a Render
Postgres resource, the adapter should emit:

- `envVars`
  - `key: DATABASE_URL`
  - `fromDatabase`
    - database name
    - `property: connectionString`

This is not a missing operator secret.

### Operator-supplied refs

For all other secret refs, the adapter should emit:

- `envVars`
  - `key: <SECRET_NAME>`
  - `sync: false`

Those refs should also remain in the export report as operator actions.

## Warning and block policy

### Export-blocking conditions

The first Render adapter must block when:

- a service role is unsupported
- a web or worker service is missing a required `start.command`
- a static service is missing `output.path`
- a backing-service kind is unsupported
- a service requires persistent app volumes the adapter cannot map honestly

### Warning-only conditions

The first Render adapter may still export with warnings when:

- operator-managed secrets remain
- service plan/size/scaling policy is still absent from the model
- domain ownership is present but DNS cutover remains operator-owned

## Next Task
Open the first bounded Render export implementation batch once the model emits
static fallback ownership directly.
