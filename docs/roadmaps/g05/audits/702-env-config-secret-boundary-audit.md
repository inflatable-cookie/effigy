# 702 Env Config Secret Boundary Audit

Date: 2026-05-12
Roadmap: [`g05.002`](../002-secret-manifest-and-doctor-surface.md)
Card: [`702`](../batch-cards/702-audit-env-config-secret-boundaries.md)
Contract: [`032`](../../../contracts/032-secret-and-local-config-management-contract.md)

## Summary

Effigy already has partial secret handling through `.env.schema` sensitive
entries, but that path is not enough for `g05`.

The new `[secrets]` surface should be added as a manifest-level declaration
model, backed by a shared secret-domain resolver. `.env.schema` should remain
as compatibility for legacy task env validation, but true secrets should move
to declared secret keys and runtime injection.

## Current Effigy Surfaces

### Manifest

Evidence:

- `crates/effigy-manifest/src/lib.rs` defines `TaskManifest` with `env`,
  raw `deploy`, and `env_schema`, but no `secrets` section.
- `crates/effigy-manifest/src/task_runtime.rs` defines task `env` and
  `env_file`.

Boundary:

- Add `[secrets]` to `TaskManifest` as a typed manifest section.
- Keep `deploy` raw for now; provider packages can consume declared secret
  metadata through deploy context later.
- Do not overload `[env_schema]` with vault/backend settings.

### `.env.schema`

Evidence:

- `crates/effigy-env/src/secret.rs` already has `SecretString`,
  redacted `Display`/`Debug`, and explicit `expose()`.
- `crates/effigy-env/src/resolver.rs` can split `plain_env()` from
  `secret_env()`.
- `crates/effigy-env/src/schema_support.rs` reads `.env` overrides and
  resolves `.env.schema`.

Boundary:

- Preserve `.env.schema` behavior.
- Treat sensitive `.env.schema` entries as compatibility secrets, not the new
  durable declaration source.
- `g05.002` can reuse `SecretString`, but should not make vault declarations
  depend on `.env.schema` syntax.

### Task Execution

Evidence:

- `src/runner/execute/pipeline/standard.rs` resolves `.env.schema`, builds
  `secret_env()`, and passes it into host/container paths.
- `src/runner/execute/process.rs` injects secrets with `Command::env()`.
- `src/runner/exec_command/transport.rs` injects container task secrets as
  compose exec `-e KEY=value`.
- `src/runner/execute/sequence_run.rs` passes sensitive schema values to shell
  sequence steps, including parallel steps.

Boundary:

- Add a shared secret-resolution step before task execution.
- Merge declared secrets with existing `.env.schema` sensitive compatibility
  values during the transition.
- Keep shell-command string rendering free of secret values.
- Revisit container `-e KEY=value` output/report redaction when declared
  secrets are added.

### Managed Run Env Resolution

Evidence:

- `crates/effigy-managed/src/run_spec/sequence/env_resolution/sources.rs`
  resolves env entries from manifest env, process env, `.env.schema`, and
  `.env` files.
- That path caches only `plain_env()` for `.env.schema`.

Boundary:

- Keep managed run env references non-secret by default.
- Add explicit secret references later instead of making profile names silently
  pull sensitive values.
- Do not let `env = "NAME"` profile syntax become a hidden secret lookup.

### Rhai

Evidence:

- `crates/effigy-rhai/src/host_api.rs` registers global `env(name)` as a raw
  process environment lookup.
- `crates/effigy-rhai/src/lib.rs` exposes runtime paths and context through
  constants and the runtime module.
- Process helpers accept arbitrary `env` option maps.

Boundary:

- Keep `env(name)` for compatibility, but do not make it the preferred secret
  API.
- Add a declaration-bound secret API in `g05.004`, for example
  `effigy.secret(name)` and `effigy.has_secret(name)`.
- Provider packages should move from `env("RENDER_API_KEY")` to the declared
  secret API once deploy injection lands.

### Deploy Provider Packages

Evidence:

- `src/runner/deploy_command/provider_context.rs` builds provider context
  without secret values.
- `src/runner/deploy_command/provider_package.rs` runs provider Rhai scripts
  with context/report paths via scoped env.
