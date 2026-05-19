# g07.051 - Init Context Inventory And Checklist Contract

Status: Complete
Depends on: `g07.050`

## Goal

Define the shared model that decides which setup jobs are relevant in a repo
and expose that model as a machine-readable checklist contract.

## Scope

- define setup-job identifiers, categories, safety class, and detection rules
- define which jobs are:
  - always available
  - available only when the repo declares a matching surface
  - inspection-only
  - guidance-only
  - mutation-capable
- define `effigy init --checklist --json`
- keep checklist output stable enough for agent consumption
- include enough metadata for a caller to decide what to run next

## Setup Job Model

Each setup job must have one stable identifier and one execution shape.

Required per-job fields:

- `id`
- `category`
- `execution_kind`
- `safety_class`
- `applicability`
- `default_selected`
- `prerequisites`
- `delegates_to`
- `writes`
- `summary`
- `reason_when_present`
- `reason_when_absent`

### `category`

- `baseline`
- `tasks`
- `health`
- `graph`
- `secrets`
- `runtime`
- `bundles`
- `docs`
- `validation`
- `advanced`

### `execution_kind`

- `inspect`
  - read-only command or file analysis
  - may appear in checklist and wizard
  - may execute during `--check`
- `guidance`
  - no direct mutation path in init
  - produces recommendation or next command only
  - wizard may surface it as an informational step
- `apply`
  - deterministic mutation or delegated command run
  - allowed only in interactive apply mode or explicit non-interactive action

### `safety_class`

- `safe_check`
  - read-only
  - never writes
- `safe_apply`
  - writes only local repo scaffolding or cache-like local state
  - no network, credentials, or long-running runtime side effects
- `contextual_apply`
  - may mutate config, pull dependencies, start local runtime, or initialize
    encrypted local state
  - only selected when the repo context proves the surface exists
- `never_default`
  - must not be auto-selected by plain init
  - init may only expose this as guidance, not execution

### `applicability`

- `always`
  - should be emitted for every repo
- `contextual`
  - emitted only when repo inspection proves the surface is present
- `conditional_missing`
  - emitted only when a baseline managed surface is absent or drifted

### `delegates_to`

One of:

- `init_internal`
- `builtin:<command>`
- `task:<selector>`
- `guidance_only`

The inventory must not invent delegate targets that Effigy cannot already run.

## Checklist JSON Contract

`effigy init --checklist --json` must return a normal command envelope with:

- top-level schema: `effigy.command.v1`
- result schema: `effigy.init.checklist.v1`

`effigy.init.checklist.v1` fields:

- `schema = "effigy.init.checklist.v1"`
- `schema_version = 1`
- `mode`
  - `checklist`
- `repo_root`
- `tty`
- `interactive_candidate`
  - whether plain `effigy init` would enter wizard mode here
- `has_changes`
  - whether any `apply` job is recommended and currently not satisfied
- `summary`
  - counts by `execution_kind`
  - counts by `safety_class`
  - counts by applicability result:
    - `applicable`
    - `not_applicable`
    - `already_satisfied`
- `jobs`

Each `jobs[]` entry:

- `id`
- `category`
- `execution_kind`
- `safety_class`
- `applicability`
  - `applicable`
  - `not_applicable`
  - `already_satisfied`
- `selected_by_default`
- `can_run_noninteractive`
- `can_run_in_wizard`
- `delegates_to`
- `prerequisites`
- `writes`
- `summary`
- `reason`
- `recommended_command`
- `artifacts`
  - optional paths or surfaces the job would inspect or write

Contract rules:

- checklist mode never writes
- `not_applicable` jobs may be omitted from human text output but stay available
  in JSON if the caller asks for full inventory later
- `already_satisfied` jobs still use the same `id`; they do not disappear
- `recommended_command` must be explicit enough for an agent to run directly
- `guidance` jobs cannot claim `can_run_noninteractive = true`

## Inventory v1

### Baseline repo contract

Baseline repo contract:

- `manifest.ensure`
- `readme.ensure`
- `agents_block.ensure`
- `skill_tree.sync`
- `gitignore.effigy_state`

Default shape:

- all baseline jobs use `category = baseline`
- `manifest.ensure`, `agents_block.ensure`, `skill_tree.sync`,
  `gitignore.effigy_state` are `conditional_missing`
- `readme.ensure` is `conditional_missing` and preserve-first
- all baseline jobs are `safe_apply`
- all delegate to `init_internal`

### Task-surface adoption

Task-surface adoption:

- `task_surface.scan`
- `task_migration.package_json`
- `task_migration.makefile`
- `task_migration.cargo_alias`
- `package_scripts.cleanup`
- `task_coverage.audit`

Default shape:

- `task_surface.scan` and `task_coverage.audit`
  - `execution_kind = inspect`
  - `safety_class = safe_check`
  - `applicability = always`
