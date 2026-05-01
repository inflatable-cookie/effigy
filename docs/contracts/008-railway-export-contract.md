# 008 - Railway Export Contract

This contract defines the first Railway export target for `deploy.model.v1`.

It does not implement Railway export yet. It defines the file shape, mapping
rules, and block conditions the first Railway adapter must obey.

Provider basis:

- Railway config-as-code is per service, using `railway.toml` or
  `railway.json` in the service source tree.
- The config file can define build and deploy settings such as build command,
  start command, pre-deploy command, and healthcheck path.
- Public domains are not created from config-as-code; they remain
  operator-managed through Railway's service networking surface.
- Service variables and reference variables are operator-managed, not part of
  service config-as-code.

Source basis:

- Railway config-as-code:
  https://docs.railway.com/config-as-code/reference
- Railway healthchecks:
  https://docs.railway.com/deployments/healthchecks
- Railway variables/reference variables:
  https://docs.railway.com/variables
- Railway domains:
  https://docs.railway.com/networking/domains/working-with-domains

## First command boundary

The first Railway export surface should stay file-oriented and inspectable.

Planned command direction:

```bash
effigy deploy export railway
effigy deploy export railway --json
effigy deploy export railway --plan
```

The adapter should consume the already-derived `deploy.model.v1` model. It
must not reopen bundle heuristics or inspect provider-specific repo state
directly.

## First generated file set

The first Railway export should produce a provider bundle, not one global root
file.

First generated files:

- `.effigy/export/railway/services/front/railway.toml`
- `.effigy/export/railway/services/admin/railway.toml`
- `.effigy/export/railway/services/api/railway.toml`
- `.effigy/export/railway/services/jobs/railway.toml` when the worker exists
- `.effigy/export/railway/report.json`

The exact output directory may still be controlled by `--path`, but the first
adapter should preserve a service-per-directory shape inside that path.

Why:

- Railway config-as-code is service-local rather than project-global
- domains and variables still need operator follow-up
- the export therefore needs an explicit report artifact, not just config files

## Service mapping

### `static` services

Map a `deploy.model.v1` static service to one Railway service config file:

- builder
  - default to `RAILPACK`
- build command
  - from `service.build.command`
- root/service source
  - from `service.source_root`
- start command
  - omitted in the first batch for purely static outputs

The first Railway contract assumes static sites are exported as build-only
services whose deploy behavior is handled by Railway's static hosting path.
If the real Railway implementation requires a different bounded shape, that
must be documented before adapter implementation opens.

If a static service does not carry an explicit output path, export must block.
If a static service needs SPA fallback behavior and the model does not carry
`output.fallback`, export must block.

### `web` services

Map a `deploy.model.v1` web service to one Railway service config file:

- builder
  - default to `RAILPACK`
- build command
  - from `service.build.command`
- start command
  - from `service.start.command`
- pre-deploy command
  - from `service.release.command` when present
- healthcheck path
  - from `service.health.path` when `kind = "http"`

The service must listen on Railway's injected `PORT`. The first adapter should
not generate a fixed public port policy.

### `worker` services

Map a `deploy.model.v1` worker service to one Railway service config file:

- builder
  - default to `RAILPACK`
- build command
  - from `service.build.command`
- start command
  - from `service.start.command`
- pre-deploy command
  - omitted in the first batch

### `cron` services

Do not implement `cron` in the first Railway batch.

If the model includes any `cron` service, export must block with an explicit
unsupported-role error until the model also carries schedule truth.

## Backing-service mapping

### `postgres`

Do not generate a Railway Postgres resource in the first Railway export batch.

Instead:

- keep `postgres` in the export report as a required provider-side service
- keep `DATABASE_URL` in the report as a required reference-variable step for
  any consuming service

Why:

- Railway service config files do not create project resources by themselves
- database/resource creation belongs to Railway project setup, not per-service
  config-as-code

### Other backing services

Do not implement other backing-service kinds in the first Railway batch.

If the model includes:

- `mariadb`
- `redis`
- `memcached`
- `object_storage`

export must block unless a later contract explicitly adds them.

## Secret mapping

The first Railway adapter should split secret handling into two cases.

### Provider/project follow-up refs

When a service references `DATABASE_URL`, the adapter should not pretend this
is solved in config-as-code. It should emit a report entry that tells the
operator to:

- create or attach a Postgres service in Railway
- set a service variable or reference variable for `DATABASE_URL`

### Operator-supplied refs

For all other secret refs, the adapter should emit report entries only.

Do not generate fake secret values into `railway.toml`.

## Domain mapping

The first Railway adapter should not attempt to create public domains in code.

Instead:

- carry service domains into `report.json`
- mark them as operator follow-up
- distinguish between:
  - acceptable Railway-generated public domains
  - desired custom domains from the source model

## Export report shape

The first Railway export should include a machine-facing `report.json` that
records:

- generated service files
- required provider-side resources
- required variable or reference-variable setup
- required domain setup
- warnings that do not block export
- errors that would have blocked export

This report is the Railway equivalent of the first Render export's thin file
set: it makes the remaining operator steps explicit instead of hiding them in
docs prose.

## Warning and block policy

### Export-blocking conditions

The first Railway adapter must block when:

- a service role is unsupported
- a web or worker service is missing a required `start.command`
- a static service is missing `output.path`
- a backing-service kind is unsupported
- a service requires persistent app volumes the adapter cannot map honestly

### Warning-only conditions

The first Railway adapter may still export with warnings when:

- provider-side Postgres creation remains operator-owned
- service variables or reference variables remain operator-owned
- custom domain setup remains operator-owned
- service plan/size/scaling policy is still absent from the model

## Next Task

Open the first bounded Railway export implementation batch once the adapter
boundary is accepted and any static-hosting ambiguity is closed.
