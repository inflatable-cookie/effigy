# Secret and Local Config Management Contract

Owner: Platform maintainers

## Managed task transport

Managed host workloads receive task values in their spawn environment. Managed
inline Compose exec and named workloads forward only `-e NAME` to the runtime;
neither host nor guest arguments, rendered wrappers, plans, logs or JSON carry
task values. Named exec retains its runtime, exact container ownership,
workspace identity and process-group settlement checks.

When stdin is free, a private environment entry carries a shell bootstrap which
shell builtins write to stdin of the guest `sh -s` (or a local `sh -s` for Docker).
The bootstrap exports values before exec; no script contents enter argv or logs.
Colima uses `colima ssh --profile … -- sh -s` and name-only sudo environment
preservation before nerdctl. Multiline values and trailing newlines survive. Capture also redacts the CRLF
display representation of multiline values produced by a PTY.

Interactive exec keeps stdin for terminal input. Before launch the supervisor
journals an exact-owned cleanup obligation, then writes a 0600 `env.delivery`
inside a 0700 per-launch directory under the owner's HOME
(`~/.effigy/managed-secrets/`). The VM reads the host HOME mount, sources and
unlinks the file before sudo or the runtime starts. That removal acknowledges
the first read; the supervisor refuses a launch that has not acknowledged in
60 seconds. A launch that exits before the read fails and immediately settles
its unread file; only the guest read can acknowledge delivery. The kernel settles
the owned directory after success, stop, kill or supervisor loss. Cleanup checks the
recorded directory device/inode, process identity, file type, owner and modes;
it never follows symlinks or recursively removes a tree. The next invocation,
including doctor, sweeps only directories whose recorded process identity is
proven non-live; unknown liveness retains them, and a live run is untouched.

Effigy Supervisor ruling, 2026-10-28 (brief approved by Tom): the threat model
excludes secret values in host and guest argv and routine output. Residual
same-user-readable process environments and the bounded 0600 file window are
accepted at the trust level of repository `.env` files. HOME must be visible to
the Colima guest for interactive delivery; failure to read it refuses the
launch without weakening process custody. This does not add an OwnedRemote
interface or a separate runtime registry.

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
- Direct vault inspection and mutation must remain an explicit operator act.
- `dev` task execution may use a repo-local runtime unlock key created during
  an earlier operator-authorized vault write or unlock.
- SSH-agent access alone is not enough to make a vault agent-safe.
- The default Effigy vault requires a human-gated passphrase.
- Effigy may support external adapters later, but the Effigy manifest contract
  is the durable source of truth.
- Secrets must be redacted in text output, JSON reports, errors, logs, and
  provider script reports.
- Runtime injection must avoid shell command strings where possible.

## Vault policy retirement

Tom required consumer cleanup before removal (2026-10-19); that cleanup
removed the inert declarations from all nine consumer files. The
`[secrets.vault].identity`, `[secrets.vault].unlock` and
`[secrets.vault].external` keys are retired. Any declaration fails with the
source file, line and instruction: "delete this line; it only restated the
passphrase default". Vault paths, values and runtime unlock behavior are
unchanged. The built-in vault always uses its passphrase policy.

## Public Manifest Shape

Initial target:

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

The final parser may use snake-case or kebab-case field aliases if that is
consistent with existing manifest grammar, but the semantic model is fixed:

- one `[secrets]` root
- one backend selector
- backend-specific configuration under `[secrets.<backend>]`
- named secret declarations under `[secrets.keys.<name>]`
- explicit consumer targets

For the built-in `effigy-vault` backend, `[secrets.vault].generate` is a
local/dev bootstrap hook only. Effigy may use it from `effigy secrets init`
and from `secrets = "required"` task startup when a required local vault is
missing or incomplete. It is not a generic deploy/state/artifact generation
hook.

While the hook runs, a Rhai step may read and write declared rhai-targeted
secrets even when `required` keys are still absent: creating them is the
hook's purpose. Vault existence, unlock, declaration and target checks still
apply. Outside the hook, Rhai secret access refuses before side effects when
a `required` rhai-targeted key is missing.

The hook is a declared durable descendant of task startup, so the tree
classifier includes it before the first effect (contract
[052](052-runtime-kernel-contract.md) §3.1). A direct-shell task with
`secrets = "required"`, a task-targeted declared key and a configured
`[secrets.vault].generate` is admitted checkout-durable, and the hook's
`secrets init` and task run execute inside that admitted tree. The join is
static: the vault's contents cannot choose a run class, so such a task is
admitted durable even when its required keys are already stored. An
undeclared durable child, such as shell code launching a named or detached
Effigy run, still refuses before its effects.

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
- passphrase-derived unlock material using a memory-hard KDF
- explicit metadata for algorithm and version
- a separately encrypted local-dev payload, unlocked by ignored machine-local
  state rather than the vault passphrase

The MVP should support one local operator vault. Team recipient management,
remote sharing, and hosted secret sync are future work.

The built-in `effigy-vault` backend is the local/dev backend. Production or
team-hosted secret storage remains a separate concern.

## Unlock Policy

