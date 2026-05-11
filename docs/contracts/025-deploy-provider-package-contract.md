# 025 - Deploy Provider Package Contract

Effigy deployment provider support must be portable. Core Effigy owns the
provider-neutral deployment transaction. Provider packages own provider-specific
export, checklist, validation, preflight, apply, and status behavior.

## Package Shape

A deploy provider package is a git, path, or OCI materialized directory with:

```text
provider.toml
scripts/
  checklist.rhai
  export.rhai
  validate.rhai
  preflight.rhai
  apply.rhai
  status.rhai
templates/
docs/
```

Only `provider.toml` is required. Scripts are optional and phase-scoped.
Missing scripts mean the phase is unsupported by that provider package.

## Provider Descriptor

```toml
[provider]
schema = "effigy.deploy-provider.v1"
name = "render"
display_name = "Render"
version = "0.1.0"

[capabilities]
checklist = "scripts/checklist.rhai"
export = "scripts/export.rhai"
validate = "scripts/validate.rhai"
preflight = "scripts/preflight.rhai"
apply = "scripts/apply.rhai"
status = "scripts/status.rhai"

[policy]
creates_projects = false
creates_services = false
creates_resources = false
creates_variables = false
creates_domains = false
prints_secret_values = false
```

Capability paths are provider-package-relative Rhai files.

## Consumer Config

Provider packages should be configured separately from environment selection:

```toml
[deploy.providers.render]
source = { type = "git", url = "git@github.com:inflatable-cookie/effigy-provider-render.git", ref = "main" }

[deploy.uat]
provider = "render"
state = "uat"
code_ref = "branch:main"
```

The provider name in `[deploy.<env>]` must resolve to a configured
`[deploy.providers.<name>]` package or a built-in compatibility provider.
The first implementation validates `path` and `git` provider package sources
during deploy planning. OCI provider package materialization is reserved for a
later slice.

## Execution Contract

Effigy invokes provider scripts with a versioned JSON context file and expects
a versioned JSON report on stdout or at an output path supplied in context.

Context schema:

```json
{
  "schema": "effigy.deploy-provider.context.v1",
  "phase": "preflight",
  "env": "uat",
  "provider": "render",
  "repo_root": "/repo",
  "provider_root": "/repo/.effigy/cache/providers/render",
  "deploy_plan": {},
  "deploy_model": {},
  "state_plan": {},
  "redaction": {
    "secret_names": ["DATABASE_URL"]
  }
}
```

Report schema:

```json
{
  "schema": "effigy.deploy-provider.report.v1",
  "phase": "preflight",
  "provider": "render",
  "status": "planned",
  "checks": [],
  "warnings": [],
  "blockers": [],
  "files": []
}
```

Provider scripts must not invent Effigy transaction state. They can add
provider evidence, warnings, blockers, generated files, and operation reports.
Core Effigy remains responsible for final report persistence and command
success/failure semantics.

## Safety Rules

Provider packages must default to read-only planning.

Mutating phases require an explicit `--yes` gate from core Effigy. Provider
scripts must receive the approved mutation mode in context; they must not infer
approval from environment variables or TTY state.

Provider packages must never:

- print secret values
- write secret values into reports
- create projects, services, resources, variables, or domains unless their
  descriptor policy explicitly allows that capability and core Effigy passed a
  mutating approval
- silently select fallback projects or services
- perform database rollback

Secrets are referenced by name and provenance only. Secret values come from
operator-owned inputs, not committed provider config.

## Rhai Surface Requirements

Core Effigy must expose enough Rhai API for provider packages to avoid shell
glue:

- runtime context: repo root, provider root, invocation cwd, catalog root
- JSON/TOML read/write helpers
- template rendering or deterministic text file writing
- HTTP helpers with redaction support
- process helpers for provider CLIs with redacted output capture
- report writing helpers
- path helpers scoped to provider root and repo root
- deploy context helpers for phase, model, plan, state, artifacts, and secrets

When a provider script needs a shell escape or raw env secret lookup, that is a
signal to widen the typed Rhai surface before making the provider package
canonical.

## Built-In Compatibility

Existing built-in Render and Railway behavior may remain as compatibility
adapters while provider packages stabilize. New provider-specific behavior
should prefer provider packages over hardcoded core branches.
