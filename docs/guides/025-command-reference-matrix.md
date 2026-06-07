# 025 - Command Reference Matrix

This is the fast lookup page for Effigy commands, key flags, JSON schemas, and
the right deeper guide.

## Start Here

Use this page when you already know roughly what you want to do and need the
right command shape fast.

If you do not know where to start yet, use these first:

```sh
effigy tasks
effigy tasks --resolve test
effigy doctor --verbose
effigy test --plan
effigy --json tasks
```

For narrative workflow guidance instead of lookup, start with:

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)

## How To Use This Reference

- use `Pick The Right Command Quickly` when you need the right command family
- use `Primary Commands` when you need one-screen command lookup
- use `Command Shapes` when you need exact invocation form
- use `Scope Notes and Constraints` when a surface has limits or surprising rules
- use `Common Recipes` when you want a few known-good combinations

## Pick The Right Command Quickly

- Need to discover tasks or inspect routing: use `effigy tasks`.
- Need health checks or routing diagnosis for one selector: use `effigy doctor`.
- Need tests, watch mode, repo init, or package-script import: use
  `effigy test`, `watch`, `init`, or `tasks migrate`.
- Need to inspect bundled service fragments or take local ownership: use
  `effigy service`.
- Need one ad-hoc command inside the dev container without opening a shell:
  use `effigy exec`.
- Need to inspect or manage local vault secrets without printing values: use
  `effigy secrets list`, `doctor`, `init`, `set`, `import`, `unset`, `unlock`, or `lock`.
- Need host-local DNS, route status, or TLS setup for container domains: use
  `effigy gateway`.
- Need machine-readable output: add top-level `effigy --json <command>` (or
  task-local `--json` where the command supports it; see
  [`017-json-output-contracts.md`](./017-json-output-contracts.md)).
- Need repo-health scanners: use `effigy scan`.
- Need to clone or update a repo and run its declared bring-up path: use
  `effigy bootstrap`.
- Need to run the configured `[defer]` fallback even when the selector exists
  locally: use `effigy defer`.
- Need proof/demo discovery, inspection, or one-off proof execution: use
  `effigy demo`.
- Need a host-clean local web/service environment or data lifecycle tools: use
  `effigy container`.
- Need substrate lifecycle (VM + compose + gateway + workspace handoff) for the
  repo's declared system: use `effigy system` and `effigy workspace`.
- Need to inspect or refresh a local, git, or OCI bundle source: use
  `effigy bundle`.
- Need a provider-neutral production model derived from the effective manifest
  and bundle: use `effigy deploy model --json`.
- Need the v0.6.0 UAT/production deployment transaction surface: use
  `effigy deploy plan <env>` before `effigy deploy apply <env> --yes`.
- Need a bounded machine-readable repo map before broad source scanning: use
  `effigy graph` (`explore` first, `affected` after edits, `context`/`search`
  for follow-up).
- Need release workflows: use `effigy release`.
- Need distribution validation, GLIBC checks, artifact validation, or
  proof evidence: use `effigy release`.
- Need to inspect the planned schema/seed/import/capture layer order for a
  repo state stack: use `effigy state plan`.

## Primary Commands

