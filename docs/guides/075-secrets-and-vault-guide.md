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

Vault mutations are serialized across CLI and Rhai commands. Existing vaults
migrate automatically on their next write: the configured path becomes a
pointer to an encrypted generation, with the prior generation kept as
`<vault>.prev`. Keep the pointer, its `.ciphertext.g<N>` files and local unlock
files together when backing up or moving a vault. A failed write leaves a
complete old or new generation; unreadable vault state is never overwritten.
Unlock, lock and consumer secret declarations keep their existing behavior.

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

The vault uses a passphrase by default. Delete any `identity`, `unlock` or
`external` line under `[secrets.vault]`; these retired keys only restated that
default and now fail with the declaring file and line. Existing vaults and
`effigy secrets unlock` keep their behavior.

### Declaration fields

| Field | Required | Description |
|---|---|---|
| `backend` | yes | secret backend name (`effigy-vault` is the built-in) |
| `[secrets.vault]` | no | backend-specific config |
| `path` | no | vault file location (default: `.effigy/secrets/local.vault`) |
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

To authorize agents and other local commands without repeated passphrase
prompts, run `effigy secrets unlock` once at an interactive terminal. It checks
the passphrase, creates or repairs the local-dev key, and writes a machine-local,
mode-`0600` credential encrypted with that key. The credential supports direct
`secrets get`, `set`, `unset`, `import`, `doctor`, `export`, and
`change-passphrase`, plus secret-backed tasks, containers, and Rhai scripts.
`change-passphrase` updates it to the new passphrase. `secrets init` still asks
for a new passphrase. The unlock lasts until `effigy secrets lock` removes both
local unlock files. This also makes the next `dev` run prompt again.

The local files live beside the vault and are shared with linked worktrees that
use the primary checkout's vault. Anyone who can read both files can use the
vault; keep the vault directory out of version control and restrict filesystem
access.

Without an explicit `secrets unlock`, mutations (`set`, `unset`, `import`,
`change-passphrase`) and `export` still require the passphrase. The local-dev
key alone unlocks resolved `dev` tasks, and it also lets `effigy secrets
doctor` and `effigy secrets get` read the vault on this machine: doctor reports
`vault: unlocked (local-dev key)` with the stored key names, and `get` prints
one declared value. Nothing else prints a value.

A prompt is always visible. Whenever Effigy needs a passphrase and a progress
display is running — a `dev` session's apply, a task that prepares its stack —
the step lines are cleared, the prompt is shown on the terminal, and the steps
resume once it is answered.

Inside a managed pane, whose output is captured, no prompt is ever shown: the
container and task secret readers, the one-time local-dev upgrade, `effigy
secrets` prompts and Rhai secret access each refuse or deliver nothing and name
`effigy secrets unlock` instead, because a question written into a captured pane
is one nobody can see. A value read for `effigy secrets set` is that command's
own input: it never becomes the session's vault passphrase.

Existing vaults prompt once on their next `effigy dev` run, then add the
local-dev payload and key in place. Delete the adjacent
`local.vault.local-dev-key` file to revoke unattended dev unlock; the next dev
run requires the passphrase and creates a new key. Use `effigy secrets lock`
when the broader command unlock is active.

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

Prints one declared stored secret after unlocking the vault with the
passphrase this invocation holds, the sealed `secrets unlock` credential, or
the local-dev key a `dev` session created. This intentionally reveals the
value; use it only for explicit operator handoff or debugging.

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

Reads the vault with the local-dev key when it exists and reports
`unlocked (local-dev key)` with the stored key names; otherwise it prompts for
the vault passphrase when run interactively, then checks vault health, missing
required values, and declaration consistency. Non-interactive runs never
prompt: with no local-dev key and no passphrase available the vault is reported
as `locked`. Doctor never prints a value.

The text report names the stored key names, the required keys the vault is
missing, the stored keys `[secrets.keys]` does not declare, and the factor that
opened the vault (`passphrase` or `local-dev key`; a passphrase this session
already holds reads as `passphrase`). It never prints a value. `--json` carries
the same facts as `vault_state` (`unlock`, `stored_keys`, `missing_required`,
`undeclared_stored`):

```sh
effigy secrets doctor
# [secrets] doctor
# repo: /path/to/repo
# backend: effigy-vault
# keys: 3
# vault: unlocked (local-dev key)
# vault path: /path/to/repo/.effigy/secrets.vault
# stored keys (3): mailchimp_api_key, openai_api_key, recaptcha_secret
# missing required keys: none
# undeclared stored keys: none
```

