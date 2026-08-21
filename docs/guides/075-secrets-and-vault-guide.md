# Secrets And Local Vault Guide

This guide explains how to declare, store, and use secrets in Effigy-managed
repos without committing credentials or leaking them through logs and reports.

## What It Is

Effigy's `[secrets]` config declares what secrets a repo needs, and the
built-in `effigy-vault` backend stores their values in an encrypted local vault
file. At runtime, Effigy injects declared secrets into the right consumers
(tasks, containers, Rhai scripts, deploy hooks) without writing plaintext to
repo files or exposing values in command output.

`effigy-vault` is the local/dev backend. It is meant for operator-controlled
development flows, not as a production hosted secret store. The optional
`[secrets.vault].generate` hook belongs to that same boundary: local vault
bootstrap for dev flows, not a general remote secret provisioning system.

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
generate = { rhai = "scripts/generate-dev-secrets.rhai", run_in = "host" }

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
| `generate` | no | local task hook Effigy may run during `secrets = "required"` task startup when the vault is missing or incomplete |
| `[secrets.keys.<name>]` | per-key | one block per secret |
| `required` | no | whether the value must exist before operations that need it |
| `targets` | yes | which consumers receive this secret at runtime |
| `description` | no | human-readable purpose |

### Consumer targets

| Target | Injected into |
|---|---|
| `tasks` | task process environments |
| `containers` | container runtime delivery (`compose` env by default, or runtime files when the container config opts into that mode) |
| `rhai` | Rhai scripts via `secrets::get(name)` |
| `deploy` | deploy provider package Rhai scripts |
| `state` | state apply hook task environments |
| `artifacts` | artifact workflow Rhai scripts |

A secret can target multiple consumers: `targets = ["tasks", "containers", "rhai"]`.

## Manage The Vault

### Initialize

```sh
effigy secrets init
```

Creates the vault file if it does not exist and prints guidance. If
`[secrets.vault].generate` is configured, `effigy secrets init` creates the
local vault and then runs that configured inline hook instead of stopping at
an empty vault. Initialization also creates an ignored, mode-`0600` local-dev
key beside the vault.

### Local dev unlock

`effigy dev` uses a separately encrypted local-dev payload and does not prompt
for the vault passphrase after setup. This applies to task, container, and Rhai
secret injection reached from the resolved `dev` task, including catalog
selectors such as `api/dev`.

Direct vault commands still follow the configured passphrase policy. In
particular, `secrets get`, `set`, `unset`, `doctor`, `change-passphrase`, and
`export` do not use the local-dev key to unlock secret values.

Existing vaults prompt once on their next `effigy dev` run, then add the
local-dev payload and key in place. Delete the adjacent
`local.vault.local-dev-key` file to revoke unattended dev unlock; the next dev
run requires the passphrase and creates a new key.

This is a command boundary, not isolation from code allowed to run as the app.
An agent that can change or inspect the running application may also influence
how injected secrets are consumed.

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

### Read one value

```sh
effigy secrets get render_api_key
effigy secrets get render_api_key --json
```

Prints one declared stored secret after unlocking the vault. This intentionally
reveals the value; use it only for explicit operator handoff or debugging.

### Remove a value

```sh
effigy secrets unset render_api_key
```

Removes the value from the vault. The declaration stays in the manifest until
you edit it.

### Import from a dotenv file

```sh
effigy secrets import
effigy secrets import infra/local.env
effigy secrets import --json
```

Imports declared keys from a `.env`-style file into the vault:

- defaults to `./.env` when no path is given
- lowercases env var names to match manifest keys
- skips keys not declared in `[secrets.keys]`
- never prints secret values in stdout or JSON

### Change the vault passphrase

```sh
effigy secrets change-passphrase
```

Prompts for the current passphrase, then prompts for and confirms the new
passphrase. Stored secret values are preserved and re-encrypted without being
printed.

### Diagnostics

```sh
effigy secrets doctor
```

Prompts for the vault passphrase when run interactively, then checks vault
health, missing required values, and declaration consistency. Non-interactive
runs do not prompt and report the vault as locked when no passphrase is
available.