| Command | Purpose | Key Flags | JSON Schema(s) | Deep Dive |
| --- | --- | --- | --- | --- |
| `effigy help` / `effigy --help` | Show CLI help and topic guidance | `--json` | `effigy.help.v1` (inside command envelope) | `021-quick-start-and-command-cookbook.md` |
| `effigy version` / `effigy --version` | Print the current Effigy version and active local build identity | `--json` | `effigy.version.v1` (inside command envelope) | `021-quick-start-and-command-cookbook.md` |
| `effigy tasks` | List discovered catalogs/tasks, probe routing, or inspect repo-scoped task status | `status <SELECTOR>`, `status --all`, `--repo`, `--task`, `--resolve`, `--json`, `--pretty true\|false` | `effigy.tasks.v1`, `effigy.tasks.filtered.v1`, `effigy.tasks-status.v1`, `effigy.tasks-status-all.v1` | `016-task-routing-precedence.md` |
| `effigy defer` | Run the configured `[defer]` fallback explicitly (same routing container semantics as selector-miss deferral) | `--repo`, `--json` | command envelope; payload follows the deferred execution path | `015-deferral-fallback-migration.md` |
| `effigy service` | Inspect the layered service catalog and extract bundled fragments into repo-owned overrides | `list`, `extract`, `--repo`, `--dir`, `--json` | service commands render command-envelope JSON with catalog payloads | `063-container-system-guide.md` |
| `effigy exec` | Run one ad-hoc command inside the manifest's default system workspace container | `--repo`, `--service`, `--json` | exec commands render command-envelope JSON with exec payloads | `063-container-system-guide.md` |
| `effigy secrets` | Inspect declared secret metadata, store and retrieve vault values, import declared keys from a `.env`-style file, and manage the local encrypted vault without printing values | `list`, `doctor`, `init`, `set`, `get`, `unset`, `import`, `change-passphrase`, `unlock`, `lock`, `export`, `--repo`, `--json` | `effigy.secrets.v1` | `075-secrets-and-vault-guide.md`, [`../contracts/032-secret-and-local-config-management-contract.md`](../contracts/032-secret-and-local-config-management-contract.md) |
| `effigy gateway` | Operate the host-native local DNS and reverse-proxy gateway for container-owned routes | `up`, `down`, `status`, `setup-tls`, `--json` | gateway commands render command-envelope JSON with gateway payloads | `063-container-system-guide.md` |
| `effigy doctor` | Run health checks and optional explain-mode selection diagnostics | `--repo`, `--fix`, `--verbose`, `--json` | `effigy.doctor.v1`, `effigy.doctor.explain.v1` | `018-doctor-explain-mode.md` |
| `effigy docs` | Run reusable docs QA checks such as path presence, link validation, heading/content/forbidden-text checks, JSON example validation, markdown index consistency checks, next-action policy validation, workflow-path validation, and log-index entry insertion | `check <KIND>`, `add-log-index`, `--repo`, `--file`, `--section`, `--min-blocks`, `--require`, `--require-heading`, `--require-block`, `--forbid`, `--policy-index`, `--policy`, `--dir`, `--index`, `--json` | `effigy.docs.link-check.v1`, `effigy.docs.json-examples.v1`, `effigy.docs.heading-check.v1`, `effigy.docs.path-check.v1`, `effigy.docs.contains-check.v1`, `effigy.docs.forbidden-check.v1`, `effigy.docs.index-check.v1`, `effigy.docs.next-action-check.v1`, `effigy.docs.workflow-path-check.v1`, `effigy.docs.add-log-index.v1` | `029-docs-qa-checklist-and-validation.md` |
| `effigy contracts` | Validate reusable JSON contract artifacts such as selection payloads and schema-index contract coverage | `check-json`, `validate-selection`, `--repo`, `--index`, `--fast`, `--full`, `--changed-only`, `--print-selected`, `--contract`, `--artifact`, `--json` | `effigy.contracts.check-json.v1`, `effigy.contracts.selection-validation.v1` | `017-json-output-contracts.md` |
| `effigy release` | Inspect release readiness, run gates, prepare/execute releases, verify installs, run release preflight, check binary floors, capture proof evidence, validate artifacts, and generate closeout evidence | `status`, `gates`, `resume`, `verify-install`, `preflight`, `validate`, `check-binary`, `proof`, `evidence validate`, `evidence closeout`, `evidence summary`, `simulate`, `prepare`, `execute`, `--repo`, `--tag`, `--skip-docs`, `--skip-smoke`, `--skip-homebrew`, `--artifacts-dir`, `--crate-version`, `--repo-url`, `--brew-formula`, `--output`, `--owner`, `--expect-homebrew`, `--homebrew-executed`, `--log-file`, `--json` | `effigy.release.status.v1`, `effigy.release.gates.v1`, `effigy.release.verify-install.v1`, `effigy.distribution.preflight.v1`, `effigy.distribution.metadata.v1`, `effigy.distribution.artifacts.v1`, `effigy.distribution.closeout.v1`, `effigy.distribution.summary.v1` | `051-release-orchestration.md`, `062-distribution-system-guide.md` |
| `effigy container` | Operate manifest-defined local container environments across Colima/containerd or Docker, along with data lifecycle, cleanup surfaces, shared-service reuse, and cross-project status views | `up`, `down`, `status`, `stats`, `logs`, `shell`, `data`, `reset`, `eject`, `volume`, `cache`, `--repo`, `--attach`, `--detach`, `--service`, `--command`, `--follow`, `--global`, `--dormant`, `--orphans`, `--project`, `--kind`, `--db-seed`, `--db-dump`, `--no-prompt`, `--push`, `--keep-data`, `--yes`, `--json` | `effigy.container.up.v1`, `effigy.container.down.v1`, `effigy.container.status.v1`, `effigy.container.logs.v1` | `063-container-system-guide.md` |
| `effigy system` | Operate the manifest's declared default system substrate (VM + compose + gateway) with lifecycle, log streaming, and recovery surfaces | `up`, `down`, `status`, `logs`, `repair`, `reset-runtime`, `--system`, `--repo`, `--follow`, `--json` | `effigy.system.recover.v1` | `064-system-workspace-and-dev-contract.md` |
| `effigy workspace` | Ensure the selected system is up and then open the resolved workspace shell for the repo's declared developer surface | `<WORKSPACE>`, `--system`, `--repo` | (interactive; no JSON payload) | `064-system-workspace-and-dev-contract.md` |
| `effigy bundle` | Inspect the active repo bundle source and refresh repo-local git/OCI bundle sources | `inspect`, `sync`, `--repo`, `--json` | `effigy.bundle.inspect.v1`, `effigy.bundle.sync.v1` | `065-external-bundle-adoption.md` |
| `effigy deploy` | Derive a provider-neutral production deployment model, export bounded provider files through configured provider packages, and run provider-neutral deployment transactions with state, artifact, release, hook, health, and report evidence | `model`, `export <PROVIDER>`, `plan`, `apply`, `status`, `history`, `redeploy`, `--repo`, `--path`, `--write-report`, `--deployment`, `--yes`, `--json` | `deploy.model.v1`, `effigy.deploy.export.v1`, `effigy.deploy.plan.v1`, `effigy.deploy.apply.v1`, `effigy.deploy.status.v1`, `effigy.deploy.history.v1` | `074-deployment-guide.md`, [`../contracts/002-production-deployment-model.md`](../contracts/002-production-deployment-model.md), [`../contracts/019-deployment-transaction-system-contract.md`](../contracts/019-deployment-transaction-system-contract.md) |
| `effigy graph` | Build, query, and keep the local code graph fresh for file, symbol, edge, impact, changed-file validation narrowing, bounded context packs, one-call exploration, and watcher-driven agent lookup | `index`, `status`, `search`, `files`, `node`, `callers`, `callees`, `impact`, `affected`, `context`, `explore`, `watch`, `--repo`, `--json`, `--limit`, `--depth`, `--stdin`, `--max-files`, `--max-bytes`, `--language`, `--path`, `--debounce-ms` | `effigy.graph.index.v1`, `effigy.graph.status.v1`, `effigy.graph.search.v1`, `effigy.graph.files.v1`, `effigy.graph.node.v1`, `effigy.graph.callers.v1`, `effigy.graph.callees.v1`, `effigy.graph.impact.v1`, `effigy.graph.affected.v1`, `effigy.graph.context.v1`, `effigy.graph.explore.v1`, `effigy.graph.watch.event.v1` | `076-code-graph-and-agent-workflows.md` |
| `effigy rhai` | Inspect the registered Rhai host API surface available to scripts, including module/function names and side-effect posture | `surface`, `--json` | `effigy.rhai.surface.v1` | `061-rhai-script-steps-guide.md`, `068-rhai-host-surface-audit.md` |
| `effigy bootstrap` | Clone or update a repo from a git URL, apply its root bootstrap contract, sync optional submodules, bring along child repos, run setup, optionally stage DB seed dumps and run the standard `bootstrap:db-seed` task, optionally prompt for missing bundle DB dumps on a real TTY, optionally isolate generated-compose runtime state with `--fresh`, optionally pin this bootstrap session to `containerd` or `docker` with `--backend`, run `[bootstrap].start` after setup by default (`--no-start` to skip), and expose `bootstrap deps sync`, `bootstrap children status/sync`, and `bootstrap teardown` for typed dependency hydration, child checkout inspection/refresh, and fresh-session cleanup | `<git-url>`, `teardown`, `deps sync`, `children status`, `children sync`, `--path`, `--branch`, `--backend <containerd|docker>`, `--db-seed <FILE|OCI>|<TARGET>=<FILE|OCI>`, `--fresh`, `--no-prompt`, `--reuse-path`, `--no-start`, `--start`, `--plan`, `--yes`, `--js-only`, `--rust-only`, `--fetch-only`, `--checkout`, `--json` | `effigy.bootstrap.v1`, `effigy.bootstrap.deps.v1`, `effigy.bootstrap.children-status.v1`, `effigy.bootstrap.children-sync.v1`, `effigy.bootstrap-teardown.v1` | `057-bootstrap-repo-bringup.md` |
| `effigy demo` | Discover repo-owned proof demos, browse them in the demo browser, inspect active/latest state, query retained attempt history, execute new attempts, and control runner-owned lifecycle for active demos | `list`, `browser`, `inspect`, `history`, `run`, `stop`, `input`, `resize`, `rerun`, `--repo`, `--json` | `effigy.demo.list.v1`, `effigy.demo.inspect.v1`, `effigy.demo.history.v1`, `effigy.demo.run.v1`, `effigy.demo.stop.v1`, `effigy.demo.input.v1`, `effigy.demo.resize.v1`, `effigy.demo.rerun.v1` | `058-demo-system-guide.md` |
| `effigy scan` | Run built-in repo scanners such as oversized code-file detection, graph-aware boundary drift checks, likely dead-code review, validation-gap review, duplicate-block detection, comment-ratio detection, bulky generated-asset detection, generated-in-src detection, attention-marker detection, and stale-suppression detection | `god-files`, `boundary-violations`, `dead-code`, `validation-gaps`, `duplicate-blocks`, `comment-ratio`, `generated-assets`, `generated-in-src`, `attention-markers`, `stale-suppressions`, `--graph-context`, `--path`, `--stdin`, `--threshold/--warn`, `--high`, `--critical`, `--include`, `--exclude`, `--source-root`, marker overrides (`--warning-marker/--high-marker/--critical-marker`), `--show-warnings`, `--no-gitignore`, `--fail-on-findings`, `--markdown`, `--out`, `--json` | `effigy.scan.god-files.v1`, `effigy.scan.boundary-violations.v1`, `effigy.scan.dead-code.v1`, `effigy.scan.validation-gaps.v1`, `effigy.scan.duplicate-blocks.v1`, `effigy.scan.comment-ratio.v1`, `effigy.scan.generated-assets.v1`, `effigy.scan.generated-in-src.v1`, `effigy.scan.attention-markers.v1`, `effigy.scan.stale-suppressions.v1` | `076-code-graph-and-agent-workflows.md`, `022-manifest-cookbook.md` |
| `effigy test` | Run built-in or explicit `tasks.test` test orchestration | `--plan`, `--verbose-results`, `--tui`, `--json` | `effigy.test.plan.v1`, `effigy.test.results.v1` | `013-testing-orchestration.md` |
| `effigy watch` | Policy-first file-triggered reruns for a target task | `--owner`, `--debounce-ms`, `--include`, `--exclude`, `--once`, `--max-runs`, `--json` | `effigy.watch.v1` (bounded JSON runs) | `019-watch-init-migrate-foundation.md` |
| `effigy init` | Prepare repo setup through one front door: bounded TTY wizard for plain terminal use, baseline managed setup for deterministic apply/check/repair, wider setup inventory via checklist mode, or explicit named starter emission when requested | `--check`, `--apply`, `--repair`, `--checklist`, `--apply-actions`, `<name>`, `--list`, `--dry-run`, `--force`, `--json` | `effigy.init.v1`, `effigy.init.checklist.v1`, `effigy.init.actions.v1`, `effigy.init.list.v1` | `019-watch-init-migrate-foundation.md` |
| `effigy tasks migrate` | Import `package.json` scripts into `[tasks]` | `--from`, `--script`, `--apply`, `--json` | `effigy.migrate.v1` | `019-watch-init-migrate-foundation.md` |
| `effigy config` | Render config reference/schema snippets, inspect the effective composed manifest, or manage user-global container defaults | `inspect`, `schema`, `path`, `get`, `set`, `unset`, `--inspect`, `--path`, `--schema`, `--minimal`, `--target`, `--runner`, `--user-inspect`, `--json` | `effigy.config.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy tasks unlock` | Clear lock scopes manually | `--all`, `--yes`, `--json` | `effigy.unlock.v1` | `020-dag-lock-policy-baseline.md` |
| `effigy artifact` | Inspect, stage, capture, and push versioned data artifacts to OCI registries or local staging | `inspect`, `stage`, `capture`, `--ref`, `--kind`, `--environment`, `--push`, `--farmyard-handoff`, `--json` | `effigy.artifact.inspect.v1`, `effigy.artifact.stage.v1`, `effigy.artifact.capture.v1` | `072-artifact-commands-guide.md` |
| `effigy tasks cache` | Inspect and invalidate phase-1 cache metadata | `inspect`, `invalidate`, `--all`, `--json` | `effigy.cache.v1` | `022-manifest-cookbook.md` |
| `effigy config completion` | Prompt for shell completion setup on a real TTY, export raw shell completion scripts, install user-local completion files, wire bash/zsh startup automatically when needed, and surface selector candidates | `bash\|zsh\|fish`, `--install`, `--export`, `candidates`, `--repo`, `--prefix`, `--json` | `effigy.completion.v2`, `effigy.completion.candidates.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy changelog` | Validate, format, analyze, and extract Northstar changelog content | `validate`, `format`, `analyze`, `extract`, `--repo`, `--write`, `--preview`, `--version`, `--json` | changelog subcommands render direct output; some results can be wrapped in `effigy.command.v1` with global JSON mode | `052-changelog-workflows-and-northstar-profile.md` |
| `effigy state` | Plan, apply, capture, and inspect layered state-stack reports without moving app semantics into Effigy | `plan [<STACK>]`, `plan --manifest <PATH>`, `plan --stack <NAME>`, `apply [<STACK>]`, `capture <STACK> <PROFILE>`, `capture --role ... --source-env ... --key ...`, `history [<STACK>]`, `--write-report`, `--yes`, `--push`, `--repo`, `--json` | `effigy.state-stack.lineage.v1`, `effigy.state-stack.apply.v1`, `effigy.state-stack.capture.v1`, `effigy.state-stack.history.v1` | `073-state-stack-guide.md`, [`../contracts/016-state-stack-and-layered-seed-framework-contract.md`](../contracts/016-state-stack-and-layered-seed-framework-contract.md) |
| `effigy <task>` / `effigy <catalog>/<task>` | Run manifest-defined tasks with routing rules | leading `--repo`, `--verbose-root`, `--env-schema`; passthrough args; task-local `--json` where supported | `effigy.task.run.v1` | `022-manifest-cookbook.md`, `050-env-schema-integration.md` |