The built-in vault uses a passphrase. Mutation, `import`, `change-passphrase`
and `export` operations follow that policy until an operator runs
`effigy secrets unlock`.
That command verifies the passphrase and seals a local command credential with
the adjacent local-dev key. Secret-backed tasks and direct commands can then
run unattended until `effigy secrets lock` removes both local files. A resolved
`dev` task may decrypt the separate local-dev payload with `.effigy` state
without the command credential. `effigy secrets doctor` and `effigy secrets
get` also read through that key when it exists, reporting `unlocked (local-dev
key)`; doctor prints stored key names and never a value, and `get` is the only
command that prints one. New vaults create that key at initialization.
A legacy vault requires one final passphrase unlock from `effigy dev` or
`effigy secrets unlock`, which upgrades it in place. Changing the passphrase
also rotates an active command credential.

The local-dev key is a runtime capability, not an isolation boundary against a
repo-owner process. A process allowed to run or alter the application can also
influence how injected secrets are consumed. Mutation and export remain
passphrase-gated; disclosure is limited to `secrets get` and doctor's key names.

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
- selection classifies the minimum secret scope for the selected local
  operation before any vault prompt
- task execution receives referenced `targets = ["tasks"]` values through
  process environment injection
- managed tasks that set `secrets = "required"` receive declared
  `targets = ["tasks"]` values across their child process launches, even when
  the child shell commands do not spell out the env names directly
- managed host process launches receive those task secrets through the child
  process environment, outside the rendered shell command and argv; captured
  headless log output redacts the resolved values
- resolved `dev` tasks use the local-dev payload without prompting; after an
  explicit `secrets unlock`, other task names and direct vault commands use the
  sealed local command credential without prompting
- a managed session resolves the secret scope it needs **once**, before any
  front end renders (TUI, headless or detach), in this order: what the
  invocation already holds, the sealed command credential `effigy secrets
  unlock` leaves, or one prompt on the real terminal (Tom 2026-10-23, recorded
  in Q-007 "Secrets before the TUI"). With no terminal and no other source it
  refuses before starting anything, naming what is locked and the next
  command: `effigy secrets unlock`, or `EFFIGY_SECRET_PASSPHRASE`