### Export (compatibility bridge)

```sh
effigy secrets export --format env --output .effigy/runtime/secrets/local.env --yes
```

Writes plaintext values to a file. This is an explicit compatibility escape
hatch, not the default runtime path. Requires `--yes`.

The destination is secured to owner-only permissions (0600) before any secret
byte is written, including when the file already exists, and the held file
descriptor is what gets secured. On Unix, a reader that already has the file
open keeps its access; the export neither revokes open descriptors nor
publishes atomically.

## Use In Tasks

Secrets with `targets = ["tasks"]` are injected into task processes when the
selected shell task actually references the corresponding env names. Those
declarations and the catalog's relative vault belong to the selected task
catalog: `desktop/package:mac:preflight` from the workspace root and
`package:mac:preflight` from `apps/bovine-desktop` use the same child
catalog vault. A root-catalog task keeps the root vault. Parent and sibling
secret sets are not merged.

```toml
[tasks.migrate]
run = "dbmate migrate"
```

The task receives `DATABASE_URL` in its environment if `database_url` is
declared with `targets = ["tasks"]`, stored in the vault, and the task
references `$DATABASE_URL` or `${DATABASE_URL}`. Missing required values block
task execution before spawn only for referenced task-target secrets. The
selected operation's secret scope is resolved before any vault prompt.

Managed tasks can opt into the broader task-secret path explicitly:

```toml
[tasks.dev]
mode = "tui"
secrets = "required"
```

