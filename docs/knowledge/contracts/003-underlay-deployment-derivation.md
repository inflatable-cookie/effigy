# 003 - Underlay Bundle-Owned Deployment Model

The git-hosted `underlay-effigy-bundle` owns its rendered `[deploy.model]`
section in `export.toml`. Effigy consumes that section as `deploy.model.v1`;
provider packages consume the neutral model. This document explains the
product-specific derivation and its omissions. The generic schema lives in
[contract 002](002-production-deployment-model.md).

## Purpose

The neutral deployment model is only useful if a real bundle can own and emit
that shape consistently.

Underlay is the first target because its shape is already regular:

- `front`
- `admin`
- `api`
- `jobs`
- standard bundled backing services for local dev

## Source of truth

The source of truth is the effective manifest after normal composition:

- root manifest
- included fragments
- bundle inputs
- bundle defaults
- explicit local overlays when included

The `underlay-effigy-bundle` owns the rendered `[deploy.model]` section inside that
effective manifest.

Effigy consumes that rendered model. It does not reconstruct the Underlay
deploy shape from Rust-side bundle-name branches or inspect live containers
and runtime state to invent production intent.

## Bundle inputs that matter

The Underlay derivation reads:

- `[bundle].host`
- `[bundle].project_name`
- `[bundle].workspace_subdir`
- `[bundle].databases`
- `[bundle].api_port`
- `[bundle.dirs]`
  - `api`
  - `front`
  - `admin`
- `[bundle.routes]`
  - `front`
  - `admin`
  - `api`

The production model does not derive from local-only knobs such as:

- `[bundle].admin_port` and `[bundle].front_port`
- `[bundle.dirs].docs`, `client`, and `ui`

- `sources.underlay`
- `sources.poodle`
- `system_name`
- `container_name`
- `workspace_service_name`
- `default_workspace`

Those matter for local orchestration, not production shape.

## First derived application services

The bundle declares three core application surfaces and one optional one:

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

For Underlay, static fallback ownership derives from the package-local
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

The API service also receives the database-related secret references that
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

The model declares only public app domains:

- front domain from `routes.front` plus `host`
- admin domain from `routes.admin` plus `host`
- api domain from `routes.api` plus `host`

The local bundle also registers:

- `dbgate.<host>`
- `mailpit.<host>`
- `minio.<host>`

Those do not appear in the first production deployment model by default.

Reason:

- they are local-dev operator helpers
- they are not part of the primary application deployment surface

## Backing-service derivation

The current bundle declares:

- one required `postgres` backing service. `[bundle].databases` supplies the
  primary database name in its secret notes; the backing-service declaration
  itself is unconditional.

It does not derive by default:

- `dbgate`
- `mailpit`
- `minio`

Reason:

- `postgres` is a real application dependency
- `dbgate` and `mailpit` are local-only helpers
- `minio` is bundled locally for convenience, but the bundle alone does not
  prove that production object storage is truly required

A production storage service requires an explicit bundle declaration; local
MinIO use is not such a declaration.

## Secret derivation

The bundle declares a minimal secret set conservatively.

With the declared `postgres` service:

- add `DATABASE_URL` for `api`
- add `DATABASE_URL` for `jobs` when the jobs service exists

Do not invent broader secret sets from local dev env guesses yet.

If a service clearly cannot start honestly without additional operator secrets,
emit warnings rather than fake defaults.

## Warning rules

Underlay derivation emits warnings when:

- a required package task is missing
  - for example, no `<front-dir>/build`
- the API package exposes no clear `db:migrate` release/migration hook
- a static service has no clear fallback file for provider rewrite generation
- the repo shape suggests extra production concerns Effigy cannot model yet

Do not emit warnings merely because local-only helpers were intentionally
excluded.

## First omission rule

The Underlay deployment model is intentionally narrow.

It does not need to infer:

- cron schedules
- object storage
- mail delivery infrastructure
- search infrastructure
- static hosting provider specifics

Those should widen only when Effigy gains enough manifest truth to derive them
honestly.

## Current Reference Limit

The sibling `underlay-reference` manifest currently uses `apps/acme-front`,
`apps/acme-admin`, and `apps/acme-api`. Its front `build` task is a task array.
As of this review, `effigy deploy model --repo <underlay-reference> --json`
rejects that task as “missing or non-command.” The bundle declaration is
current design truth, but the old static JSON specimen is not a verified
output of the current reference repository. See the
[reference boundary](004-underlay-reference-deploy-model-example.md).