- `task_migration.*`
  - `execution_kind = apply`
  - `safety_class = contextual_apply`
  - `applicability = contextual`
  - `delegates_to = builtin:tasks migrate`
- `package_scripts.cleanup`
  - `execution_kind = guidance` in v1
  - actual rewrite waits for later product support

### Health and diagnostics

Health and diagnostics:

- `doctor.run`
- `tasks.inspect`
- `test_plan.inspect`

Default shape:

- all use `execution_kind = inspect`
- all use `safety_class = safe_check`
- `applicability = always`
- delegate to built-ins:
  - `builtin:doctor`
  - `builtin:tasks`
  - `builtin:test --plan`

### Graph

Graph:

- `graph_status.inspect`
- `graph_index.build`
- `graph_watch.guidance`

Default shape:

- `graph_status.inspect`
  - `inspect`
  - `safe_check`
  - `always`
  - `builtin:graph status`
- `graph_index.build`
  - `apply`
  - `safe_apply`
  - `always`
  - `builtin:graph index`
- `graph_watch.guidance`
  - `guidance`
  - `safe_check`
  - `always`
  - `guidance_only`

### Secrets

Secrets:

- `secrets_surface.inspect`
- `secrets_vault.init`
- `secrets_doctor.run`
- `secrets_first_entry.guidance`

Default shape:

- all use `category = secrets`
- all are `contextual`
- `secrets_surface.inspect` and `secrets_doctor.run`
  - `inspect`
  - `safe_check`
- `secrets_vault.init`
  - `apply`
  - `contextual_apply`
  - delegate to `builtin:secrets init`
- `secrets_first_entry.guidance`
  - `guidance`
  - `contextual_apply`
  - `guidance_only`

### Runtime and bundles

Runtime and bundles:

- `containers_surface.inspect`
- `containers_up.guidance`
- `bundle_surface.inspect`
- `bundle_sync.run`

Default shape:

- runtime/container jobs are `contextual`
- `containers_surface.inspect`
  - `inspect`
  - `safe_check`
- `containers_up.guidance`
  - `guidance`
  - `contextual_apply`
  - never starts long-running runtime automatically in v1
- `bundle_surface.inspect`
  - `inspect`
  - `safe_check`
- `bundle_sync.run`
  - `apply`
  - `contextual_apply`
  - delegate to `builtin:bundle sync`

### Docs and QA

Docs and QA:

- `agent_docs.audit`
- `agent_defaults_qa.guidance`
- `validation_command.recommend`

Default shape:

- `agent_docs.audit`
  - `inspect`
  - `safe_check`
- `agent_defaults_qa.guidance`
  - `guidance`
  - `safe_check`
- `validation_command.recommend`
  - `guidance`
  - `safe_check`
  - selects the best available validation command but does not execute it in
    checklist mode

### Advanced read-only setup checks

Advanced read-only setup checks:

- `state_surface.inspect`
- `deploy_surface.inspect`
- `distribution_surface.inspect`
- `release_surface.inspect`

Default shape:

- all are `inspect`
- all are `safe_check`
- all are `contextual`
- they exist so init can explain the repo surface without claiming it can drive
  higher-risk mutation from setup

## Applicability Rules

The inventory must decide applicability from repo facts, not prompt-time whim.

Minimum context inputs:

- manifest present or absent
- README present or absent
- `AGENTS.md` present and managed block status
- project-local skill tree present and drift status
- `.gitignore` managed block status
- package.json / Makefile / cargo alias presence
- declared graph state
- declared secrets config
- declared container/runtime config
- declared bundle config
- declared state / deploy / distribution / release config
- available Effigy tasks and QA surfaces

Checklist output may include an optional `context` summary, but the stable
contract is the job list, not the raw context dump.

## Guardrails

- do not force every repo through every setup job
- checklist mode must not mutate
- do not blur inspection with mutation in the contract
- avoid repo-specific doctrine in core job identifiers
- do not emit fake jobs that init cannot actually run or delegate
- do not make TTY-only concepts part of the checklist schema
- do not require an agent to infer execution order from prose alone

## Explicit Non-Goals For v1

- no multi-select freeform prompt design in the contract
- no background `graph watch` start from init
- no deploy, state apply, distribution publish, or release execute job
- no repo-specific custom task jobs unless backed by a declared Effigy task
- no hidden package-script rewriting beyond commands Effigy already owns

## Acceptance Criteria

- one explicit setup-job inventory exists
- every job has a stable identifier and action type
- checklist JSON distinguishes:
  - applicability
  - safety
  - default recommendation
  - required prerequisites
- TTY and non-TTY init can both consume the same inventory later

## Evidence

- [`2026-05/19-120509-init-checklist-contract.md`](../../logs/2026-05/19-120509-init-checklist-contract.md)

## Next Task

Execute `1002`.