## JSON Envelope

For sample payloads per schema, see [`026-json-payload-examples.md`](./026-json-payload-examples.md).

Canonical JSON mode:

```sh
effigy --json <command>
```

All command JSON responses are wrapped in:
- envelope schema: `effigy.command.v1`
- command-specific payload in `result` (or `error.details` for some failures)

See [`017-json-output-contracts.md`](./017-json-output-contracts.md) for envelope and payload details.

## Command Shapes

This page is a lookup surface, not the full generated help output. Use these
family shapes first, then `effigy <command> --help` for the exhaustive flag
set.

### Core Discovery and Diagnostics

```sh
effigy version [--json]
effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>] [--json] [--pretty true|false]
effigy tasks status <SELECTOR> [--repo <PATH>] [--json]
effigy tasks status --all [--repo <PATH>] [--json]
effigy defer [--repo <PATH>] [--json] <REQUEST> [args...]
effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]
effigy doctor <task> <args> [--json]
effigy secrets list [--repo <PATH>] [--json]
effigy secrets doctor [--repo <PATH>] [--json]
effigy secrets init [--repo <PATH>] [--json]
effigy secrets get <NAME> [--repo <PATH>] [--json]
effigy secrets set <NAME> [--repo <PATH>] [--json]
effigy secrets import [<PATH>] [--repo <PATH>] [--json]
effigy secrets unset <NAME> [--repo <PATH>] [--json]
effigy secrets change-passphrase [--repo <PATH>] [--json]
effigy secrets export --format env --output <PATH> --yes [--repo <PATH>] [--json]
effigy config path [--json]
effigy config get <containers.backend|containers.profile> [--json]
effigy config set <containers.backend|containers.profile> <value> [--json]
effigy config unset <containers.backend|containers.profile> [--json]
effigy config --inspect [--path <dotted.path>] [--json]
effigy config --schema [--minimal] [--target <manifest|section>] [--runner <runner>] [--json]
effigy config --user-inspect [--json]
```

