# 038 - Unified Test Orchestration Contract

Status: active
Owner: Platform
Release: v0.11

## Purpose

`effigy test` is one deterministic test front door for single-language and
polyglot repositories. It is always the built-in orchestrator; manifest task
routing must never shadow its flags or execution model.

## Authority

The top-level `[test]` section is the only test-orchestration configuration
authority. `[tasks.test]` is removed in v0.11. Repositories use auto-detection
or declare named `[test.suites]` entries.

```toml
[test.suites]
rust = "cargo nextest run --workspace"
ts = "bun x vitest run"
```

Configured suites replace auto-detection for their catalog. With no configured
suites, Effigy detects every supported ecosystem present at that catalog root:
Vitest plus one Rust runner (`cargo nextest` preferred, `cargo test` fallback).

## Command Grammar

- `effigy test` runs every selected suite across the declared catalog set.
- `effigy test <suite>` runs one named suite across matching catalogs.
- `effigy <catalog>/test` runs every selected suite in one catalog.
- `effigy <catalog>/test <suite>` runs one suite in one catalog.
- `effigy test --plan [suite]` resolves and renders the same selection without
  running setup, suites, teardown, task references, or scripts.
- runner passthrough is allowed only when selection resolves unambiguously.

Suites are on the default board unless a full suite table sets
`default = false`. On-demand suites remain available through
`effigy test <suite>` but do not run for bare `effigy test`.

`--plan` is a hard no-execution boundary. Future routing or configuration
changes may enrich its output but may not delegate to an executable task.

## Suite Flexibility

Plain suite strings remain supported. Full suite tables own env, env files,
setup, teardown, and teardown policy. Suite `run` accepts the managed run-step
grammar so commands, task references, Rhai steps, ordered sequences,
dependencies, timeouts, and retries stay declarative under `[test]`.

Suite planning renders the resolved target, source, suite, command or step
chain, cwd, lifecycle stages, and forwarded arguments. Opaque runtime behavior
must be labelled; planning must not execute it to discover more detail.

Task-reference expansion preserves two distinct contexts. The selected catalog
owns the task working directory. The originating repository owns the already
loaded catalog graph and execution registries, including an ancestor
`[containers]` default. A child catalog's explicit execution registry wins;
the ancestor registry is the fallback. Expansion must not rerun repository
discovery from the child working directory and silently discard that fallback.
Direct invocation from the child remains governed by normal root selection and
does not gain ambient configuration from an undeclared ancestor.

## Workspace Ownership

Root fanout includes declared catalogs by default. When a parent suite already
owns a nested workspace, exclude the child alias explicitly:

```toml
[test]
exclude_catalogs = ["api"]
```

The exclusion applies only to root fanout. `effigy api/test` remains available.
Plans report excluded aliases and warn about nested Cargo targets that may run
overlapping tests.

## Migration

For a legacy task:

```toml
[tasks]
test = "cargo nextest run --workspace"
```

move the command to:

```toml
[test.suites]
rust = "cargo nextest run --workspace"
```

`effigy tasks migrate` maps a package-manager `test` script to a named test
suite instead of creating `tasks.test`. Manifest rejection and doctor output
must name this migration directly.

## Ownership

- `effigy-manifest`: `[test]` grammar and rejection of `tasks.test`
- `effigy-tasks`: ecosystem detection and suite labels
- `effigy-builtin`: selection, planning, execution, migration, rendering
- `effigy-routing` and root runner: built-in selector delivery without a
  competing manifest-task path
- `effigy-doctor`: manifest failure presentation and migration remediation

## Validation

- a legacy `tasks.test` manifest fails with the v0.11 migration message
- `effigy test --plan` cannot create execution markers
- mixed Rust and TypeScript roots plan and run both supported ecosystems
- named suites, catalog targeting, suite targeting, lifecycle steps, task
  references, Rhai, and passthrough retain focused coverage
- on-demand suites stay selectable without joining the default board
- root exclusions preserve direct catalog selection and plan visibility
- migration preview/apply writes `[test.suites]`, never `tasks.test`
- text and JSON plan contracts remain aligned

## Drift Triggers

- any manifest task can shadow `test`
- `--plan` reaches an execution pipeline
- a second test configuration authority appears
- mixed-root detection stops returning every supported ecosystem
- migration or starter guidance emits `tasks.test`
- a suite task reference changes cwd by rebuilding repository context and loses
  an already loaded ancestor execution registry

## Next Task

Card `1100` is complete. Preserve this routing boundary while the downstream
Acowtancy owner revalidates and removes its workaround separately.