That makes declared `targets = ["tasks"]` values available to managed child
processes before launch, which is useful when the child commands expect
runtime auth/env keys without spelling them out in the shell command. Host
process entries, including entries with `run_in = "host"`, receive those
values through the child environment. The values stay out of the rendered
shell command and argv, and headless logs redact matches.
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
generate = { rhai = "scripts/generate-dev-secrets.rhai", run_in = "host" }
```

This hook is only consumed from task startup. Effigy does not treat it as a
general deploy/state/artifact generation hook. Because the hook is a declared
durable step of that startup, such a task is admitted with its checkout
journal before its body runs, so it appears in `effigy runs` history
even when no generation was needed.

## Use In Containers

Secrets with `targets = ["containers"]` are resolved before `effigy container
up`. Read-only checks (`status`, `logs`, `stats`), exec/shell against an
already running runtime, `down`, and `reset` do not unlock the vault. Reset
only tears down runtime artifacts; the next `container up` is what injects
secrets again. Unrelated required container secrets cannot block those
operations. No repo-root `.env` file is written.

Stored optional values are injected when the vault is unlocked. A `dev`
session resolves them through the local-dev key, or through the passphrase that
session just entered, in every step it starts, including the `container up`
child a managed run brings up. Only keys declared with `required = true` gate
container startup.

A declared container key that could not be delivered — the vault is locked with
no terminal to ask for its passphrase, the key is declared but not stored, the
vault is missing, or the local-dev key cannot read it — is reported once:

```text
[warn] container secret `openai_api_key` was not delivered: the vault is locked and this run has no terminal to ask for its passphrase; run `effigy secrets unlock` at a terminal, or rerun `effigy dev` on one
```

When this machine holds a local-dev key but the command is not a `dev` session,
the warning says exactly that instead, because the key is a `dev` factor and the
vault is not locked for a machine that can read it:

```text
[warn] container secret `openai_api_key` was not delivered: the local-dev key only unlocks `effigy dev`, and this run is not a dev session; for direct commands run `effigy secrets unlock` once at a terminal
```

Only keys declared `required = true` are ever asked for: a resolution whose
container keys are all optional never blocks on a prompt, so an interactive
`effigy workspace` or `effigy dev` cannot wait on a question it did not
promise. What this session already holds — its own passphrase, or the sealed
`effigy secrets unlock` credential — is still delivered.

The same lines appear under `warnings` in `effigy container up --json`. An
empty `runtime.env`/`runtime.json` is never written silently: a bring-up that
delivered nothing leaves the files alone instead of replacing them with an
empty file the service reads as "no secrets configured".

By default, Effigy passes those values through the compose process environment.
On Colima, Compose runs inside the VM and never inherits that environment, so
Effigy writes the forwarded values to a per-invocation interpolation file under
`<effigy state home>/compose-env/` and names it with `--env-file`. Each such
launch runs under a retained kernel Run: the file stays in kernel custody until
the exact Compose group has provably closed, and only then is it removed. A
launch that cannot be admitted as a Run refuses before it starts, naming why.

That directory can keep crash residue: a `compose-<32 hex>.env` file, a
`.rm-<32 hex>` quarantine name from an interrupted removal, or a legacy
`<16 hex>.env` file from an older Effigy. Effigy reports the count once —
`N Compose interpolation file(s) … are left in place` — and never blocks a
launch or deletes one on its own, because a free file lock does not prove the
Compose group behind a file has closed. To clear them by hand, first make sure
no Effigy Compose invocation is running against that state home (no
`effigy container up`, `effigy dev` or task apply is active on this host), then
delete the individual files you recognise:

```sh
ls -la "$HOME/.effigy/compose-env"
rm "$HOME/.effigy/compose-env/compose-<32 hex>.env"
```

List before deleting: a name removed while an invocation is live makes that
invocation fail to read its file. Deleting a file that is not crash residue is
outside what Effigy can help you decide.

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

under the configured `runtime_dir` inside the running primary service.

The files are written on every apply that resolves values, not only on first
creation. A `dev` session that borrows an environment a direct `container up`
started — which cannot resolve that session's optional keys — therefore
rewrites both files with what the session resolves, instead of reusing a stack
that is missing them.

A service that is already running keeps the values it read when it started. The
delivery reads the files back before rewriting them and, when a delivered value
differs from what the service holds (including a service that holds no file at
all), reports the changed keys once in that apply:

```text
[warn] container secret(s) mailchimp_api_key were written to /run/effigy/secrets/runtime.json after `app` started; services that read this file per request see them now; a service that reads it only at startup needs its process restarted to see them
```

That is one apply's report, not a record: nothing is persisted between applies
and nothing retires it. An apply that writes exactly what the service already
holds says nothing, and an apply that changes a key says so again because it
changed it again. The line states facts and prescribes nothing: it does not
recommend a container restart, which may clear a tmpfs `runtime_dir`.

Nothing restarts a live service on its own: the apply is the preparation the
task is about to run in, so stopping it there would break the run. Consumers
that read these files when a process starts — a request-time bootstrap, a
deferred spawn — see the new values on their next start and need no restart.

**Known limitation: delivery timing for startup-only readers.** These files are
written into a container that is already running, so a process that reads them
exactly once at container start does not see a delivery that arrives afterwards
— on any path, including a clean `effigy dev`. This is a pre-existing
limitation of runtime-file delivery, not something this change introduces or
fixes; the planner tracks it as its own lead. Until it is addressed, a consumer
that must have the values at process start either reads the files per request
(as the PHP bootstrap example below does), sources `runtime.env` for the
process it starts, or gets its process restarted after a delivery — with the
caveat that a restart clears a tmpfs `runtime_dir`, so a delivery into a tmpfs
has to be repeated afterwards.

Core
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
- Required Rhai secrets unlock only when the selected script has an executable
  `secrets` module path (`secrets::`). Comments, string literals, and longer
  identifiers such as `mysecrets::` do not count. A secret-free script does not
  prompt or fail for unrelated required values.
  A script that uses `secrets::` still fails closed before side effects when a
  required value is missing or the vault is locked.
- Isolated `effigy skill run` tasks never resolve the consumer vault
- Known values are redacted from Rhai errors and host output maps
- Never build shell commands that embed secrets; use structured helpers

## Use In Deploy Hooks

Deploy provider packages and deploy/state hooks receive `targets = ["deploy"]`
and `targets = ["state"]` secrets through their process environment or Rhai
context automatically.

## Safety

- Values are never printed in JSON or text output
- Values are redacted in logs, errors, and provider reports
- Host task injection uses `Command::env()` instead of shell command strings
  to avoid `ps` exposure
- Direct vault access requires a human-gated unlock factor
- `effigy dev` uses an ignored, mode-`0600` local-dev key and never writes
  plaintext secret values to that key file
- Local-dev unlock is a runtime capability, not a security boundary against
  code allowed to run as the application

## JSON Output

`effigy --json secrets list` returns `effigy.secrets.v1` with metadata only — no
values, no hashes, no decrypted contents. See
[`017-json-output-contracts.md`](017-json-output-contracts.md).

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

- [`050-env-schema-integration.md`](050-env-schema-integration.md) — env schema
  for non-secret configuration
- [`061-rhai-script-steps-guide.md`](061-rhai-script-steps-guide.md) — secret
  access from Rhai scripts
- [`074-deployment-guide.md`](074-deployment-guide.md) — deploy secret targets
  and hooks
- [`../contracts/032-secret-and-local-config-management-contract.md`](../knowledge/contracts/032-secret-and-local-config-management-contract.md) — full contract