### Docs, Contracts, and Scans

```sh
effigy docs check links [--repo <PATH>] [<FILE>...] [--json]
effigy docs check json-examples [--repo <PATH>] [--file <PATH>] [--section <TITLE>] [--min-blocks <N>] [--require <TEXT>]... [--require-block <N:TEXT>]... [--json]
effigy docs check headings [--repo <PATH>] <FILE>... --require-heading <TEXT>... [--json]
effigy docs check paths [--repo <PATH>] <PATH>... [--json]
effigy docs check contains [--repo <PATH>] <FILE>... --require <TEXT>... [--json]
effigy docs check forbidden [--repo <PATH>] <FILE>... --forbid <TEXT>... [--json]
effigy docs check index [--repo <PATH>] [--policy-index <NAME>] [--dir <PATH>] [--index <PATH>] [--json]
effigy docs check next-action [--repo <PATH>] [--policy <NAME>] [--json]
effigy docs check workflow-paths [--repo <PATH>] [--dir <PATH>] [--json]
effigy docs add-log-index [--repo <PATH>] <LOG_FILE> [--json]
effigy contracts check-json [--repo <PATH>] [--index <PATH>] [--fast|--full] [--changed-only <BASE>] [--print-selected|--print-selected=json] [--json]
effigy contracts validate-selection [--repo <PATH>] [--contract <PATH>] [--artifact <PATH>] [--json]
effigy scan <subcommand> [options]
effigy scan <subcommand> [--markdown] [--out <PATH>] [--json]
```

