# 002 - Production Deployment Model

This contract defines the first neutral production deployment model for
Effigy.

It is the model that provider export adapters should consume.

It is not a local-dev compose export, and it is not provider-specific config.

## Purpose

Effigy needs one inspectable model between:

- effective manifest, including any bundle-owned `[deploy.model]` content
- provider-specific deployment output

That middle layer should let Effigy:

- explain what it thinks the production shape is
- export provider files without duplicating bundle heuristics in each adapter
- warn clearly when human production policy is still required

## First target

The first target for this model is Underlay.

That means the first version should be strong for:

- frontend plus admin plus API apps
- jobs workers
- standard backing services
- managed-platform deployment files

It should not pretend Decodelabs is fully production-exportable yet.

## Command surface

The neutral model should be inspectable directly.

Primary command direction:

```bash
effigy deploy model --json
```

Provider export should build on that model:

```bash
effigy deploy export render
effigy deploy export railway
```

The first model output should use schema id:

- `deploy.model.v1`

## Top-level model

The deployment model should carry:

- `schema`
  - fixed schema id
- `schema_version`
  - schema version number
- `app`
  - repo or project identity
- `services`
  - production application services
- `backing_services`
  - database, cache, object-storage, or similar dependencies
- `domains`
  - public hostnames and ownership
- `secrets`
  - required secret names and whether Effigy knows only the reference or also
    a safe default
- `warnings`
  - unresolved production questions or provider gaps

Recommended first envelope:

```json
{
  "schema": "deploy.model.v1",
  "schema_version": 1,
  "app": {
    "name": "contact-patch",
    "bundle": "underlay",
    "project_name": "contact-patch"
  },
  "services": [],
  "backing_services": [],
  "domains": [],
  "secrets": [],
  "warnings": []
}
```

## App contract

The `app` object should carry only repo-wide identity, not service-local
details.

First shape:

- `name`
- `bundle`
  - optional when the repo is not bundle-backed
- `project_name`
- `source_root`
  - relative repo path when export does not target the repo root
- `notes`
  - optional short operator-facing notes

## Service list contract

`services` should be the main application deployment units.

They should be fully expanded by the time the model is emitted. Adapters should
not need to reconstruct service graphs from bundle internals.

## Service roles

Application services should use explicit runtime roles:

- `web`
- `worker`
- `cron`
- `static`

That role controls how adapters translate the service into a provider target.

Examples:

- `web`
  - long-running HTTP service
- `worker`
  - long-running job consumer
- `cron`
  - scheduled command owner
- `static`
  - deployable asset-only surface if a provider supports it

## Application service contract

Each application service should carry at least:

- `name`
- `role`
- `runtime`
  - `php`
  - `node`
  - `rust`
  - or future supported runtime family
- `source_root`
  - repo-relative package or service root
- `build`
  - build command
- `start`
  - start command
- `release`
  - optional pre-start or release command
- `health`
  - optional health path or command
- `output`
  - required for `static` services
- `port`
  - internal listen port when relevant
- `domains`
  - domains served by this service
- `env`
  - non-secret env values
- `secret_refs`
  - secret keys the operator must populate
- `volumes`
  - persistent storage needs
- `warnings`
  - service-specific unresolved questions

Recommended first shape:

```json
{
  "name": "api",
  "role": "web",
  "runtime": "rust",
  "source_root": "nursery",
  "build": {
    "command": "cargo build --release -p nursery-api"
  },
  "start": {
    "command": "./target/release/nursery-api"
  },
  "release": {
    "command": "cargo run -p nursery-api-migrate"
  },
  "health": {
    "kind": "http",
    "path": "/health"
  },
  "port": 41001,
  "domains": [
    "api.songsprout.test"
  ],
  "env": {
    "RUST_LOG": "info"
  },
  "secret_refs": [
    "DATABASE_URL",
    "SESSION_SECRET"
  ],
  "volumes": [],
  "warnings": []
}
```

### Service field rules

- `name`
  - stable logical service name, not provider-specific id
- `role`
  - one of the defined runtime roles
- `runtime`
  - runtime family, not image name
- `source_root`
  - repo-relative service root
