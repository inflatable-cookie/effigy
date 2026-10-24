# 002 - Production Deployment Model

This contract defines the first neutral production deployment model for
Effigy.

It is the model that provider packages should consume.

It is not a local-dev compose export, and it is not provider-specific config.

## Purpose

Effigy needs one inspectable model between:

- effective manifest, including any bundle-owned `[deploy.model]` content
- provider-specific deployment output owned by provider packages

That middle layer should let Effigy:

- explain what it thinks the production shape is
- export provider files without duplicating bundle heuristics in each package
- warn clearly when human production policy is still required

## First Shape

The first version should be strong for common managed-platform app shapes:

- frontend plus admin plus API apps
- jobs workers
- standard backing services
- managed-platform deployment files

Product-specific bundle repos can provide their own `[deploy.model]` defaults,
but core Effigy should keep this model provider-neutral and product-neutral.

## Command surface

The neutral model should be inspectable directly, and it describes one named
environment rather than an implicit development profile:

```bash
effigy deploy model <ENV> --json
effigy deploy model <ENV> --env <ENV>   # accepted naming form
```

Provider export builds on that model:

```bash
effigy deploy export <PROVIDER> <ENV> --path <DIR> [--plan]
```

`<PROVIDER>` is a provider id configured under `[deploy.providers]`.
`<ENV>` names a declared `[deploy.<env>]` environment.

`model` and `export` refuse an environment the manifest does not declare, and
the refusal names the declared environments. Without `<ENV>`, a project with
exactly one declared environment uses it and records a notice; the default is
available for one release. A project with two or more declared environments
requires `<ENV>`. A project that declares no `[deploy.<env>]` environment
reports the shared `[deploy.model]` with an unscoped notice, exactly as before
this contract change.

The model output uses schema id:

- `deploy.model.v1`

The environment-aware fields are additive, so the schema id stays `v1`.

## Environment resolution

The effective deployment model is the shared `[deploy.model]` with the selected
environment's `[deploy.<env>.model]` overrides applied per key.

```text
effective = [deploy.model] <- [deploy.<env>.model]
```

- Overrides replace per key. Lists (`domains`, `consumers`, `secret_refs`,
  `volumes`, `warnings`) replace wholesale and never concatenate.
- Map values (`env`) merge per key: an environment value wins, and
  `env_unset = ["KEY"]` removes a shared development value.
- Task steps (`build`, `start`, `release`) and `health`/`output` merge per field,
  so an environment can substitute only `task` or `path` and keep shared flags.
- Services, backing services and secrets are identified by `name`. An override
  may only reference a name the shared model declares; anything else is a
  refusal, because the bundle owns the shared service shape.
- `app` fields override individually.

The model records where every key came from:

```json
{
  "environment": "production",
  "environment_state": "production",
  "provenance": {
    "app.project_name": "environment",
    "services.front.domains": "environment",
    "services.api.build": "shared",
    "services.api.release": "missing"
  },
  "missing": ["services.api.release"]
}
```

`provenance` values are `environment`, `shared` or `missing`. `missing` lists
the key paths whose shared value was development-only and therefore withheld.

## No development inference

A non-development environment never inherits a value that only exists in the
development profile:

- a shared domain ending in a reserved local suffix (`.test`, `.localhost`,
  `.local`) is withheld from the environment, reported under `missing`, and
  warned about with `environment-missing-field` naming the key;
- a shared task command that is development-only (a `dev`/`development`/
  `debug`/`local` token, `localhost`, `127.0.0.1`, `0.0.0.0` or a `.test` host)
  is withheld from `build`, `start` or `release`, reported under `missing`, and
  warned about with `environment-missing-field` naming the key.

An environment named `dev` (or declaring `state = "dev"`/`"local"`) is a
development environment and keeps the shared values. Values the environment
itself declares are always honoured.

`state = "production"` goes further: the model records a `reserved-local-domain`
warning at `error` severity when a resolved domain uses a reserved local suffix,
and `deploy export` refuses rather than export a local-only host.

## Closed vocabularies

Service `runtime` and backing-service `mode` are closed vocabularies. Model
derivation refuses any other value, naming the allowed set.

Allowed service `runtime` values:

- `static` — a static-site surface served as files, with no application runtime
- `node`, `bun`, `deno`
- `rust`, `python`, `go`, `php`, `ruby`, `java`

Allowed backing-service `mode` values:

- `managed` — the provider platform owns and provisions the resource
- `external` — the operator supplies an endpoint Effigy does not provision

The vocabulary closes the approximations Acowtancy #718 had to make: a static
site declares `runtime = "static"` instead of `runtime = "node"`, and an
external resource declares `mode = "external"` instead of `mode = "managed"`
with prose warnings. Values used before this contract remain accepted for one
release, with a `deploy-vocabulary-migration` warning pointing at the new value:
a `role = "static"` service that still declares a non-static runtime, and a
`managed` backing service whose warnings describe an external resource. The
notices appear only when the model is bound to a declared environment.

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
  "environment": "production",
  "environment_state": "production",
  "app": {
    "name": "example-site",
    "bundle": "acme",
    "project_name": "example-site"
  },
  "services": [],
  "backing_services": [],
  "domains": [],
  "secrets": [],
  "provenance": {},
  "missing": [],
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
  - `static`, `node`, `bun`, `deno`, `rust`, `python`, `go`, `php`, `ruby`
    or `java`
  - any other value is refused; see [Closed vocabularies](#closed-vocabularies)
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
  - `external`
  - any other value is refused; see [Closed vocabularies](#closed-vocabularies)
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

`deploy model <ENV> --json` should always emit the model when derivation
succeeds, even if export would later stop on `error` warnings. A production
export refuses reserved local domains before touching the provider package.

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
- substitute development values for a named environment

The provider context names the selected environment (`env`) and carries its
resolved model in `model`, together with the environment's `deploy.state`,
`code_ref`, `release_policy` and `artifact_policy`. A provider package
therefore sees the same environment-scoped model that `deploy model <ENV>`
reports, never the unqualified development profile.

## First non-goals

This contract does not yet promise:

- live cloud provisioning
- secret sync into provider accounts
- deployment execution
- production state reconciliation

Those are future concerns if the export surface proves trustworthy first.