Common values:

- docs check kinds: `links`, `json-examples`, `headings`, `paths`,
  `contains`, `forbidden`, `index`, `next-action`, `workflow-paths`
- scanners: `god-files`, `boundary-violations`, `dead-code`,
  `validation-gaps`, `duplicate-blocks`, `comment-ratio`,
  `generated-assets`, `generated-in-src`, `attention-markers`,
  `stale-suppressions`

### Local Runtime and Services

```sh
effigy service list [--repo <PATH>] [--json]
effigy service extract <SERVICE> [--repo <PATH>] [--dir <PATH>] [--json]
effigy exec [--repo <PATH>] [--service <NAME>] [--json] <COMMAND> [ARGS...]
effigy gateway <up|down|status|setup-tls> [--json]
effigy container up [--repo <PATH>] [--attach|--detach] [--json]
effigy container <NAME> up [--repo <PATH>] [--attach|--detach] [--json]
effigy container down [--repo <PATH>] [--json]
effigy container down --global [--json]
effigy container status [--repo <PATH>] [--json]
effigy container status --global [--json]
effigy container stats --global [--json]
effigy container volume list [--repo <PATH>] [--dormant] [--json]
effigy container volume list --global [--orphans] [--json]
effigy container volume prune [--repo <PATH>] --dormant [--yes] [--json]
effigy container volume prune --global --orphans [--yes] [--json]
effigy container profile status [--profile <NAME>] [--json]
effigy container profile recreate [--profile <NAME>] [--yes] [--json]
effigy container cache list [--repo <PATH>] [--global] [--project <NAME>] [--kind <KIND>] [--json]
effigy container cache prune [--repo <PATH>] [--global] [--project <NAME>] [--kind <KIND>] [--yes] [--json]
effigy container data list [--repo <PATH>] [--json]
effigy container data export <VOLUME> <PATH> [--repo <PATH>] [--json]
effigy container [<NAME>] data dump [<FILE|OCI>|<TARGET>|<TARGET>=<FILE|OCI>]... [--db-dump <FILE|OCI>|<TARGET>|<TARGET>=<FILE|OCI>]... [--push] [--repo <PATH>] [--json]
effigy container data import <VOLUME> <PATH> [--repo <PATH>] [--yes] [--json]
effigy container data pull-production [--repo <PATH>] [--yes] [--json]
effigy container data seed [--db-seed <FILE|OCI>|<TARGET>=<FILE|OCI>]... [--no-prompt] [--yes] [--repo <PATH>] [--json]
effigy container <NAME> logs [--repo <PATH>] [--service <NAME>] [--follow] [--json]
effigy container <NAME> shell [--repo <PATH>] [--service <NAME>] [--command <CMD>]
effigy container <NAME> reset [--repo <PATH>] [--keep-data] [--json]
effigy container <NAME> eject [--repo <PATH>] [--json]
effigy system <up|down|status|logs|repair|reset-runtime> [--system <NAME>] [--repo <PATH>] [--follow] [--json]
effigy workspace [<NAME>] [--system <NAME>] [--repo <PATH>]
```