### Export (compatibility bridge)

```sh
effigy secrets export --format env --output .effigy/runtime/secrets/local.env --yes
```

Writes plaintext values to a file. This is an explicit compatibility escape
hatch, not the default runtime path. Requires `--yes`.

## Use In Tasks

Secrets with `targets = ["tasks"]` are injected into task processes when the
selected shell task actually references the corresponding env names:

```toml
[tasks.migrate]
run = "dbmate migrate"
```

The task receives `DATABASE_URL` in its environment if `database_url` is
declared with `targets = ["tasks"]`, stored in the vault, and the task
references `$DATABASE_URL` or `${DATABASE_URL}`. Missing required values block
task execution before spawn only for referenced task-target secrets.

Managed tasks can opt into the broader task-secret path explicitly:

```toml
[tasks.dev]
mode = "tui"
secrets = "required"
```

That forces declared `targets = ["tasks"]` values into the managed child
process environment before launch, which is useful when the child commands
expect runtime auth/env keys without spelling them out in the shell command.
The same startup eagerly unlocks container-targeted values, but it does not
promote optional keys: a container key with `required = false` may be absent
without blocking the managed task.

If `[secrets.vault].generate` is configured, Effigy may run `effigy secrets init`
during `secrets = "required"` startup when required task-target secrets are
missing. `secrets init` then delegates to the configured generator hook. The
common local-dev shape is a direct Rhai script that fills repo-local defaults:

```toml
[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"
generate = { rhai = "scripts/generate-dev-secrets.rhai", run_in = "host" }
```

This hook is only consumed from task startup. Effigy does not treat it as a
general deploy/state/artifact generation hook.

## Use In Containers

Secrets with `targets = ["containers"]` are resolved before `effigy container
up`. No repo-root `.env` file is written.

Stored optional values are injected when the vault is unlocked. Missing
optional values are skipped; only keys declared with `required = true` gate
container startup.

By default, Effigy passes those values through the compose process environment.

Bundles and repos that need a tighter runtime contract can opt a container into
runtime-file delivery:

```toml
[containers.web.secrets]
delivery = "runtime-files"
runtime_dir = "/run/effigy/secrets"
source_for_deferrals = true
```

In that mode, Effigy writes:

- `runtime.env`
- `runtime.json`

under the configured `runtime_dir` inside the running primary service. Core
Effigy treats those as generic runtime files. The bundle/catalog can then wire
them into the app runtime however it needs. A legacy PHP bundle, for example,
can mount `runtime_dir` as tmpfs, auto-prepend a PHP bootstrap that reads
`runtime.json`, and let container deferral source `runtime.env` only for the
deferred process.

Repo `.env` files can still carry non-secret local overrides. Runtime-file
delivery is for the sensitive values declared under `[secrets]`.

## Use In Rhai Scripts

Scripts that declare `targets = ["rhai"]` can read secrets safely:

```rhai
if secrets::has("api_token") {
    let token = secrets::get("api_token");
    let result = http::request("GET", "https://api.example.com/v1/status", #{
        headers: #{ "Authorization": `Bearer ${token}` }
    });
}
```

Repo-owned Rhai tasks may also store generated local values:

```rhai
let keys = random::jwt_env_keys();
secrets::set_many(#{
  auth_jwt_private_key: keys["private_key"],
  auth_jwt_public_key: keys["public_key"],
});
```

Rules:

- `secrets::get(name)` rejects undeclared or wrong-target reads at runtime
- `secrets::has(name)` checks whether a declared Rhai secret is available
- `secrets::set(name, value)` and `secrets::set_many(map)` require each secret
  to be declared for the `rhai` target and write to the encrypted vault
- `secrets::set_many(map)` batches validation, unlock, encryption, and write
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
- Direct vault access requires a human-gated unlock factor
- `effigy dev` uses an ignored, mode-`0600` local-dev key and never writes
  plaintext secret values to that key file
- Local-dev unlock is a runtime capability, not a security boundary against
  code allowed to run as the application

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
