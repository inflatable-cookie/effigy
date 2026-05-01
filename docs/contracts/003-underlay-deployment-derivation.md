# 003 - Underlay Deployment Derivation

This contract defines how the shipped `underlay` bundle should derive into
`deploy.model.v1`.

It is the first concrete derivation target for the production export surface.

## Purpose

The neutral deployment model is only useful if Effigy can map a real bundle
into it consistently.

Underlay is the first target because its shape is already regular:

- `front`
- `admin`
- `api`
- `jobs`
- standard bundled backing services for local dev

## Source of truth

Derivation should come from the effective manifest after normal composition:

- root manifest
- included fragments
- bundle inputs
- bundle defaults
- explicit local overlays when included

The derivation should not inspect live containers or runtime state.

## Bundle inputs that matter

The first Underlay derivation pass should care about:

- `[bundle].host`
- `[bundle].project_name`
- `[bundle].workspace_subdir`
- `[bundle].databases`
- `[bundle].api_port`
- `[bundle].admin_port`
- `[bundle].front_port`
- `[bundle.dirs]`
  - `api`
  - `front`
  - `admin`
  - optional `docs`
  - optional `client`
  - optional `ui`
- `[bundle.routes]`
  - `front`
  - `admin`
  - `api`

The first export lane should ignore local-only knobs like:

- `sources.underlay`
- `sources.poodle`
- `system_name`
- `container_name`
- `workspace_service_name`
- `default_workspace`

Those matter for local orchestration, not production shape.

## First derived application services

The first Underlay export should derive three core application surfaces and one
optional one:

- `front`
- `admin`
- `api`
- `jobs` when the API package exposes a `jobs` task

### Front derivation

`[bundle.dirs].front` derives one application service:

- `name`
  - `front`
- `role`
  - `static`
- `runtime`
  - `node`
- `source_root`
  - `<front-dir>`
- `build.command`
  - from `<front-dir>/build`
- `output`
  - `{ kind = "directory", path = "build", fallback = "<from svelte config>" }`
- `start`
  - omitted in the first shape
- `domains`
  - route derived from `routes.front` plus `host`

Reason:

- the shipped Underlay front packages are Vite/Svelte build outputs
- they have `build`
- they do not declare a real production `start` task

So the honest first export shape is static output, not a fake long-running web
process.

### Admin derivation

`[bundle.dirs].admin` derives one application service:

- `name`
  - `admin`
- `role`
  - `static`
- `runtime`
  - `node`
- `source_root`
  - `<admin-dir>`
- `build.command`
  - from `<admin-dir>/build`
- `output`
  - `{ kind = "directory", path = "build", fallback = "<from svelte config>" }`
- `start`
  - omitted in the first shape
- `domains`
  - route derived from `routes.admin` plus `host`

This matches the same static-site reasoning as `front`.

For Underlay, static fallback ownership should derive from the package-local
Svelte adapter config:

- read `svelte.config.js` / `svelte.config.ts` when present
- promote the adapter-static `fallback` value into `service.output.fallback`
- warn if the service is still static but no fallback can be derived

### API derivation

`[bundle.dirs].api` derives one application service:

- `name`
  - `api`
- `role`
  - `web`
- `runtime`
  - `rust`
- `source_root`
  - `<api-dir>`
- `build.command`
  - from `<api-dir>/build`
- `start.command`
  - from `<api-dir>/api`
- `release.command`
  - from `<api-dir>/db:migrate` when the task exists
- `health`
  - `{ kind = "http", path = "/v1/health" }`
- `port`
  - from `[bundle].api_port`
- `domains`
  - route derived from `routes.api` plus `host`

The API service should also receive the database-related secret references that
fall out of the backing-service derivation.

Reason:

- shipped Underlay APIs expose the shared `/v1/health` route shape
- the common `db:migrate` task is the first honest release-hook promotion seam

### Jobs derivation

If `<api-dir>/jobs` exists, derive one additional application service:

- `name`
  - `jobs`
- `role`
  - `worker`
- `runtime`
  - `rust`
- `source_root`
  - `<api-dir>`
- `build.command`
  - same build owner as `api`
- `start.command`
  - from `<api-dir>/jobs`
- `domains`
  - none

If the `jobs` task does not exist, omit the service instead of inventing it.

## Domain derivation

The first Underlay export should derive only public app domains:

- front domain from `routes.front` plus `host`
- admin domain from `routes.admin` plus `host`
- api domain from `routes.api` plus `host`

The local bundle also registers:

- `dbgate.<host>`
- `mailpit.<host>`
- `minio.<host>`

Those should not appear in the first production deployment model by default.

Reason:

- they are local-dev operator helpers
- they are not part of the primary application deployment surface

## Backing-service derivation

The first Underlay export should derive:

- one required `postgres` backing service when `[bundle].databases` is set

It should not derive by default:

- `dbgate`
- `mailpit`
- `minio`

Reason:

- `postgres` is a real application dependency
- `dbgate` and `mailpit` are local-only helpers
- `minio` is bundled locally for convenience, but the bundle alone does not
  prove that production object storage is truly required

Future work can widen backing-service derivation if the manifest grows explicit
production storage declarations.

## Secret derivation

The first Underlay export should derive a minimal secret set conservatively.

When `postgres` is derived:

- add `DATABASE_URL` for `api`
- add `DATABASE_URL` for `jobs` when the jobs service exists

Do not invent broader secret sets from local dev env guesses yet.

If a service clearly cannot start honestly without additional operator secrets,
emit warnings rather than fake defaults.

## Warning rules

Underlay derivation should emit warnings when:

- a required package task is missing
  - for example, no `<front-dir>/build`
- the API package exposes no clear `db:migrate` release/migration hook
- a static service has no clear fallback file for provider rewrite generation
- the repo shape suggests extra production concerns Effigy cannot model yet

Do not emit warnings merely because local-only helpers were intentionally
excluded.

## First omission rule

The first Underlay deployment export is allowed to be intentionally narrow.

It does not need to infer:

- cron schedules
- object storage
- mail delivery infrastructure
- search infrastructure
- static hosting provider specifics

Those should widen only when Effigy gains enough manifest truth to derive them
honestly.

## Next Task

Use this derivation contract to draft the first implementation-facing JSON
example for a real Underlay repo, then turn that example into:

- `deploy model --json` expectations
- Render export expectations
- Railway export expectations
