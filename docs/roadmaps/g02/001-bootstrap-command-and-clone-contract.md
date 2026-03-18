# 001 - Bootstrap Command and Clone Contract

Generation: `g02`

Status: In Progress
Owner: Platform
Created: 2026-03-17
Depends on: 011, 015, 019, 027, 028

## Vision Alignment

This roadmap defines a first-class Effigy bootstrap surface for "clone this repo
here, bring along its declared companion repos, run setup, and optionally start
the dev environment" without introducing machine-global configuration or
turning bootstrap into an opaque shell-script convention.

The goal is to let an operator or agent run one command from an arbitrary local
directory:

```sh
effigy bootstrap <git-url>
```

and have Effigy:

- clone or update the target repo into an explicit destination
- read the repo's own bootstrap contract from `effigy.toml`
- sync submodules when requested
- clone or update declared child repos
- run declared setup tasks
- optionally launch the declared start task

This is not "remote execution" and it is not a global workspace manager. It is
a stateless repo acquisition and bring-up command that uses repo-local Effigy
contracts after the checkout exists.

## Primary Tags

- `OPERATE`
- `ROUTE`
- `CONTRACT`
- `MAINT`

## Target Envelope

- Effigy ships a built-in `bootstrap` command family that can clone/update a
  repo into the current directory tree without relying on machine-global state.
- A repo can declare a minimal `[bootstrap]` contract in root `effigy.toml`
  that tells Effigy how to finish setup after clone.
- Multi-repo development environments can declare child repos and optional
  submodule handling declaratively.
- Setup logic remains normal Effigy task composition inside the target repo
  instead of a second bootstrap-specific shell DSL.
- Operators can inspect the full plan before writing anything via `--plan` and
  can consume the same result through JSON.

## Vision Target Delta

- Moved from `repo acquisition and first-run setup require local folklore or ad
  hoc shell scripts` toward `Effigy-native, stateless bootstrap orchestration
  with repo-declared topology and setup semantics`.

## Source of Truth

This roadmap is based on current Effigy onboarding and built-in consolidation
surfaces:

- [`../../guides/019-watch-init-migrate-foundation.md`](../../guides/019-watch-init-migrate-foundation.md)
- [`../../guides/028-migration-quick-paths.md`](../../guides/028-migration-quick-paths.md)
- [`../../guides/047-agent-and-cross-repo-adoption.md`](../../guides/047-agent-and-cross-repo-adoption.md)
- [`../../guides/051-release-orchestration.md`](../../guides/051-release-orchestration.md)
- [`./028-script-surface-reduction-and-builtins.md`](./028-script-surface-reduction-and-builtins.md)

The design direction from this discussion is explicit:

- no machine-global config
- no hidden workspace root
- destination is cwd-relative by default
- `--path <DIR>` is the explicit override
- bootstrap is the product term; clone/update is one phase inside it

## Design Rules

### 1. Stateless by default

Effigy must not require global machine config for where repos live.

Default destination:

- `effigy bootstrap <git-url>` clones into `./<repo-name>` relative to the
  current working directory

Explicit override:

- `--path <DIR>` targets a specific destination

There is no hidden `~/Dev/projects` equivalent in product behavior.

### 2. Built-in owns acquisition, repo owns setup

Effigy should own:

- clone vs update decision
- destination resolution
- remote checkout
- submodule sync
- child repo orchestration
- plan/reporting/state

Repo manifests should own:

- which follow-up setup tasks run
- which task starts the dev environment
- which additional child repos belong to the environment

### 3. Do not invent a second shell language

Bootstrap config should not become a giant embedded setup DSL.

Good bootstrap fields:

- `setup = ["bootstrap:local", "doctor"]`
- `start = "aura/dev"`
- `submodules = "recursive"`
- declared child repos

Bad bootstrap fields:

- hundreds of inline shell commands
- machine-specific install locations
- ad hoc environment mutation rules unrelated to task execution

### 4. Start must be explicit

Bootstrap can safely run setup by default if the repo declares it, but starting
the live dev environment should require explicit intent:

- `effigy bootstrap <git-url>` -> clone/update + setup
- `effigy bootstrap <git-url> --start` -> clone/update + setup + start

This keeps bootstrap useful in CI, scripting, and review flows without
surprising operators by launching long-running processes automatically.

### 5. Respect existing local state

If the destination already exists, Effigy must behave predictably:

- clean matching checkout -> update allowed
- dirty checkout -> fail unless an explicit update policy says otherwise
- mismatched remote -> fail with clear diagnostics

Bootstrap should not silently repurpose an unrelated existing directory.

## Phase 1 Command Surface

Base command:

```sh
effigy bootstrap <git-url>
```

Initial flags:

- `--path <DIR>`
- `--branch <NAME>`
- `--start`
- `--plan`
- `--json`

Planned later flags:

- `--tag <TAG>`
- `--commit <SHA>`
- `--no-setup`
- `--no-children`
- `--submodules <none|init|recursive>`
- `--update-mode <fail|pull|reset>` if needed after real-world validation

Phase 1 behavior:

1. Resolve destination path.
2. Clone if missing, otherwise validate/update existing checkout.
3. Load root `effigy.toml` from the cloned repo.
4. Read `[bootstrap]`.
5. Apply submodule policy if declared.
6. Clone/update declared child repos.
7. Run declared setup tasks.
8. If `--start` is set, run the declared start task.

## Phase 1 Manifest Contract

Root contract:

```toml
[bootstrap]
setup = ["bootstrap:local", "doctor"]
start = "aura/dev"
submodules = "recursive"
```

Child repo contract:

```toml
[[bootstrap.children]]
path = "aura"
repo = "git@github.com:inflatable-cookie/aura.git"
branch = "main"
setup = ["install"]
required = true
```

### Proposed fields

#### `[bootstrap]`

- `setup`
  - optional list of task selectors to run after checkout/update
  - executed from the root repo
- `start`
  - optional task selector to launch when `--start` is supplied
- `submodules`
  - optional enum: `"none"`, `"init"`, `"recursive"`
  - default: `"none"`

#### `[[bootstrap.children]]`

- `path`
  - required relative directory under the root repo
- `repo`
  - required git remote URL
- `branch`
  - optional checkout branch
- `setup`
  - optional list of task selectors to run inside that child repo
- `required`
  - optional bool, default `true`

Later candidates only after validation:

- `tag`
- `commit`
- `start`
- `submodules`
- `catalog`
- `repo_override_policy`

## Destination and Path Rules

Default:

- infer repo name from git URL
- clone into `./<repo-name>`

Examples:

```sh
effigy bootstrap git@github.com:inflatable-cookie/loophole.git
# -> ./loophole

effigy bootstrap https://github.com/inflatable-cookie/loophole.git --path ./work/loophole
```

Rules:

- `--path` may point to a missing directory or an existing repo checkout
- `path` entries inside `[[bootstrap.children]]` are always relative to the
  cloned root repo
- child repos may not escape root via `..`

## Plan and JSON Requirements

`--plan` is required from day one.

It should show:

- resolved root destination
- clone vs update decision
- checkout ref/branch decision
- submodule policy
- child repo actions
- setup task plan
- start task plan

JSON should expose the same structure, for example:

```json
{
  "schema": "effigy.bootstrap.v1",
  "schema_version": 1,
  "ok": true,
  "mode": "plan",
  "root": {
    "repo": "git@github.com:inflatable-cookie/loophole.git",
    "path": "./loophole",
    "action": "clone"
  },
  "submodules": {
    "mode": "recursive"
  },
  "children": [
    {
      "path": "aura",
      "repo": "git@github.com:inflatable-cookie/aura.git",
      "action": "clone",
      "required": true
    }
  ],
  "setup": ["bootstrap:local", "doctor"],
  "start": {
    "requested": false,
    "task": "aura/dev"
  }
}
```

## Failure Rules

### Dirty existing root checkout

Default behavior:

- fail

Reason:

