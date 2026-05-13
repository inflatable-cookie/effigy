# Secrets And Local Vault Guide

This guide explains how to declare, store, and use secrets in Effigy-managed
repos without committing credentials or leaking them through logs and reports.

## What It Is

Effigy's `[secrets]` config declares what secrets a repo needs, and the
built-in `effigy-vault` backend stores their values in an encrypted local vault
file. At runtime, Effigy injects declared secrets into the right consumers
(tasks, containers, Rhai scripts, deploy hooks) without writing plaintext to
repo files or exposing values in command output.

## When To Use It

Use `[secrets]` for:

- database connection URLs with embedded credentials
- API tokens for deployment providers
- signing keys and private tokens
- any value that grants access and should not be committed

Do not use `[secrets]` for:

- local ports, hostnames, or service aliases
- feature flags or environment names
- generated internal URLs without credentials
- container image names or bucket names that are not confidential

Move those to normal config, bundle defaults, or `.env.schema` non-secret
entries.

## Declare Secrets

Add a `[secrets]` section to `effigy.toml`:

```toml
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "ssh-agent"
unlock = "key-and-passphrase"

[secrets.keys.database_url]
required = true
targets = ["tasks", "containers"]
description = "Application database connection URL"

[secrets.keys.render_api_key]
required = false
targets = ["deploy"]
description = "Render API key for deployment checks and apply"
```

### Declaration fields

| Field | Required | Description |
|---|---|---|
| `backend` | yes | secret backend name (`effigy-vault` is the built-in) |
| `[secrets.vault]` | no | backend-specific config |
| `path` | no | vault file location (default: `.effigy/secrets/local.vault`) |
| `unlock` | no | unlock policy: `passphrase`, `key-and-passphrase`, or `external` |
| `[secrets.keys.<name>]` | per-key | one block per secret |
| `required` | no | whether the value must exist before operations that need it |
| `targets` | yes | which consumers receive this secret at runtime |
| `description` | no | human-readable purpose |

### Consumer targets

| Target | Injected into |
|---|---|
| `tasks` | task process environments |
| `containers` | container service environments at startup |
| `rhai` | Rhai scripts via `effigy::secret(name)` |
| `deploy` | deploy provider package Rhai scripts |
| `state` | state apply hook task environments |
| `artifacts` | artifact workflow Rhai scripts |

A secret can target multiple consumers: `targets = ["tasks", "containers", "rhai"]`.

## Manage The Vault

### Initialize

```sh
effigy secrets init
```

Creates the vault file if it does not exist and prints guidance.

### List declared secrets

```sh
effigy secrets list
effigy secrets list --json
```

Reports names, targets, required flags, and vault state. Never prints values.

### Store a value

```sh
effigy secrets set database_url
effigy secrets set render_api_key
```

Prompts for the value and stores it in the vault. For CI or non-TTY use, values
must be supplied through stdin or environment variables depending on the Effigy
version; check `effigy secrets set --help`.

### Remove a value

```sh
effigy secrets unset render_api_key
```

Removes the value from the vault. The declaration stays in the manifest until
you edit it.

### Unlock for the current session

```sh
effigy secrets unlock
```

Verifies the vault can be decrypted. Unlock is per-invocation; Effigy does not
keep a daemon.

### Lock

```sh
effigy secrets lock
```

Clears invocation-local decrypted state.

### Diagnostics

```sh
effigy secrets doctor
```

Checks vault health, missing required values, and declaration consistency.

### Export (compatibility bridge)

```sh
effigy secrets export --format env --output .effigy/runtime/secrets/local.env --yes
```

Writes plaintext values to a file. This is an explicit compatibility escape
hatch, not the default runtime path. Requires `--yes`.

## Use In Tasks

Secrets with `targets = ["tasks"]` are injected into task processes
automatically:

```toml
[tasks.migrate]
run = "dbmate migrate"
```

The task receives `DATABASE_URL` in its environment if `database_url` is
declared with `targets = ["tasks"]` and stored in the vault. Missing required
values block task execution before spawn.

## Use In Containers

Secrets with `targets = ["containers"]` are resolved before `effigy container
up` and passed through the compose process environment. No repo-root `.env`
file is written.

## Use In Rhai Scripts

Scripts that declare `targets = ["rhai"]` can read secrets safely:

```rhai
if effigy::has_secret("api_token") {
    let token = effigy::secret("api_token");
    let result = http::request("GET", "https://api.example.com/v1/status", #{
        headers: #{ "Authorization": `Bearer ${token}` }
    });
}
```

Rules:

- `effigy::secret(name)` rejects undeclared or wrong-target reads at runtime
- Known values are redacted from Rhai errors and host output maps
- Never build shell commands that embed secrets; use structured helpers

## Use In Deploy Hooks

Deploy provider packages and deploy/state hooks receive `targets = ["deploy"]`
and `targets = ["state"]` secrets through their process environment or Rhai
context automatically.

## Safety

- Values are never printed in JSON or text output
- Values are redacted in logs, errors, and provider reports
- Injection uses `Command::env()` instead of shell command strings to avoid `ps`
  exposure
- The vault requires a human-gated unlock factor (passphrase or key + passphrase)
- `key-only` unlock is not supported without explicit justification

## JSON Output

`effigy --json secrets list` returns `effigy.secrets.v1` with metadata only — no
values, no hashes, no decrypted contents. See
[`017-json-output-contracts.md`](./017-json-output-contracts.md).

## Relationship To `.env.schema`

`.env.schema` remains useful for validation and legacy task environments, but
`[secrets]` is the preferred surface for true secrets in new projects.

Migration path:

1. Move secret declarations from `.env.schema` to `[secrets.keys]`
2. Store values with `effigy secrets set`
3. Update `targets` to match the actual consumers
4. Remove the old `@sensitive` entries from `.env.schema` once tasks no longer
   depend on them

## Related

- [`050-env-schema-integration.md`](./050-env-schema-integration.md) — env schema
  for non-secret configuration
- [`061-rhai-script-steps-guide.md`](./061-rhai-script-steps-guide.md) — secret
  access from Rhai scripts
- [`074-deployment-guide.md`](./074-deployment-guide.md) — deploy secret targets
  and hooks
- [`../contracts/032-secret-and-local-config-management-contract.md`](../contracts/032-secret-and-local-config-management-contract.md) — full contract