`effigy container data seed` currently targets the repo default container only
and stays on the generated-compose path.
`effigy container cache list` inventories purge-safe isolated build caches such
as Rust `target`, `node_modules`, `pnpm-store`, and Cargo caches. `--global`
uses machine-level runtime inventory, including stopped project caches where the
runtime metadata allows it.
`effigy container volume list` inventories Effigy-managed named volumes. `--dormant`
shows repo-scoped superseded volumes; `--global` shows machine-level volumes
across available runtimes and `--orphans` narrows that global view to ownerless
volumes.
`effigy container data dump` exports logical SQL dumps from generated-compose
database services; `data export` still exports raw named-volume archives.
Use `[data.targets.<name>]` when a sidecar DB should participate in
bootstrap/data seed/data dump without becoming part of `[bundle].databases`.

### Bundles, Bootstrap, and Demos

```sh
effigy bundle inspect [--repo <PATH>] [--json]
effigy bundle sync [--json]
effigy deploy model [--repo <PATH>] --json
effigy deploy export <PROVIDER> [--repo <PATH>] --path <DIR> [--plan] [--json]
effigy deploy plan <ENV> [--repo <PATH>] [--write-report] [--json]
effigy deploy apply <ENV> [--repo <PATH>] --yes [--json]
effigy deploy status <ENV> [--repo <PATH>] [--json]
effigy deploy history <ENV> [--repo <PATH>] [--limit <N>] [--json]
effigy deploy redeploy <ENV> [--repo <PATH>] --deployment <ID> --yes [--json]
effigy graph index [--repo <PATH>] [--json]
effigy graph status [--repo <PATH>] [--json]
effigy graph search <QUERY> [--repo <PATH>] [--limit <N>] [--json]
effigy graph files [--repo <PATH>] [--limit <N>] [--json]
effigy graph node <ID> [--repo <PATH>] [--json]
effigy graph callers <ID> [--repo <PATH>] [--limit <N>] [--json]
effigy graph callees <ID> [--repo <PATH>] [--limit <N>] [--json]
effigy graph impact <TARGET> [--repo <PATH>] [--limit <N>] [--json]
effigy graph affected [--repo <PATH>] [--depth <N>] [--limit <N>] [--stdin] <PATH>... [--json]
effigy graph context <REQUEST> [--repo <PATH>] [--max-files <N>] [--max-bytes <N>] [--language <ID>]... [--path <PREFIX>]... [--json]
effigy graph explore <REQUEST> [--repo <PATH>] [--max-files <N>] [--max-bytes <N>] [--language <ID>]... [--path <PREFIX>]... [--json]
effigy graph watch [--repo <PATH>] [--debounce-ms <MS>] [--json]
effigy bootstrap <GIT_URL> [--path <DIR>] [--branch <NAME>] [--backend <containerd|docker>] [--db-seed <FILE|OCI>|<TARGET>=<FILE|OCI>]... [--fresh] [--no-prompt] [--reuse-path] [--no-start] [--plan] [--json]
effigy bootstrap teardown [--yes] [--json]
effigy bootstrap deps sync [<path>...] [--js-only|--rust-only] [--json]
effigy bootstrap children status [--json]
effigy bootstrap children sync [--fetch-only] [--checkout] [--json]
effigy demo <list|browser|inspect|history|run|stop|input|resize|rerun> [ARGS...] [--json]
```

`effigy graph watch --json` is the graph JSON streaming exception: it emits
newline-delimited `effigy.graph.watch.event.v1` payloads directly instead of a
single `effigy.command.v1` envelope.

### Testing and Project Workflow

```sh
effigy test [--plan] [--verbose-results] [--tui] [suite] [runner args]
effigy watch --owner <effigy|external> [--debounce-ms <MS>] [--include <GLOB>] [--exclude <GLOB>] <task> [task args]
effigy init [--check|--apply|--repair] [--json]
effigy init --checklist [--json]
effigy init --apply-actions <ID>[,<ID>...] [--json]
effigy init <name> [--dry-run] [--force] [--json]
effigy init --list [--json]
effigy tasks migrate [--from <PATH>] [--script <NAME>]... [--apply] [--json]
effigy <task> [--repo <PATH>] [--verbose-root] [--env-schema <PATH>] [task args]
effigy <catalog>/<task> [--repo <PATH>] [--verbose-root] [--env-schema <PATH>] [task args]
```

Use `effigy scan <subcommand> --help` (and per-scanner help) for optional scan
flags such as `--include`, `--exclude`, `--threshold`, `--source-root`, marker
overrides, and `--no-gitignore`.

### Cache, Completion, Changelog, and Release

```sh
effigy tasks unlock [--all | <scope>...] [--yes] [--json]
effigy tasks cache inspect [<selector>] [--json]
effigy tasks cache invalidate [<selector>...] [--all] [--json]
effigy config completion [<bash|zsh|fish>] [--install|--export] [--json]
effigy config completion candidates [--repo <PATH>] [--prefix <value>] [--json]
effigy changelog <validate|format|analyze|extract> [--repo <PATH>] [ARGS...] [--json]
effigy release <status|gates|resume|simulate|prepare|execute|verify-install|preflight|validate|check-binary|proof> [ARGS...] [--json]
effigy release evidence <validate|closeout|summary> [ARGS...] [--json]
effigy state plan [<STACK>] [--repo <PATH>] [--json] [--write-report]
effigy state plan --manifest <PATH> [--repo <PATH>] [--json] [--write-report]
effigy state plan --stack <NAME> [--repo <PATH>] [--json] [--write-report]
effigy state apply [<STACK>] [--yes] [--skip-layer <KEY>]... [--json]
effigy state capture <STACK> <PROFILE> [--yes] [--push] [--json]
effigy state capture-set <STACK> <PROFILE>... [--key <KEY>] [--yes] [--push] [--json]
effigy state capture [<STACK>] --role <ROLE> --source-env <ENV> --key <KEY> [--json]
effigy state capture [<STACK>] --role <ROLE> --source-env <ENV> --key <KEY> --source <PATH> --ref oci://<REF> --yes [--push] [--json]
effigy state history [<STACK>] [--kind plan|apply|capture] [--limit <N>] [--lineage <ID>] [--json]
```