- bootstrap should not silently merge over a developer's in-progress changes

Required error shape:

- explain that the destination exists and is dirty
- show the path
- recommend manual cleanup or a future explicit update policy flag

### Existing checkout with mismatched remote

Default behavior:

- fail

Reason:

- same path should not be repointed implicitly

### Missing required child repo

If clone/update fails for a required child:

- fail the whole bootstrap

If clone/update fails for a non-required child:

- report warning
- continue setup for the rest of the environment

### Setup task failure

Default behavior:

- fail bootstrap immediately

Reason:

- setup tasks are the declared contract for a usable environment

### Start task failure

If `--start` was requested and the start task fails:

- bootstrap fails after setup
- output must clearly distinguish `checkout/setup succeeded` from `start failed`

## Scope Boundary

This roadmap does not yet include:

- distributed caching
- machine-global workspace management
- environment snapshot/restore
- remote secret provisioning
- cross-machine personal preferences
- project template generation

This command is for bringing up a declared repo environment locally, not for
becoming a full workstation state manager.

## Wave 1 - Phase 1 Built-in

Deliver the smallest useful built-in:

- `effigy bootstrap <git-url>`
- `[bootstrap]`
- `[[bootstrap.children]]`
- `--path`
- `--start`
- `--plan`
- `--json`

Tasks:

- [x] Define the parser and top-level built-in command family
- [x] Define `effigy.bootstrap.v1` JSON contract
- [x] Define the manifest schema for `[bootstrap]` and `[[bootstrap.children]]`
- [x] Implement destination inference from git URL
- [x] Implement root clone/update behavior
- [x] Implement root checkout validation for dirty/mismatched repos
- [x] Implement submodule policy handling
- [x] Implement child clone/update orchestration
- [x] Implement setup task execution
- [x] Implement opt-in start task execution
- [x] Add plan-mode text and JSON rendering
- [x] Add end-to-end fixture coverage for single-repo and child-repo bootstrap

Acceptance:

- an operator can bootstrap a repo from any cwd without machine-global config
- a repo can declare setup and start semantics in root `effigy.toml`
- multi-repo environments can be brought up from one command
- plan mode fully explains clone/update/setup/start decisions

## Wave 2 - Update and Recovery Hardening

Once Wave 1 is stable, harden the surface for real operator use.

Tasks:

- [ ] Add resume/recovery state if bootstrap becomes multi-minute and failure-prone
- [ ] Add explicit update-policy controls only if real-world dirt/update friction demands them
- [ ] Add tag/commit pinning when branch-only checkout proves insufficient
- [ ] Add child-specific diagnostics and partial-success summaries
- [ ] Evaluate whether `bootstrap` should support rerun-idempotence markers

Acceptance:

- repeated bootstrap runs are predictable
- recovery after partial failure is inspectable
- advanced checkout/update controls exist only where proven necessary

## Wave 3 - Product Boundary Review

After real use, decide what stays native and what should remain repo task logic.

Questions:

- should bootstrap own more than clone/update/orchestration?
- should child repo topology stay purely manifest-driven?
- do we need a repo-contract validator for `[bootstrap]` declarations?
- does bootstrap need its own doctor checks?

Acceptance:

- Effigy owns the durable reusable engine
- repo-local tasks remain the home for domain-specific provisioning
- the command stays simpler than the shell folklore it replaces

## Related Guides

- [`../../guides/019-watch-init-migrate-foundation.md`](../../guides/019-watch-init-migrate-foundation.md)
- [`../../guides/022-manifest-cookbook.md`](../../guides/022-manifest-cookbook.md)
- [`../../guides/047-agent-and-cross-repo-adoption.md`](../../guides/047-agent-and-cross-repo-adoption.md)
- [`../../guides/051-release-orchestration.md`](../../guides/051-release-orchestration.md)

## Next Task

Turn this roadmap into an implementation contract by defining the exact CLI
syntax, parser variants, and `effigy.bootstrap.v1` JSON schema before writing
any clone/update runtime code.
