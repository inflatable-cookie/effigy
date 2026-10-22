# Built-In Surfaces

Lookup for Effigy **built-ins** (not manifest task names). Run `effigy --help`
for the live list, grouped by job. Use `effigy <topic> --help` for flags.

`effigy help <group>` narrows the live list to one job: `work`, `local`, `repo`,
`deliver`, `extend`, or `admin`. `effigy help <command>` renders the same panel
as `effigy <command> --help`. Grouping is discovery only — there is no
`effigy <group> <command>` route, and group words stay available as manifest
selectors.

## Default agent loop

| Built-in | First use |
|----------|-----------|
| `doctor` | Health + routing on entry |
| `tasks` | Task inventory |
| `test` | Test plan (`--plan`) and execution |
| `<selector>` | Repo work the manifest defines |

Details: `agent-operating-loop.md`.

## Manifest selector runtime

Managed selectors are repo tasks, not built-ins. A task with `mode = "tui"`
also supports `--headless` / `EFFIGY_MANAGED_HEADLESS=1` plus task-local
`status`, `logs [process] [--follow]`, and `stop` companions.

## Discovery and execution

| Built-in | Purpose |
|----------|---------|
| `tasks` | List catalogs/tasks; `tasks --resolve`; `tasks status` |
| `doctor` | Health checks; `doctor <selector>` explain mode; running-workspace ownership diagnosis |
| `test` | Built-in test orchestration (`--plan`, `--tui`) |
| `defer` | Run `[defer]` fallback explicitly |
| `scan` | Repo scanners (`god-files`, `duplicate-blocks`) |
| `config` | Inspect merged manifest; schema snippets; `config get/set` |
| `config completion` | Shell completion + selector candidates |
| `deps` | Inspect, link, and unlink full Cargo/Bun local dependency closures |

## Documentation navigation

`effigy docs check` validates links, JSON examples, headings, paths, forbidden
text, indexes, next-actions and workflow paths. v0.15 removed `effigy docs
context`; use `docs/README.md` as the docs index and `rg` for exact text.

## Local runtime

| Built-in | Purpose |
|----------|---------|
| `container` | Compose/colima/docker lifecycle, data, cache, volume |
| `system` | Default system substrate (VM + compose + gateway) |
| `workspace` | System up + dev shell |
| `gateway` | Host DNS/TLS for container routes |
| `service` | Catalog inspect/extract |
| `exec` | One-shot command in dev workspace container; primary-service workspace identity; no TTY for non-console callers |
| `bootstrap` | Clone/bring-up repo from git URL |

## Data, deploy, and proof

| Built-in | Purpose |
|----------|---------|
| `state` | Layered schema/seed/capture stacks |
| `deploy` | Model, export, plan/status/history; apply/redeploy refuse in v0.15 (guide 074 / Q-003) |
| `artifact` | OCI/local data artifacts |
| `bundle` | Bundle source inspect/sync |
| `secrets` | Vault init/set/list/doctor/**import** |

## Release and quality

| Built-in | Purpose |
|----------|---------|
| `release` | Gates, prepare/execute, verify-install, distribution evidence |
| `changelog` | Validate/format/analyze/extract changelog |
| `docs` | Link/JSON/index/workflow-path QA checks |
| `contracts` | JSON contract validation |
| `init` | Repo setup wizard; `--check`, `--checklist`, `--apply` |

## Distribution note

There is **no** top-level `effigy distribution` command. Distribution work lives
under `effigy release validate`, `release preflight`, `release proof`, and
`release evidence …`. Guide: `docs/guides/062-distribution-system-guide.md`.

## JSON mode

```bash
effigy --json <built-in>
```


Details: `json-envelope.md`, `docs/guides/017-json-output-contracts.md`.

## Command matrix

Full flags and schemas: `docs/guides/025-command-reference-matrix.md`