- `external/providers/render/scripts/preflight.rhai`,
  `status.rhai`, and `apply.rhai` read `RENDER_API_KEY` from `env()`.
- `docs/contracts/025-deploy-provider-package-contract.md` says provider
  scripts must not print or write secret values.

Boundary:

- `g05.002` should support declaring deploy-targeted secrets.
- `g05.004` should pass deploy secrets into provider package execution through
  the Rhai secret API, not raw process env.
- Existing Render scripts can keep `RENDER_API_KEY` as compatibility until
  provider packages are migrated.

### Container Runtime

Evidence:

- Task-to-container execution can inject sensitive `.env.schema` values through
  compose exec.
- Local container startup currently relies on generated compose/env material
  and bundle scripts, not a secret-aware container plan.

Boundary:

- `g05.005` needs service/container-level secret requirements and injection.
- Startup-time container secrets are different from task exec secrets.
- Persistent repo-root `.env` generation must become compatibility only.

## Underlay And Example App Classification

### Underlay Bundle

Evidence:

- `external/bundles/underlay/scripts/bootstrap-env.rhai` generates API,
  admin, and front `.env` files.
- It writes ordinary config such as hosts, ports, CORS, email adapter, S3
  endpoint URLs, and public URLs.
- It also writes true local secrets such as JWT private key,
  `AUTH_OAUTH_SECRET_KEY`, `ENCRYPTION_KEY`, and S3 access keys.

Boundary:

- Underlay bundle bootstrap should eventually split generated local config from
  generated local secrets.
- Non-secret values should move to app config or generated runtime config.
- Local generated credentials should either be stored in the Effigy vault or
  generated into an explicit compatibility file under `.effigy/runtime/`.

### Example App

Evidence:

- Example App has `.env` files in `cream`, `dairy`, and `farmyard`.
- `cream` and `dairy` `.env` files only contain public URL/version config.
- `farmyard/.env` mixes ordinary config with true credentials and local
  generated secrets.
- `farmyard/config/default.toml` and `farmyard/config/local.toml` already carry
  ordinary app config such as email, auth policy, AI defaults, operations, and
  migration settings.

Classification:

- Ordinary config: host, port, environment, CORS, cookie settings, SMTP host
  and port, public URLs, bucket names, S3 endpoint URLs, path-style flags,
  WebAuthn RP display config, token lifetimes, support/default email, AI
  runtime toggles, local base URLs.
- Generated runtime config: local database URLs, local test database URLs,
  local MinIO credentials if they remain fixed dev credentials, generated
  public service URLs.
- True secrets: API provider keys, OAuth client secrets, JWT private keys,
  OAuth/session secret keys, encryption keys, production S3 credentials,
  provider deploy API keys, Vimeo access token, SMTP password.
- Legacy compatibility: existing app `.env` files until the app config loader
  and Effigy injection path are updated.

## Required Parser Work For `g05.002`

Implement these first:

- `ManifestSecretsConfig`
- `ManifestSecretsBackend`
- `ManifestSecretsVaultConfig`
- `ManifestSecretKeyConfig`
- target enum for `tasks`, `containers`, `rhai`, `deploy`, `state`,
  `artifacts`
- manifest tests for valid/invalid sections
- CLI command parsing for `effigy secrets list` and `effigy secrets doctor`

Do not implement these in `g05.002`:

- vault encryption
- unlock/session cache
- value storage
- runtime injection
- provider package migration
- container startup injection

## Compatibility Constraints

- Existing `.env.schema` auto-detection must keep working.
- Existing task `env_file` and run-step `env_file` behavior must keep working.
- Existing Rhai `env(name)` must keep working.
- Existing Render provider package env-var compatibility must keep working
  until `g05.004`.
- Existing Underlay bootstrap `.env` generation must not be removed before
  the Example App proof has a replacement path.

## Blockers

None for `g05.002`.

The main risk is confusing users with two secret paths during the transition.
The implementation should present `.env.schema @sensitive` as legacy
compatibility and `[secrets]` as the forward contract.

## Next Implementation Direction

Open `g05.002` with a parser-only strict lane:

- add typed `[secrets]` manifest parsing
- add read-only `effigy secrets list`
- add read-only `effigy secrets doctor`
- emit JSON without value fields
- document `.env.schema` compatibility without changing behavior