- the session's access reaches **Effigy's own child processes only**, and only
  through a one-shot inherited descriptor the child reads once at startup and
  closes (052 K12, 052 §5.7). Named children: the run's supervisor, the managed
  environment `up`, and a nested Effigy step Effigy itself launches. It never
  reaches a command line, a rendered shell string, a spawn environment a
  workload inherits, or any later child of a workload; a process listing shows
  neither argv nor an environment value for it (effigy#338)
- a user workload never receives the passphrase or the local-dev marker: every
  workload and host-process spawn starts from an ambient environment with the
  session-access keys removed, and the kernel no longer offers a command-string
  route for them. That one spawn helper also removes every other Effigy control
  marker: no `EFFIGY_INTERNAL_*` key, `EFFIGY_MANAGED_RUN_TOKEN` or
  `EFFIGY_SECRETS_REQUIRED` is inherited from ambient, so a workload sees the
  user's environment plus the documented Effigy variables. Three exceptions
  stand, each because a child depends on it: `EFFIGY_INTERNAL_CONTAINER_HANDOFF`,
  which a consumer's Rhai task reads to detect the workspace container;
  `EFFIGY_INTERNAL_TASK_SECRET_GENERATION_ACTIVE`, which stops a
  `[secrets.vault].generate` hook's own child Effigy from running the hook
  again; and `EFFIGY_INTERNAL_EXTERNAL_TASK_SOURCE_ISOLATION`, which makes a
  nested Effigy under an isolated external task source resolve no consumer
  secrets (effigy#341). A nested `effigy` started from user code (a shell tab, a
  manifest `run` command, a seeded workspace task) is therefore a top-level
  invocation: the local-dev key's own rules apply, or it asks once on its own
  terminal. It does not inherit the session's access
- a command that runs inside a container never carries session access either:
  a container command string is not a descriptor handoff, so an Effigy process
  there resolves its own access
- the session never writes a sealed credential for itself: sharing is not an
  implicit `secrets unlock`, and no value reaches argv, a rendered shell string
  or a log; a value a child echoes is redacted like any other resolved secret
- `EFFIGY_SECRET_PASSPHRASE` names the passphrase a caller supplies for one
  invocation, which is what a headless session with no terminal uses. It is a
  source, not a credential: nothing is sealed or written
- the local-dev key keeps its reach. Prompt-free use of it without a passphrase
  stays limited to a resolved `dev` session (below); a session whose vault
  holds only that key asks once before it renders rather than inside a tab, and
  the key is never handed to another task's children
- a declared container key that is not delivered is reported once, naming the
  key, the reason and the fix, and recorded under `warnings` in the bring-up's
  JSON envelope; required keys still fail the bring-up, and no bring-up writes
  an empty runtime secret file
- container operations unlock `targets = ["containers"]` values only when the
  selected operation starts a runtime (`container up`); read, exec, logs,
  stats, reset, and teardown stay secret-free. Reset tears down runtime
  artifacts and does not inject secrets; a later `container up` is the
  secret-consuming bring-up
- Rhai scripts can request declared values through `secrets::get(name)` and
  test availability with `secrets::has(name)`
- a Rhai script unlocks required `targets = ["rhai"]` values only when the
  selected source has an executable `secrets` module path (`secrets::`).
  Comments, string literals, and longer identifiers such as `mysecrets::`
  do not count. Secret-free scripts do not open the vault
- `eval` and imported modules that first call `secrets::` after other side
  effects stay fail-closed at the `secrets::*` call rather than at script
  start; classify those as secret-consuming when the selected source itself
  has an executable `secrets::` path
- isolated skill runs never resolve or unlock the consumer vault
- a secret-consuming operation still fails closed with a precise missing-secret
  diagnostic when a required value in that scope is absent or the vault is
  locked
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
effigy secrets get <name>
effigy secrets set <name>
effigy secrets unset <name>
effigy secrets doctor [--json]
effigy secrets change-passphrase
effigy secrets export --format env --output <PATH> --yes
```

The `export` command is a compatibility escape hatch, not the default runtime
path.

If `[secrets.vault].generate` is configured, `effigy secrets init` should
create the local vault and then run that hook instead of stopping at an empty
vault.

## `.env.schema` Relationship

`.env.schema` remains useful for validation and legacy task environments, but
it is not the long-term source of truth for secrets.

Current compatibility rule:

- `.env.schema` continues as a compatibility and validation layer
- `.env.schema @sensitive` behavior is preserved for existing task execution
- `[secrets]` supersedes `.env.schema` as the portable declaration surface for
  true secret values
- no automatic bridge from `.env.schema` to `[secrets]` is added

Changes to `.env.schema` behavior require an explicit compatibility decision.

## Varlock Posture

Varlock remains deferred.

Effigy keeps the parts already internalised from the earlier env-schema work:

- native `.env.schema` parsing
- validation
- redaction for `@sensitive` values
- task-time environment resolution

Effigy does not ship a live Varlock backend adapter .

Reasons:

- the built-in vault now provides the no-dependency local secret path
- the `[secrets]` manifest model is the durable cross-surface contract
- a Varlock adapter would need a separate command, unlock, status, error, and
  provider boundary review
- adding that adapter now would increase integration risk after task,
  container, Rhai, state, artifact, and deploy injection already work through
  one model

Parser support for `backend = "external"` is a reserved manifest shape only.
It should not be documented as an operational Varlock path until a future product decision defines and tests the adapter contract.

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

## Vault persistence

`effigy-secrets::storage` owns reads and writes for the CLI, runtime local-dev
upgrades and Rhai. A bounded machine `Gate(Store vault)` covers each complete
read–modify–write, including passphrase rotation and unlock/lock mutations.
Prompts and generation hooks run outside the gate. Writers resolving through
linked worktrees use the same gate as direct primary-checkout writers.

The configured vault path selects a kernel generation. Its single part,
`<vault>.ciphertext.g<N>`, contains the existing encrypted envelope, including
both passphrase and optional local-dev ciphertext, salts and nonces. Kernel
`commit_generation` writes and fsyncs that part, then atomically replaces the
pointer. Ciphertext never enters `kernel.db`. The previous pointer is
`<vault>.prev`; the selected and previous ciphertext generations are retained.
A legacy envelope at the configured path is read directly and preserved as
`.prev` on the first successful write. Unknown or unreadable state refuses
mutation, rather than being replaced with an empty vault.

The existing `.local-dev-key` remains stable while present. It is created
with kernel `atomic_replace` before selecting a local-dev-enabled envelope;
a failed first installation cannot remove the old passphrase ciphertext.
`secrets lock` still removes that key and the `.local-unlock` credential.
Auxiliary key and credential writes also use private (0600) atomic replacement
under the same gate. Unlock policy and passphrase delivery are unchanged.

`test:secrets:vault` proves legacy migration, retained decryptability,
cross-process exclusion and Kill/ENOSPC/EPERM at every crossed kernel boundary
during migration and passphrase rotation. The ratchet explicitly treats the
CLI, Rhai and shared storage modules as state writers even though their vault
paths arrive as parameters; the removed raw writes cannot return unnoticed.
The engine proof
`faults_engine_vault_rotation` lives with the shared writer, replacing the
kernel's engine placeholder. `test:secrets:callers` and `test:secrets:cli` cover
CLI/Rhai/task/container parity, including simultaneous CLI and Rhai updates.

## Acceptance Criteria

- Repos can declare required secrets without storing values.
- A local developer can initialize, set, unlock, validate, and inject secrets
  without Varlock.
- Unlock requires explicit operator participation.
- Task, container, Rhai, and deploy paths can consume declared secrets through
  one model.
- Secret-free local operations do not prompt for an unrelated required vault
  value. Secret-consuming operations still fail closed.
- Secret values are redacted from normal output and reports.
- Underlay and Example App can move non-secret `.env` values into ordinary config.
- Varlock is clearly documented as deferred.
