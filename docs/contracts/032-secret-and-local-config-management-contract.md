# Secret and Local Config Management Contract

Status: Active
Owner: Platform maintainers
Roadmaps: `g05.001` through `g05.007`

## Purpose

Define Effigy's next-generation secret and local configuration contract.

The contract separates portable non-secret configuration from true secret
material, then provides a human-gated secret vault and runtime injection model
that works across tasks, containers, Rhai scripts, deploy provider packages,
and Underlay-based applications.

## Problem

Current `.env` usage mixes unrelated concerns:

- local ports, hosts, service names, bucket names, and feature toggles
- generated container/runtime values
- actual credentials and API tokens

That shape is no longer acceptable for agent-heavy development. Agents often
need broad read access to repo files, but they should not automatically gain
plaintext credentials from `.env`.

Varlock support exists, but it has not become the portable default because it
adds dependency and integration friction around containers, deploy providers,
and consumer repo setup.

## Core Rules

- Non-secret config belongs in typed config files or generated runtime config,
  not secret stores.
- Secret declarations belong in the Effigy manifest so required names,
  consumers, and validation rules are repo-portable.
- Secret values must not be committed to the repo.
- Secret values must not be written to persistent `.env` files by default.
- Secret unlock must be an explicit operator act.
- SSH-agent access alone is not enough to make a vault agent-safe.
- The default Effigy vault must require a human-gated unlock factor, such as a
  passphrase, in addition to any key identity.
- Effigy may support external adapters later, but the Effigy manifest contract
  is the durable source of truth.
- Secrets must be redacted in text output, JSON reports, errors, logs, and
  provider script reports.
- Runtime injection must avoid shell command strings where possible.

## Public Manifest Shape

Initial target:

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

The final parser may use snake-case or kebab-case field aliases if that is
consistent with existing manifest grammar, but the semantic model is fixed:

- one `[secrets]` root
- one backend selector
- backend-specific configuration under `[secrets.<backend>]`
- named secret declarations under `[secrets.keys.<name>]`
- explicit consumer targets

## Config Versus Secrets

Effigy must help repos move these values out of secret storage:

- ports
- local hostnames
- service aliases
- container image names
- object-store bucket names when not confidential
- database names
- non-secret usernames
- feature flags
- environment names
- generated internal URLs

These values should live in one of:

- Underlay or app config such as `config/local.toml`
- bundle-provided defaults
- Effigy manifest config
- generated runtime config under `.effigy/runtime/`

Only credentials, tokens, private keys, signing secrets, and values that grant
access should be handled as secrets.

## Vault Model

The built-in vault should use conservative, standard primitives rather than a
novel crypto design:

- encrypted records or an encrypted document for key/value entries
- random data-encryption key per vault
- authenticated encryption for vault payloads
- key wrapping for configured identities
- passphrase-derived unlock material using a memory-hard KDF
- explicit metadata for algorithm and version

The MVP should support one local operator vault. Team recipient management,
remote sharing, and hosted secret sync are future work.

## Unlock Policy

Supported policy targets:

- `passphrase`: unlock with a local passphrase only
- `key-and-passphrase`: require an available key identity and passphrase
- `external`: reserved for a future backend adapter contract

`key-only` is not the default and should not be accepted for the built-in vault
unless a later roadmap explicitly justifies its safety boundary.

Reason: an agent process with filesystem and SSH-agent access may otherwise be
able to decrypt without a human decision.

## Runtime Injection

Effigy-owned injection targets:

- task execution
- container services
- Rhai scripts
- deploy provider packages
- state and artifact workflows that need credentials

Injection rules:

- pass secret values directly to child process environments where possible
- avoid putting secrets into shell command strings
- task execution receives `targets = ["tasks"]` values through process
  environment injection
- Rhai scripts can request declared values through `secrets::get(name)` and
  test availability with `secrets::has(name)`
- deploy provider package scripts run with `targets = ["deploy"]` access
- state apply hook tasks can receive `targets = ["state"]` values through the
  same process environment injection path
- artifact-targeted Rhai workflow callers may opt into `targets = ["artifacts"]`
  when an artifact workflow has a script execution point
- avoid persistent `.env` files by default
- if a compatibility file is required, write it under `.effigy/runtime/`,
  redact it from reports, and make lifecycle/cleanup explicit
- never print secret values in JSON or text output

## Command Surface

Target command family:

```sh
effigy secrets init
effigy secrets list [--json]
effigy secrets set <name>
effigy secrets unset <name>
effigy secrets doctor [--json]
effigy secrets unlock
effigy secrets lock
effigy secrets export --format env --output <PATH> --yes
```

The `export` command is a compatibility escape hatch, not the default runtime
path.

## `.env.schema` Relationship

`.env.schema` remains useful for validation and legacy task environments, but
it is not the long-term source of truth for secrets.

Decision for `g05`:

- `.env.schema` continues as a compatibility and validation layer
- `.env.schema @sensitive` behavior is preserved for existing task execution
- `[secrets]` supersedes `.env.schema` as the portable declaration surface for
  true secret values
- no automatic bridge from `.env.schema` to `[secrets]` is added in `g05`

No roadmap should silently remove existing `.env.schema` behavior.

## Varlock Posture

Varlock is deferred for `g05`.

Effigy keeps the parts already internalised from the earlier env-schema work:

- native `.env.schema` parsing
- validation
- redaction for `@sensitive` values
- task-time environment resolution

Effigy does not ship a live Varlock backend adapter in this generation.

Reasons:

- the built-in vault now provides the no-dependency local secret path
- the `[secrets]` manifest model is the durable cross-surface contract
- a Varlock adapter would need a separate command, unlock, status, error, and
  provider boundary review
- adding that adapter now would increase integration risk after task,
  container, Rhai, state, artifact, and deploy injection already work through
  one model

Parser support for `backend = "external"` is a reserved manifest shape only.
It should not be documented as an operational Varlock path until a future
roadmap defines and tests the adapter contract.

## Underlay Contract

Underlay should become the documentation authority for Underlay-based app
implementation:

- standard local config layout
- which values belong in `config/local.toml`
- which values are generated by Effigy
- which values must be declared as Effigy secrets
- how containerized local development receives secrets

Effigy owns the tooling. Underlay owns the app-facing convention.

## Non-Goals

- No provider-hosted secret provisioning in the first pass.
- No automatic migration of production secrets.
- No team secret-sharing protocol in the MVP.
- No hidden reads from arbitrary `.env` files as the primary secret source.
- No promise that unlocked secrets are safe from a process the operator chooses
  to run with those secrets.
- No bespoke cryptographic primitives.

## Acceptance Criteria

- Repos can declare required secrets without storing values.
- A local developer can initialize, set, unlock, validate, and inject secrets
  without Varlock.
- Unlock requires explicit operator participation.
- Task, container, Rhai, and deploy paths can consume declared secrets through
  one model.
- Secret values are redacted from normal output and reports.
- Underlay and Acowtancy can move non-secret `.env` values into ordinary config.
- Varlock is clearly documented as deferred.