## Scope Notes and Constraints

Use the deeper guides for full surface detail. The main sharp edges here are:

- `tasks --pretty false` is valid only with `--json`
- `watch --owner` is required
- `watch --json` requires bounded mode such as `--once` or `--max-runs`
- `exec` runs inside the manifest default system workspace container and
  defaults to that container's `primary_service` unless `--service` is supplied
- `gateway up`, `gateway down`, and `gateway setup-tls` may request host admin
  approval
- routes with `tls = true` redirect plain HTTP to HTTPS once the gateway TLS
  listener is available
- `container shell` and `workspace` are interactive and intentionally do not
  support `--json`
- `container status --global` and `container stats --global` are cross-project views
- generated-compose data lifecycle stays on the product-owned path and does not
  widen direct `compose_file` ownership
- `system logs` is streaming and intentionally does not support `--json`
- `system repair` and `system reset-runtime` are the recovery surfaces for
  half-up substrate state
- mounted sibling repos listed in `systems.<name>.mounts` auto-adopt
  producer-declared isolation paths into workspace containers
- `bundle inspect` reports the active repo bundle source only; there is no named
  bundle inspection anymore
- `bundle sync` is the explicit refresh path for git and OCI bundle sources in
  the current repo; local path bundle sources report not-applicable
- `deploy model` is intentionally JSON-only and reads the active rendered
  `[deploy.model]` section from the effective manifest
- `deploy export <PROVIDER>` requires a matching
  `[deploy.providers.<provider>]` package with an export capability
- the external Render provider package currently writes `render.yaml`
- the external Railway provider package currently writes service-local
  `railway.toml` files plus `report.json`
- v0.6.0 deployment transactions are separate from `deploy export`:
  `deploy plan/apply/status/history/redeploy` compose code refs, provider
  targets, state stacks, OCI artifact policy, release evidence, hooks, health
  checks, and reports; provider-specific planning, apply, and status behavior
  comes from configured deploy-provider packages rather than built-in core
  provider logic
- `[deploy.providers.<name>]` can resolve path and git deploy-provider packages
  with `provider.toml` descriptors during deploy export and deployment
  transactions; declared `export.rhai`, `preflight.rhai`, `apply.rhai`, and
  `status.rhai` scripts run through `deploy::provider_context()` and report
  through `deploy::provider_report(...)`
- `deploy apply` must validate provider setup and block with
  remediation instead of creating provider projects, services, resources,
  domains, variables, or secrets
- `deploy redeploy` is evidence-backed replay of recorded immutable inputs, not
  automatic database or media rollback
- `bootstrap` runs `[bootstrap].start` after setup by default; pass `--no-start`
  to skip that phase; it fails fast on dirty existing checkouts or remote
  mismatches
- demo surfaces are intentionally bounded:
  - `inspect` reports current declared and retained state
  - `history` is one-demo retained-history review
  - `run` starts a new attempt
  - `stop` only works for runner-owned active attempts
  - `browser` is interactive but must not launch nested TUIs
- task execution locks on `task:<name>` by default; `tasks.<name>.lock` opts
  several tasks into a shared lock scope
- managed `mode = "tui"` tasks also acquire `profile:<task>/<profile>`
- managed `concurrent` entries accept `shutdown_on_exit = true`
- `--verbose-root` and `--env-schema` apply to manifest task invocations; the
  passthrough-style built-ins `doctor`, `watch`, and `scan` reject them on the
  built-in invocation itself (use `effigy <builtin> --help` and
  [`050-env-schema-integration.md`](./050-env-schema-integration.md))
- all scan commands accept either `--json` or `--markdown`, not both
- `scan validation-gaps` accepts changed paths as args or via `--stdin`
- `scan --graph-context` reports graph readiness even when a scan family does
  not enrich findings yet
- scan `--out <PATH>` values resolve relative to the scanned repo root
- `config --minimal` requires `--schema`
- `config --inspect` cannot be combined with `--schema`
- `config --path` requires `--inspect`
- `config --runner` requires `--schema --target test`
- `tasks unlock` accepts either explicit scopes or `--all`, not both
- `tasks cache invalidate` accepts selectors or `--all`, not both
- release operator flows should prefer built-in `effigy release ...` commands,
  not wrapper scripts
- top-level `[env]`, task-local `env`, and run-array env directives all
  participate in task execution; use the env guide for the full fallback and
  indirection rules
- config completion candidates are cached and include both built-ins and
  discovered task selectors
- `init` never replaces an existing **root** `README.md` unless `--force`; other
  declared paths still fail fast when present without `--force`
- `state plan` is plan-only: it validates the manifest, reports ordered lineage,
  and records planned artifact resolutions without running hooks or applying data
