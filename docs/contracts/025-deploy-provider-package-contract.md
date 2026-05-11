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
state = "uat"
code_ref = "branch:main"

[deploy.uat.provider]
adapter = "render"
```

For Effigy repo development, the first-party provider packages live under
`external/providers/` as Git submodules:

```toml
[deploy.providers.render]
source = { type = "path", dir = "external/providers/render" }
```

Hydrate them with `git submodule update --init --recursive`.

The provider name in `[deploy.<env>]` must resolve to a configured
`[deploy.providers.<name>]` package. v0.6.0 removes the built-in provider
adapter fallback; provider-specific behavior belongs in provider packages.
The implementation validates `path` and `git` provider package sources during
deploy planning. OCI provider package materialization is reserved for a later
slice.

## Execution Contract

Effigy invokes provider scripts with a versioned JSON context file and expects
a versioned JSON report through the provider Rhai surface.

Provider scripts read context with:

```rhai
let context = deploy::provider_context();
```

Provider scripts write their phase report with:

```rhai
deploy::provider_report(#{
    schema: "effigy.deploy-provider.report.v1",
    phase: "preflight",
    provider: "render",
    status: "planned",
    checks: [],
    warnings: [],
    blockers: [],
    files: [],
});
```

Effigy also exposes `deploy::provider_context_path()` and
`deploy::provider_report_path()` for scripts that need file paths directly.

Context schema:

```json
{
  "schema": "effigy.deploy-provider.context.v1",
  "phase": "preflight",
  "env": "uat",
  "provider": {
    "adapter": "render",
    "project_id": "prj-...",
    "environment_id": "env-...",
    "services": {
      "front": "srv-..."
    }
  },
  "provider_project": "acowtancy-uat",
  "provider_package": {
    "root": "/repo/.effigy/cache/providers/render",
    "name": "render",
    "display_name": "Render",
    "version": "0.1.0"
  },
  "deploy": {
    "state": "uat",
    "code_ref": "branch:main",
    "release_policy": "optional",
    "artifact_policy": "digest-preferred"
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

`deploy plan` runs provider-package `preflight.rhai` when declared. Effigy
merges reported `checks` into `provider_preflight.checks`, converts
warnings/files into provider checks, and blocks the plan when the provider
report returns blockers or an unsupported status.

`deploy apply` runs provider-package `apply.rhai` after the plan passes and
`--yes` is supplied. The provider report determines the provider operation
status in `effigy.deploy.apply.v1`; core Effigy still owns report persistence,
state/hook/health bookkeeping, and transaction gates.

`deploy status` runs provider-package `status.rhai` when the deploy environment
and provider package can be resolved, and includes the returned provider report
as `provider_status` in `effigy.deploy.status.v1`.

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

- runtime context: repo root, invocation cwd, catalog root
- provider context/report helpers under `deploy::*`
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
