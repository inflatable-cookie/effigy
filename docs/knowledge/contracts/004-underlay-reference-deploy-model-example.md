# Underlay Reference Deployment Model Boundary

The old `underlay-reference` JSON specimen remains in Git history. It is not
an observed result of the current reference repository. The current bundle
still declares the same neutral service roles, but the reference's directories
and task shapes changed. Treat [contract 003](003-underlay-deployment-derivation.md)
and the bundle's `export.toml` as the design source; use `effigy deploy model
--json` as the runtime proof before presenting an output specimen.

## Current Reference Inputs

The sibling `underlay-reference` repository declares:

| Input | Value |
| --- | --- |
| `host` | `acme.test` |
| `project_name` | `underlay-reference-dev` |
| `databases` | `acme` |
| `dirs.front` | `apps/acme-front` |
| `dirs.admin` | `apps/acme-admin` |
| `dirs.api` | `apps/acme-api` |
| `dirs.client` | `packages/acme-client` |
| `dirs.ui` | `packages/acme-ui` |

The current `underlay-effigy-bundle/export.toml` declares these production
roles in its `[deploy.model]` section:

| Service | Role | Source | Public domain |
| --- | --- | --- | --- |
| front | static Node output | front directory, `build` | `acme.test` |
| admin | static Node output | admin directory, `build` | `admin.acme.test` |
| api | Rust web process | API directory, `api` | `api.acme.test` |
| jobs | optional Rust worker | API directory, `jobs` when present | none |

The static services use output directory `build` and request Svelte fallback
detection. The API declares `/v1/health` and an optional `db:migrate` release
task. The bundle declares managed Postgres and `DATABASE_URL` references for
API and jobs. Local dbgate, mailpit, and MinIO services are excluded from the
production model. Domains use provider-managed TLS.

## Current Verification Limit

On 2026-09-26, the current Effigy binary rejected
`effigy --json deploy model --repo <underlay-reference>` with:

```text
required task `build` is missing or non-command in `apps/acme-front/effigy.toml`
```

The front manifest has a `build` task array containing `config:generate` and
a Vite build command. The old specimen assumed a single command and older
`acme-front`/`acme-admin`/`acme-api` root paths. It must not be quoted as live
JSON output. This document records the current model declaration and the
specific proof gap without inventing a successful provider export.

The generic [deployment model contract](002-production-deployment-model.md)
continues to govern `deploy.model.v1`. When the reference can produce a model,
a fresh CLI output can replace the historical specimen.