- `state plan` without a standalone manifest reads `[state]` from the composed
  Effigy manifest; use `state.default`, positional `<STACK>`, or `--stack
  <NAME>` when multiple stacks are declared
- `state plan --write-report` writes the lineage payload to
  `.effigy/reports/state/<stack>/plan.json`, updates `latest-plan.json`, writes
  a timestamped `history/*.json` entry, and includes written path fields in the
  output
- `state apply` is plan-only unless `--yes` is supplied; `--yes` executes
  `apply_mode = "task"` layers, stages `apply_mode = "artifact"` layers, and
  imports `apply_mode = "sql"` layers through the existing DB seed/import path;
  apply reports update `latest-apply.json` and timestamped history
- `state apply --skip-layer <KEY>` marks a named layer as `skipped` and does
  not execute its task/import/stage step; this is for wrapper workflows that
  already ran a prerequisite layer and still want the canonical stack report
- `state capture` is plan-only unless `--yes --source <PATH> --ref oci://...`
  is supplied; execution stages the already-produced local payload, and `--push`
  explicitly publishes it after local staging; `--task <TASK>` runs one
  repo-owned capture task before staging, and named capture profiles can declare
  `task` as either a selector string or inline run-array task definition, while
  produced-layer apply hooks still do not run; capture reports update
  `latest-capture.json` and timestamped history; capture tasks receive
  `EFFIGY_STATE_CAPTURE_CONTEXT` pointing to a versioned JSON context file, and
  relative `EFFIGY_STATE_CAPTURE_SOURCE` values are resolved to absolute
  task-runtime paths
- `state capture <STACK> <PROFILE>` resolves `[state.<STACK>.captures.<PROFILE>]`
  from the composed manifest; CLI flags can still override profile fields for
  one-off captures
- `state capture-set <STACK> <PROFILE>...` runs multiple named capture profiles
  with one shared key; omit `--key` to let Effigy generate a timestamp key, and
  use `--push` to publish all captured artifacts after local staging; aggregate
  reports update `latest-capture-set.json` and timestamped history alongside
  the normal child capture reports
- `state history` is read-only; it scans report JSON files and ignores malformed
  files with warnings instead of maintaining an index
- `graph index` is explicit; queries do not rebuild the graph for you
- `graph status --json` exposes `freshness.state` (`ready`, `refresh-recommended`,
  `degraded`, `missing-index`) and `freshness.usable`; agents should gate on
  those fields before trusting explore/affected output
- `graph explore` is the preferred one-call agent navigation packet for
  code-understanding work; use `graph context` when you want the lower-level
  ranked item list instead
- `graph affected` accepts changed paths as args or via `--stdin`; it narrows
  likely validation targets but does not prove exhaustive test reachability
- `graph watch --json` streams newline-delimited `effigy.graph.watch.event.v1`
  events and does not use the one-shot `effigy.command.v1` envelope

## Common Recipes

Routing diagnosis:

```sh
effigy tasks --resolve test
effigy doctor --repo /path/to/workspace app/build --watch
```

Test planning and execution:

```sh
effigy test --plan
effigy test vitest
```

CI/JSON mode:

```sh
effigy --json tasks
effigy --json doctor
effigy --json scan god-files
effigy --json scan boundary-violations
effigy --json scan dead-code
git diff --name-only | effigy scan validation-gaps --stdin --json
effigy --json scan duplicate-blocks
effigy --json scan comment-ratio
effigy --json scan generated-in-src
effigy --json scan attention-markers
effigy --json scan stale-suppressions
effigy --json bootstrap git@github.com:inflatable-cookie/loophole.git --plan
effigy --json test --plan
effigy state plan
effigy state plan --write-report
effigy state plan uat
effigy state apply uat
effigy state apply uat --yes
effigy state capture uat new-content --yes
effigy state capture-set legacy-source db media --yes --push
effigy --json state plan ./state/example-app-uat.toml
effigy release simulate
effigy release prepare --plan
effigy release execute --plan
effigy --json release status --check-gates
```

Agent repo map:

```sh
effigy graph index
effigy --json graph status
effigy --json graph explore "trace release orchestrator" --max-files 6 --max-bytes 12288
git diff --name-only | effigy graph affected --stdin --json
```

Lock recovery:

```sh
effigy tasks unlock task:watch:test
effigy tasks unlock --all --yes
```

## Related Guides

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`055-everyday-workflows.md`](./055-everyday-workflows.md)
- [`057-bootstrap-repo-bringup.md`](./057-bootstrap-repo-bringup.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`051-release-orchestration.md`](./051-release-orchestration.md)
- [`052-changelog-workflows-and-northstar-profile.md`](./052-changelog-workflows-and-northstar-profile.md)
- [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)
- [`072-artifact-commands-guide.md`](./072-artifact-commands-guide.md)
- [`076-code-graph-and-agent-workflows.md`](./076-code-graph-and-agent-workflows.md)

## Expected Outcome

After this guide, you should be able to:

- choose the right Effigy command without scanning multiple docs first
- confirm the key flags and JSON schema for a command quickly
- jump from the reference surface to the deeper workflow page only when needed

## Next Step

After using this matrix to find the right command, move to the matching workflow
guide and simplify the corresponding repo path so people can rely on that
command directly instead of a local wrapper or tribal workaround.