- `build`
  - omitted only when there is no build step
- `start`
  - required for long-running services
- `release`
  - optional one-shot command for migrations or release prep
- `health`
  - optional, but preferred for `web` services
- `output`
  - required for `static`
  - omitted for `web`, `worker`, and `cron`
- `port`
  - required for `web`, omitted for pure worker or cron services
- `domains`
  - empty for non-public services
- `env`
  - only non-secret values
- `secret_refs`
  - names only
- `volumes`
  - explicit storage claims, not local-dev bind mounts
- `warnings`
  - service-local warnings only

### Static output contract

Static services need an explicit deployable artifact claim so provider adapters
do not have to guess what a build step produced.

First shape:

- `kind`
  - `directory`
- `path`
  - repo-relative output directory
- `fallback`
  - optional SPA fallback file such as `index.html` or `200.html`

Recommended first shape:

```json
{
  "kind": "directory",
  "path": "build",
  "fallback": "200.html"
}
```

## Backing-service contract

Backing services should not pretend to be provider-native resources yet. The
neutral model should describe the dependency honestly.

Each backing service should carry:

- `name`
- `kind`
  - `postgres`
  - `mariadb`
  - `redis`
  - `memcached`
  - `object_storage`
- `mode`
  - `managed`
  - `self_hosted`
  - `unknown`
- `required`
  - whether the app can run without it
- `consumers`
  - application services that depend on it
- `warnings`
  - unresolved production-policy questions

Recommended first shape:

```json
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
```

## Domain contract

Domains should stay explicit rather than being re-derived inside adapters.

Each domain entry should carry:

- `host`
- `service`
- `tls`
  - `required`
  - `provider_managed`
  - `operator_managed`
  - `unknown`

Recommended first shape:

```json
{
  "host": "api.songsprout.test",
  "service": "api",
  "tls": "provider_managed"
}
```

## Secrets contract

Secrets should be references, not values.

Each secret should carry:

- `name`
- `services`
  - consumers
- `required`
- `source`
  - `operator`
  - `derived`
  - `unknown`
- `notes`
  - short explanation when Effigy cannot infer enough

Recommended first shape:

```json
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
```

## Warnings contract

Warnings are part of the product, not incidental output.

The export layer should surface warnings for cases like:

- missing required secrets
- no clear release or migration hook
- ambiguous worker scaling
- storage policy not defined
- backing service present locally but no clear managed equivalent chosen
- provider cannot represent the requested shape directly

Warnings should be structured, not only prose.

At minimum they should carry:

- `code`
- `scope`
  - app
  - service
  - backing_service
  - domain
- `message`
- `severity`
  - `info`
  - `warn`
  - `error`

Recommended first shape:

```json
{
  "code": "missing-secret",
  "scope": "service",
  "target": "api",
  "message": "SESSION_SECRET must be provided by the operator",
  "severity": "warn"
}
```

The extra `target` field should identify the affected service, backing
service, or domain when the scope is not app-wide.

## Exportability rule

The model should be valid even when export is incomplete.

That means:

- warnings can coexist with a valid model
- provider export may still produce files while also surfacing `warn` entries
- `error` warnings should block `deploy export` unless the operator explicitly
  chooses a future override surface

`deploy model --json` should always emit the model when derivation succeeds,
even if export would later stop on `error` warnings.

## Derivation rule

The model should be derived from effective manifest and bundle state after
normal composition.

That means the model is built from:

- root manifest
- imported fragments
- bundle defaults
- local overlays when explicitly included

It should not inspect local runtime state or active dev containers.

## Export boundary

Provider adapters should consume this model and produce:

- generated files
- an export report
- any provider-specific warnings that remain after translation

Adapters should not:

- re-derive bundle intent from scratch
- silently guess secrets
- silently invent storage or scale policy

## First non-goals

This contract does not yet promise:

- live cloud provisioning
- secret sync into provider accounts
- deployment execution
- production state reconciliation

Those are future concerns if the export surface proves trustworthy first.

## Next Task

Use this contract to define the first concrete model shape for Underlay:

- web services
- jobs worker
- database and cache references
- public domains
- warning output for missing provider policy
