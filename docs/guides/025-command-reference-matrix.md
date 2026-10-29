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
effigy doctor
effigy test --plan
effigy --json tasks
```

For narrative workflow guidance instead of lookup, start with:

- [`021-quick-start-and-command-cookbook.md`](021-quick-start-and-command-cookbook.md)
- [`055-everyday-workflows.md`](055-everyday-workflows.md)

## How To Use This Reference

- use `Pick The Right Command Quickly` when you need the right command family
- use `Primary Commands` when you need one-screen command lookup
- use `Command Shapes` when you need exact invocation form
- use `Scope Notes and Constraints` when a surface has limits or surprising rules
- use `Common Recipes` when you want a few known-good combinations

## Pick The Right Command Quickly

- Need to discover tasks or inspect routing: use `effigy tasks`.
- Need health checks, workspace ownership diagnosis, or routing diagnosis for
  one selector: use `effigy doctor`.
- Need tests or repository setup: use
  `effigy test` or `effigy init`.
- Need to inspect bundled service fragments or take local ownership: use
  `effigy service`.
- Need one ad-hoc command inside the dev container without opening a shell:
  use `effigy exec`.
- Need to inspect or manage local vault secrets without printing values: use
  `effigy secrets list`, `doctor`, `init`, `set`, `import`, `unset`, `unlock`, or `lock`.
- Need host-local DNS, route status, or TLS setup for container domains: use
  `effigy gateway`.
- Need to inspect a selector without running it: use `effigy <selector> --plan`
  (or `effigy --json <selector> --plan`). `--json` alone still executes; see
  [`017-json-output-contracts.md`](017-json-output-contracts.md).
- Need to recover a run after a restart or a lost stream: start it with
  `effigy --run-id <key> <task>`, then use `effigy run status|logs|stop <key>`;
  a repeat with the same key attaches instead of starting another. See
  [`080-host-wide-validation-admission.md`](080-host-wide-validation-admission.md).
- Need machine-readable output: add top-level `effigy --json <command>` (or
  task-local `--json` where the command supports it; see
  [`017-json-output-contracts.md`](017-json-output-contracts.md)).
- Need repo-health scanners: use `effigy scan`.
- Need to download and start someone else's project — clone or update it,
  materialise its pinned sources, run its `setup` selectors and its `start`
  selector: use `effigy bootstrap`.
- Need to run the configured `[defer]` fallback even when the selector exists
  locally: use `effigy defer`.
- Need a host-clean local web/service environment or data lifecycle tools: use
  `effigy container`.
- Need substrate lifecycle (VM + compose + gateway + workspace handoff) for the
  repo's declared system: use `effigy container` and `effigy workspace`.
- Need to inspect or refresh a local, git, or OCI bundle source: use
  `effigy bundle`.
- Need a provider-neutral production model for one declared environment derived
  from the effective manifest and bundle: use `effigy deploy model <ENV> --json`.
- Need deployment configuration: use `effigy deploy model <ENV> --json` or
  `effigy deploy export <PROVIDER> <ENV> --path <DIR> --plan`. Apply/redeploy
  are not yet supported in v0.15; see [guide 074](074-deployment-guide.md) and
  Q-003.
- Need a bounded machine-readable repo map before broad source scanning: use
  `rg`, then read `docs/README.md` for the owning knowledge.
- Need repository-defined documentation graph semantics: use the optional
  `[docs_policy.graph]` profile documented in
  [`../contracts/041-documentation-graph-profile-contract.md`](../knowledge/contracts/041-documentation-graph-profile-contract.md)
  and the architecture note
  [`../architecture/024-repository-defined-documentation-graph.md`](../knowledge/architecture/024-repository-defined-documentation-graph.md).
- Need release workflows: use `effigy release`.
- Need release validation, GLIBC checks, artifact validation or proof
  evidence: use `effigy release` for the Run, and the repository's own
  `release:` tasks (`config/tasks.toml`) for Effigy's checks.
- Need to inspect the planned schema/seed/import/capture layer order for a
  repo state stack: use `effigy state plan`.
- Need dependencies to resolve from a local Cargo or Bun checkout through
  machine-local links, or through a committed Bun pin: use `effigy deps`.
- Need to run tasks shipped by an installed skill against the current or an
  explicit consumer repo: use `effigy skill tasks` then `effigy skill run`.
- Need documentation authority instead of a manual file hunt: start from
  `docs/README.md` and `rg` the docs tree.

## Help-First Discovery

`effigy --help` and `effigy help` group the built-in surface by operator job.
Grouping is discovery only: it adds no `effigy <group> <command>` route, reserves
no new top-level built-in name, and leaves manifest selectors named `work`,
`local`, `repo`, `deliver`, `extend`, or `admin` on their current routing.

| Topic | Primary commands |
| --- | --- |
| `effigy help work` | `effigy <task>`, `effigy <catalog>/<task>`, `effigy tasks`, `effigy test`, `effigy doctor`, `effigy init` |
| `effigy help local` | `effigy container`, `effigy workspace`, `effigy gateway`, `effigy service`, `effigy exec` |
| `effigy help repo` | `effigy scan`, `effigy docs`, `effigy contracts` |
| `effigy help deliver` | `effigy state` (with `state artifact`), `effigy deploy`, `effigy release`, `effigy bundle`, `effigy bootstrap` |
| `effigy help extend` | `effigy skill`, `effigy rhai surface` |
| `effigy help admin` | `effigy config`, `effigy deps`, `effigy secrets`, `effigy defer`, `effigy uninstall`, `effigy version`, `effigy config completion`, `effigy help` |

`effigy help <command>` renders the same panel as `effigy <command> --help`, so
`effigy help docs` and `effigy docs --help` agree. That holds for every command
in the inventory: `config` and `scan` keep their detailed help inside the
built-in rather than a typed panel, and `effigy help config` / `effigy help scan`
resolve to the same command the direct form runs, so stdout, JSON, and exit
status match exactly. An unknown topic fails with exit code `2` and names the
valid groups and commands; it never falls back silently to general help.

Built-ins deferred to a repository selector stay hidden from general help and
group help. Their detail help defers too: when a manifest task or
`[defer] builtins` entry owns the name, `effigy <command> --help` already routes
to the repository, so `effigy help <command>` refuses the built-in panel and
points back at `effigy <command> --help`. Help never executes repository work,
so a selector named `config` or `scan` gets the same refusal instead of running
through the help route.

## Primary Commands

| Command | Purpose | Key Flags | JSON Schema(s) | Deep Dive |
| --- | --- | --- | --- | --- |
| `effigy help` / `effigy --help` | Show job-grouped CLI help; `effigy help <group>` renders one group and `effigy help <command>` renders command detail | `--json` | `effigy.help.v1` (inside command envelope) | `021-quick-start-and-command-cookbook.md` |
| `effigy version` / `effigy --version` | Print the current Effigy version and active local build identity | `--json` | `effigy.version.v1` (inside command envelope) | `021-quick-start-and-command-cookbook.md` |
| `effigy uninstall` | Plan or remove Effigy-owned local installation state | `--plan`, `--yes`, `--json` | command envelope with cleanup plan/result | `effigy uninstall --help` |
| `effigy tasks` | List public tasks by group or inspect task status; `--all` and JSON include internal tasks, which still run by name | `status <SELECTOR>`, `status --all`, `--all`, `--task`, `--resolve`, `--repo`, `--json`, `--pretty true\|false` | `effigy.tasks.v1`, `effigy.tasks.filtered.v1` with `visibility`, `visibility_reason`, `group`; status schemas `effigy.tasks-status.v3`, `effigy.tasks-status-all.v3` | `016-task-routing-precedence.md`, `022-manifest-cookbook.md` |
| `effigy skill` | List or execute one explicitly selected, isolated skill task catalog while a separate consumer repo owns runtime effects | `tasks`, `run`, `--path`, `--repo`, `--json`, `-- <ARGS>` | `effigy.skill.tasks.v1`, `effigy.skill.run.v1` | `021-quick-start-and-command-cookbook.md`, [`../contracts/042-external-skill-task-runner-contract.md`](../knowledge/contracts/042-external-skill-task-runner-contract.md) |
| `effigy deps` | Inspect dependency state, manage machine-local Cargo and Bun links, and author committed Bun pins | `status [cargo\|bun]`; `link <cargo\|bun> <PATH> [--dry-run]`; `unlink <cargo\|bun> <PATH> [--dry-run]`; `pin bun <PATH> [--dry-run]`; `unpin bun <PATH> [--dry-run]`; `--repo`, `--json` | `effigy.deps.status.v1`, `effigy.deps.link.v1`, `effigy.deps.unlink.v1`, `effigy.deps.pin.v1` | [`077-local-dependency-linking.md`](077-local-dependency-linking.md) |
| `effigy defer` | Run the configured `[defer]` fallback explicitly (same routing container semantics as selector-miss deferral) | `--repo`, `--json` | command envelope; payload follows the deferred execution path | `015-deferral-fallback-migration.md` |
| `effigy service` | Inspect catalog sources and the source of each service, and extract baseline fragments into the project catalog | `list`, `extract`, `--repo`, `--dir`, `--json` | `effigy.service.list.v2`, `effigy.service.extract.v1` | [`067-catalog-services-reference.md`](067-catalog-services-reference.md), `063-container-system-guide.md` |
| `effigy exec` | Run one ad-hoc command inside the manifest's default system workspace container; primary-service commands use its declared `workspace_user` and `workspace_home`, while non-console callers run without a TTY | `--repo`, `--service`, `--json` | exec commands render command-envelope JSON with exec payloads | `063-container-system-guide.md` |
| `effigy secrets` | Inspect declared secret metadata, store and retrieve vault values, import declared keys from a `.env`-style file, and manage the local encrypted vault without printing values | `list`, `doctor`, `init`, `set`, `get`, `unset`, `import`, `change-passphrase`, `unlock`, `lock`, `export`, `--repo`, `--json` | `effigy.secrets.v1` | `075-secrets-and-vault-guide.md`, [`../contracts/032-secret-and-local-config-management-contract.md`](../knowledge/contracts/032-secret-and-local-config-management-contract.md) |
| `effigy gateway` | Operate the host-native local DNS and reverse-proxy gateway for container-owned routes, or select an isolated disposable instance | `up`, `down`, `status`, `setup-tls`, `--private-state-root`, private `--dns-addr` / `--proxy-addr` / `--https-addr`, `--json` | gateway commands render command-envelope JSON with gateway payloads; private `up` and `status` report actual held loopback addresses and `client_ca_file`; the retired `setup-listener-handoff`, `listener-handoff-status` and `remove-listener-handoff` commands refuse with their replacement | `063-container-system-guide.md`, `083-v0.14.0-consumer-migration.md` |
| `effigy doctor` | Run bounded structural checks by default, including `container.workspace-ownership`; opt into catalog-scoped scans and health with `--deep`; provide optional explain-mode selection diagnostics | `--repo`, `--fix`, `--verbose`, `--json`, `--deep`, `--catalog`, `--all-catalogs`, `--refresh` | `effigy.doctor.v1`, `effigy.doctor.explain.v1` | `018-doctor-explain-mode.md`, `063-container-system-guide.md`, [`../contracts/047-bounded-doctor-and-scan-cache-contract.md`](../knowledge/contracts/047-bounded-doctor-and-scan-cache-contract.md) |
| `effigy docs` | Run reusable docs QA checks such as path presence, link validation, heading/content/forbidden-text checks, JSON example validation, markdown index consistency checks, next-action policy validation, workflow-path validation, and log-index entry insertion | `check <KIND>`, `add-log-index`, `--repo`, `--file`, `--section`, `--min-blocks`, `--require`, `--require-heading`, `--require-block`, `--forbid`, `--policy-index`, `--policy`, `--dir`, `--index`, `--json` | `effigy.docs.link-check.v1`, `effigy.docs.json-examples.v1`, `effigy.docs.heading-check.v1`, `effigy.docs.path-check.v1`, `effigy.docs.contains-check.v1`, `effigy.docs.forbidden-check.v1`, `effigy.docs.index-check.v1`, `effigy.docs.next-action-check.v1`, `effigy.docs.workflow-path-check.v1`, `effigy.docs.add-log-index.v1` | `029-docs-qa-checklist-and-validation.md` |
| `effigy contracts` | Validate the project's JSON artifacts and selection payload against its own `[contracts]` schema index and selection contract | `check-json`, `--repo`, `--index`, `--contract`, `--changed-only`, `--print-selected`, `--validate-selection`, `--artifact`, `--json` | `effigy.contracts.check-json.v1` | `017-json-output-contracts.md` |
| `effigy release` | Resolve the release pipeline, run gates, run/resume/approve the checkpointed release Run, record an unknown publish decision, and print the receipt | `--plan`, `status`, `gates`, `run [--to <STAGE>]`, `resume <RUN>`, `approve <RUN>`, `decide <RUN> --published|--not-published`, `receipt [<RUN>]`, `--repo`, `--version`, `--json` | `effigy.release.plan.v1`, `effigy.release.status.v1`, `effigy.release.gates.v1`, `effigy.release.run.v1`, `effigy.release.receipt.v1`, plus the `effigy.release.input.v1` / `effigy.release.output.v1` release-task documents | `051-release-orchestration.md`, [`../knowledge/contracts/054-release-platform-contract.md`](../knowledge/contracts/054-release-platform-contract.md) |
| `effigy container` | Operate manifest-defined local container environments across Colima/containerd or Docker, along with data lifecycle, cleanup surfaces, shared-service reuse, scoped identity and host discovery, and per-scope retirement | `up`, `down`, `status`, `stats`, `logs`, `restart`, `shell`, `data`, `reset`, `eject`, `render`, `hosts`, `scope`, `retire`, `volume`, `cache`, `profile status`, `profile resize`, `profile recreate`, `profile purge`, `profile repair`, `profile reset-runtime`, `--apply`, `--expect-plan`, `--repo`, `--system`, `--attach`, `--detach`, `--service`, `--command`, `--bounded`, `--deadline-ms`, `--stdout-limit-bytes`, `--stderr-limit-bytes`, `--follow`, `--global`, `--dormant`, `--orphans`, `--project`, `--kind`, `--profile`, `--disk`, `--db-seed`, `--db-dump`, `--no-prompt`, `--push`, `--keep-data`, `--scope`, `--yes`, `--json` | `effigy.container.up.v1`, `effigy.container.down.v1`, `effigy.container.status.v1`, `effigy.container.logs.v1`, `effigy.container.restart.v1`, `effigy.container.hosts.v1`, `effigy.container.scope.v1`, `effigy.container.retire.v1`, `effigy.container.bounded-exec.v1`, `effigy.system.recover.v1` (`profile repair` and `profile reset-runtime` keep this envelope) | `063-container-system-guide.md`, `064-system-workspace-and-dev-contract.md` |
| `effigy workspace` | Ensure the selected system is up and then open the resolved workspace shell for the repo's declared developer surface | `<WORKSPACE>`, `--system`, `--repo` | (interactive; no JSON payload) | `064-system-workspace-and-dev-contract.md` |
| `effigy bundle` | Inspect the active repo bundle source and refresh repo-local git/OCI bundle sources | `inspect`, `sync`, `--repo`, `--json` | `effigy.bundle.inspect.v1`, `effigy.bundle.sync.v1` | `065-external-bundle-adoption.md` |
| `effigy deploy` | Derive the provider-neutral deployment model for a declared environment, export provider files, inspect plans/status/history; apply and redeploy refuse as unsupported in v0.15 (Q-003) | `model [<ENV>]`, `export <PROVIDER> [<ENV>]`, `--env`, `plan`, `apply`, `status`, `history`, `redeploy`, `--repo`, `--path`, `--write-report`, `--deployment`, `--yes`, `--json` | `deploy.model.v1`, `effigy.deploy.export.v1`, `effigy.deploy.plan.v1`, `effigy.deploy.apply.v1`, `effigy.deploy.status.v1`, `effigy.deploy.history.v1` | `074-deployment-guide.md`, [`../contracts/002-production-deployment-model.md`](../knowledge/contracts/002-production-deployment-model.md), [`../contracts/019-deployment-transaction-system-contract.md`](../knowledge/contracts/019-deployment-transaction-system-contract.md) |
| `effigy rhai` | Inspect the registered Rhai host API surface available to scripts, including module/function names and side-effect posture | `surface`, `--json` | `effigy.rhai.surface.v1` | `061-rhai-script-steps-guide.md`, `068-rhai-host-surface-audit.md` |
| `effigy bootstrap` | Clone or update a repo from a git URL (or use this checkout with no URL), materialise its pinned bundle, catalog and `[sources.<name>]` sources, initialise submodules when `[bootstrap] submodules = true`, run its `setup` selectors in order as ordinary tasks, then start its `start` selector unless `--no-start` | `[<git-url>]`, `--path`, `--branch`, `--no-start`, `--plan`, `--json` | `effigy.bootstrap.v1` | `057-bootstrap-repo-bringup.md` |
| `effigy scan` | Run the built-in repo scanners `god-files` (oversized code files, per-language size defaults) and `duplicate-blocks` (repeated code spans, opt-in) | `god-files`, `duplicate-blocks`, `--threshold/--warn`, `--high`, `--critical`, `--include`, `--exclude`, `--show-warnings`, `--no-gitignore`, `--fail-on-findings`, `--markdown`, `--out`, `--json` | `effigy.scan.god-files.v1`, `effigy.scan.duplicate-blocks.v1` | `022-manifest-cookbook.md` |
| `effigy test` | Plan or run detected and configured test suites | `--plan`, `--verbose-results`, `--tui`, `--json` | `effigy.test.plan.v1`, `effigy.test.results.v1` | `013-testing-orchestration.md` |
| `effigy init` | Prepare repo setup through one front door: bounded TTY wizard for plain terminal use, baseline managed setup without a project skill copy, wider setup inventory via checklist mode, explicit optional skill snapshot action, or named starter emission | `--check`, `--apply`, `--repair`, `--checklist`, `--apply-actions`, `<name>`, `--list`, `--dry-run`, `--force`, `--json` | `effigy.init.v1`, `effigy.init.checklist.v1`, `effigy.init.actions.v1`, `effigy.init.list.v1` | `019-init-foundation.md` |
| `effigy config` | Render config reference/schema snippets, including the optional repository-defined `[docs_policy.graph]` profile, inspect the effective composed manifest, or manage user-global container defaults | `inspect`, `schema`, `path`, `get`, `set`, `unset`, `--inspect`, `--path`, `--schema`, `--minimal`, `--target`, `--runner`, `--user-inspect`, `--json` | `effigy.config.v1` | `021-quick-start-and-command-cookbook.md`, [`../contracts/041-documentation-graph-profile-contract.md`](../knowledge/contracts/041-documentation-graph-profile-contract.md) |
| `effigy state artifact` | Inspect, stage, capture, and push versioned data artifacts to OCI registries or local staging (moved from the retired top-level `effigy artifact` in v0.15) | `inspect`, `stage`, `capture`, `--ref`, `--kind`, `--environment`, `--push`, `--farmyard-handoff`, `--json` | `effigy.artifact.inspect.v1`, `effigy.artifact.stage.v1`, `effigy.artifact.capture.v1` | `072-artifact-commands-guide.md` |
| `effigy config completion` | Prompt for shell completion setup on a real TTY, export raw shell completion scripts, install user-local completion files, wire bash/zsh startup automatically when needed, and surface selector candidates | `bash\|zsh\|fish`, `--install`, `--export`, `candidates`, `--repo`, `--prefix`, `--json` | `effigy.completion.v2`, `effigy.completion.candidates.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy changelog` | Validate, format, analyze, and extract Northstar changelog content | `validate`, `format`, `analyze`, `extract`, `--repo`, `--write`, `--preview`, `--version`, `--json` | changelog subcommands render direct output; some results can be wrapped in `effigy.command.v1` with global JSON mode | `052-changelog-workflows-and-northstar-profile.md` |
| `effigy fmt` / `effigy fmt-rust` | Format an explicit Rust file set with rustfmt, using the config nearest each file and never following a linked workspace. Relative file paths are cwd-relative. `effigy fmt-rust` is the explicit spelling when a repository task owns `fmt` | `rust <FILE>...`, `--check`, `--repo`, `--json`; `fmt-rust <FILE>...` takes the same flags | command envelope with the format report as text | `030-contributor-onboarding-15-minutes.md`, `085-v0.15.0-consumer-migration.md`, `016-task-routing-precedence.md` |
| `effigy state` | Plan, apply, capture, and inspect layered state-stack reports without moving app semantics into Effigy, and inspect/stage/capture standalone data artifacts | `plan [<STACK>]`, `plan --manifest <PATH>`, `plan --stack <NAME>`, `apply [<STACK>]`, `capture <STACK> <PROFILE>`, `capture --role ... --source-env ... --key ...`, `history [<STACK>]`, `artifact inspect|stage|capture`, `--write-report`, `--yes`, `--push`, `--repo`, `--json` | `effigy.state-stack.lineage.v1`, `effigy.state-stack.apply.v1`, `effigy.state-stack.capture.v1`, `effigy.state-stack.history.v1` | `073-state-stack-guide.md`, [`../contracts/016-state-stack-and-layered-seed-framework-contract.md`](../knowledge/contracts/016-state-stack-and-layered-seed-framework-contract.md) |
| `effigy <task>` / `effigy <catalog>/<task>` | Run manifest-defined tasks with routing rules; `--plan` inspects the resolved task, catalog, and command without executing; managed tasks also support a concurrent headless supervisor selected by flag or environment | Effigy flags before the task: `--json`, `--plan`, `--repo`, `--verbose-root`, `--env-schema`, `--lock-wait-ms`; `EFFIGY_LOCK_WAIT_MS`; everything after the task name is the task's (`<task> --plan` / `<task> --json` still Effigy's for one release); managed `--headless` / `EFFIGY_MANAGED_HEADLESS=1`, `status`, `logs [process] [--follow]`, `stop` | `effigy.task.run.v1`, `effigy.task.plan.v1`; lock-conflict details use `effigy.lock-wait.v2` | `012-dev-process-manager-tui.md`, `016-task-routing-precedence.md`, `020-dag-lock-policy-baseline.md`, `022-manifest-cookbook.md`, `050-env-schema-integration.md` |
| `effigy --run-id <key> <task>` | Name a task run with a caller key; a repeat attaches to that run and exits with its result | leading `--run-id`; `EFFIGY_ADMISSION_HOOK` gates `admission = "heavy"` tasks | `effigy.task.run.v1`; attach reuses the run's result | `080-host-wide-validation-admission.md` |
| `effigy runs` / `effigy run status\|logs\|stop <run>` | List, inspect, follow and stop runs from the kernel run journal | `--limit`, `--all`; `logs --follow`, `--stdout-offset`, `--stderr-offset`; `stop --wait-ms`; `--json` | `effigy.runs.v1`, `effigy.run.v1`, `effigy.run.logs.v1` | `080-host-wide-validation-admission.md` |

## JSON Envelope

For sample payloads per schema, see [`026-json-payload-examples.md`](026-json-payload-examples.md).

Canonical JSON mode:

```sh
effigy --json <command>
```

All command JSON responses are wrapped in:
- envelope schema: `effigy.command.v1`
- command-specific payload in `result` (or `error.details` for some failures)

See [`017-json-output-contracts.md`](017-json-output-contracts.md) for envelope and payload details.

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
effigy skill tasks --path <SKILL_DIR|EFFIGY_TOML> [--json]
effigy skill run [--path <SKILL_DIR|EFFIGY_TOML>] <SELECTOR> [--repo <CONSUMER>] [--json] [--stdio passthrough] [-- <ARGS>]
effigy deps [--repo <PATH>] [--json]
effigy deps status [cargo|bun] [--repo <PATH>] [--json]
effigy defer [--repo <PATH>] [--json] <REQUEST> [args...]
effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]
effigy doctor --deep [--catalog <ALIAS> | --all-catalogs] [--refresh]
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
effigy contracts check-json [--repo <PATH>] [--index <PATH>] [--contract <PATH>] [--changed-only <BASE>] [--print-selected[=text|json]] [--validate-selection] [--artifact <PATH>] [--json]
effigy scan <subcommand> [options]
effigy scan <subcommand> [--markdown] [--out <PATH>] [--json]
```

Common values:

- docs check kinds: `links`, `json-examples`, `headings`, `paths`,
  `contains`, `forbidden`, `index`, `next-action`, `workflow-paths`
- `docs check links` default scope: `README.md` plus `docs/**/*.md`;
  recursive walks skip `target/` and `node_modules/` at entry and still
  check explicit file paths ([docs-check source traversal](../knowledge/architecture/030-docs-check-source-traversal.md))
  default 24000, maximum 100000; `--max-hops` default 1, maximum 3
- scanners: `god-files`, `duplicate-blocks` (v0.15 removed the other five;
  use `rg -n 'TODO|FIXME' <path>` for marker searches)

### Local Runtime and Services

```sh
effigy service list [--repo <PATH>] [--json]
effigy service extract <SERVICE> [--repo <PATH>] [--dir <PATH>] [--json]
effigy exec [--repo <PATH>] [--service <NAME>] [--json] <COMMAND> [ARGS...]
effigy gateway setup [--suffix <DOMAIN>] [--remove] [--json]
effigy gateway <up|down|status|setup-tls> [--private-state-root <DIR>] [--json]
effigy gateway up --private-state-root <DIR> [--dns-addr <IP:PORT>] [--proxy-addr <IP:PORT>] [--https-addr <IP:PORT>] [--json]
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
effigy container profile resize [--profile <NAME>] [--json]
effigy container profile purge [--profile <NAME>] [--yes] [--json]
effigy container profile recreate [--profile <NAME>] [--disk <GiB>] [--yes] [--json]
effigy container cache list [--repo <PATH>] [--global] [--project <NAME>] [--kind <KIND>] [--json]
effigy container cache prune [--repo <PATH>] [--global] [--project <NAME>] [--kind <KIND>] [--yes] [--json]
effigy container data list [--repo <PATH>] [--json]
effigy container data export <VOLUME> <PATH> [--repo <PATH>] [--json]
effigy container [<NAME>] data dump [<FILE|OCI>|<TARGET>|<TARGET>=<FILE|OCI>]... [--db-dump <FILE|OCI>|<TARGET>|<TARGET>=<FILE|OCI>]... [--push] [--repo <PATH>] [--json]
effigy container data import <VOLUME> <PATH> [--repo <PATH>] [--yes] [--json]
effigy container data pull-production [--repo <PATH>] [--yes] [--json]
effigy container data seed [--db-seed <FILE|OCI>|<TARGET>=<FILE|OCI>]... [--no-prompt] [--yes] [--repo <PATH>] [--json]
effigy container <NAME> logs [--repo <PATH>] [--service <NAME>] [--follow] [--json]
effigy container <NAME> restart <process>... [--repo <PATH>] [--json]
effigy container <NAME> shell [--repo <PATH>] [--service <NAME>] [--command <CMD>]
effigy container <NAME> reset [--repo <PATH>] [--keep-data] [--json]
effigy container render [<NAME>] [--repo <PATH>] [--json]
effigy container migrate [<NAME>] [--repo <PATH>] [--json]
effigy container hosts [--repo <PATH>] [--json]
effigy container <NAME> hosts [--repo <PATH>] [--json]
effigy container scope [--repo <PATH>] [--json]
effigy container retire [--repo <PATH>] [--scope <TOKEN>] [--keep-data] [--yes] [--json]
effigy container retire --dead [--keep-data] [--yes] [--json]
effigy workspace [<NAME>] [--system <NAME>] [--repo <PATH>]
```

`effigy container data seed` currently targets the repo default container only
and stays on the generated-compose path.
`effigy container cache list` inventories purge-safe isolated build caches such
as Rust `target`, `node_modules`, `pnpm-store`, and Cargo caches. `--global`
uses machine-level runtime inventory, including stopped project caches where the
runtime metadata allows it. Text and JSON include reclaimable and in-use size
totals; unknown sizes are counted instead of guessed.
`effigy container retire` removes every container, volume and network the
backend labels for this checkout, including Created and stopped containers, then
releases its routes, ports and loopback addresses by checkout path.
`--keep-data` keeps the volumes instead of removing them and lists what it kept.
Anything it could not delete is named with the runtime's error and the command
exits non-zero. Call it from worktree teardown. `--dead` retires every
labelled stack whose checkout path no longer exists. It rejects `effigy
container <NAME> retire` before checkout resolution or runtime inventory,
including when `--scope` or `--yes` is also supplied. Pass `--scope <TOKEN>` to
restrict retirement to one scope token of this checkout.
`effigy container volume list` inventories Effigy-managed named volumes. `--dormant`
shows repo-scoped superseded volumes; `--global` shows machine-level volumes
across available runtimes and `--orphans` narrows that global view to ownerless
volumes.
`effigy container data dump` exports logical SQL dumps from generated-compose
database services; `data export` still exports raw named-volume archives.
Use `[data.targets.<name>]` when a sidecar DB should participate in
bootstrap/data seed/data dump without becoming part of `[bundle].databases`.

### Local Effigy Cleanup

```sh
effigy uninstall [--json]
effigy uninstall --plan [--json]
effigy uninstall --yes [--json]
```

### Bundles and Bootstrap

```sh
effigy bundle inspect [--repo <PATH>] [--json]
effigy bundle sync [--json]
effigy deploy model [<ENV>] [--repo <PATH>] [--json]
effigy deploy export <PROVIDER> [<ENV>] [--repo <PATH>] --path <DIR> [--plan] [--json]
effigy deploy plan <ENV> [--repo <PATH>] [--write-report] [--json]
# Unsupported in v0.15; retained spelling always refuses:
effigy deploy apply <ENV> [--repo <PATH>] --yes [--json]
effigy deploy status <ENV> [--repo <PATH>] [--json]
effigy deploy history <ENV> [--repo <PATH>] [--limit <N>] [--json]
# Unsupported in v0.15; retained spelling always refuses:
effigy deploy redeploy <ENV> [--repo <PATH>] --deployment <ID> --yes [--json]
effigy bootstrap [<GIT_URL>] [--path <DIR>] [--branch <NAME>] [--no-start] [--plan] [--json]
```

### Testing and Project Workflow

```sh
effigy test [--plan] [--verbose-results] [--tui] [suite] [runner args]
effigy init [--check|--apply|--repair] [--json]
effigy init --checklist [--json]
effigy init --apply-actions <ID>[,<ID>...] [--json]
effigy init <name> [--dry-run] [--force] [--json]
effigy init --list [--json]
effigy <task> [--repo <PATH>] [--verbose-root] [--env-schema <PATH>] [--lock-wait-ms <MS>] [--plan] [--json] [task args]
effigy <catalog>/<task> [--repo <PATH>] [--verbose-root] [--env-schema <PATH>] [--lock-wait-ms <MS>] [--plan] [--json] [task args]
effigy --run-id <key> <task> [task args]
effigy runs [--limit <N>] [--all] [--json]
effigy run status <run> [--json]
effigy run logs <run> [--follow] [--stdout-offset <N>] [--stderr-offset <N>] [--json]
effigy run stop <run> [--wait-ms <MS>] [--json]
```

See [`080-host-wide-validation-admission.md`](080-host-wide-validation-admission.md)
for heavy tasks, the admission hook, and run control.

Use `effigy scan <subcommand> --help` (and per-scanner help) for optional scan
flags such as `--include`, `--exclude`, `--threshold`, `--high`, `--critical`
and `--no-gitignore`.

### Completion, Changelog, and Release

```sh
effigy config completion [<bash|zsh|fish>] [--install|--export] [--json]
effigy config completion candidates [--repo <PATH>] [--prefix <value>] [--json]
effigy changelog <validate|format|analyze|extract> [--repo <PATH>] [ARGS...] [--json]
effigy fmt rust <FILE>... [--check] [--repo <PATH>] [--json]
effigy fmt-rust <FILE>... [--check] [--repo <PATH>] [--json]
effigy release <--plan|status|gates|run|resume|approve|decide|receipt> [ARGS...] [--json]
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
- `skill --path` selects one isolated task source; it never selects the
  consumer. Without `--path`, `skill run` derives the skill name from a
  qualified `<skill>/<task>` selector and resolves the invocation project's
  `.agents/skills/<skill>` first, then the unique installed global root under
  `~/.agents/skills`, `~/.codex/skills`, `~/.claude/skills`, or
  `~/.cursor/skills`; distinct global matches fail closed. `skill run` resolves
  the consumer from invocation CWD or `--repo`, executes host-only tasks from
  the consumer root, and rejects members, container/runtime inheritance,
  managed/TUI/concurrent shapes, manifest secrets, escaping composition, and
  canonically escaping Rhai assets before task side effects
- `skill run --stdio passthrough` gives the selected task raw
  stdin/stdout/stderr and its exit status, and cannot combine with `--json`
- `deps` and `deps status` are the same read-only inspection path; manager
  filtering is optional; `deps link cargo` applies the verified full closure
  and `deps unlink cargo` proves committed-source and lock recovery; `deps
  link bun` applies one explicit save-less full-closure link; `deps unlink bun`
  removes exact local symlinks and releases only that consumer's claims,
  including a named gone `--repo` checkout and the nested Bun roots that recorded
  it;
  `bun install` can drift a desired link, so inspect with status/doctor and
  re-run the same link command to repair it
- `exec` runs inside the manifest default system workspace container and
  defaults to that container's `primary_service` unless `--service` is
  supplied; primary-service commands use the declared `workspace_user` and
  `workspace_home`, interactive callers retain a TTY, and non-console callers
  run without one
- standard `gateway setup` is the administrator step for resolver, loopback,
  TLS trust and low-port prerequisites; `gateway setup --remove` tears down
  verified platform output. Ordinary `up`, `down`, `status` and route registration
  run as the operator without elevation. Standard `setup-tls` translates to setup;
  private mode does not request privilege, install trust, change host resolver
  files, or provision host loopback aliases
- `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT=<DIR>` selects the same caller-owned
  instance for container route registration and managed startup; clients must
  explicitly trust `<DIR>/ca/rootCA.pem` and resolve each route name to the
  actual HTTPS address shown by `gateway status --json`
- `gateway status --json` keeps unprovable state unknown: when this caller
  cannot read or verify the recorded generation, the command exits non-zero and
  prints the `effigy.gateway.status.v1` refusal with `ok: false`,
  `status: "unknown"`, a recorded `pid` when one was read, and
  `observations.record`/`observations.process`/`observations.identity` labels
  (`authenticated`, `running`, `permission-denied`, `unreadable`, …). A
  headless or noninteractive plan gates on those observations —
  `record=authenticated` and `process=running` with a recorded PID — and never
  on `gateway status` exit 0; `identity=permission-denied` or `identity=unknown`
  stays unknown and the command stays non-zero. `headless.satisfied` in that
  refusal reports only the observation precondition, not readiness. Resolving a
  denied identity still needs the bounded read-only check from an
  interactive admin-capable terminal, or `gateway setup` to migrate a legacy
  root-owned gateway
- routes with `tls = true` redirect plain HTTP to HTTPS once the gateway TLS
  listener is available
- `workspace` and interactive `container shell` do not support `--json`; finite
  `container shell --command … --bounded` capture supports it when all required
  deadline and byte limits are supplied
- `container status --global` and `container stats --global` are cross-project views
- `container migrate` writes a deprecated `compose_file` out as project catalog
  fragments with an unchanged render; `container eject` is retired
- volume and data commands run on the environment's exact backend and touch
  only volumes whose Effigy labels name the project
- `container logs --json` returns a finite snapshot; `container logs --follow`
  streams logs and rejects `--json`.
- `container <NAME> restart <process>...` replaces only the named host
  processes and the processes that `depends_on` them, leaving the containers and
  every other host process running; an unknown name fails before any process
  changes, a listener keeps its published address, and an environment that is
  not up is one warning rather than a refusal.
- `container profile repair` and `container profile reset-runtime` are the
  recovery surfaces for half-up substrate state; they keep the
  `effigy.system.recover.v1` JSON envelope and accept `--repo` and `--system`
- `effigy container` without a name resolves `[containers].default` and then the
  `[systems].default` workspace container; `--system <NAME>` selects one system
- mounted sibling repos listed in `systems.<name>.mounts` auto-adopt
  producer-declared isolation paths into workspace containers
- `bundle inspect` reports the active repo bundle source only; there is no named
  bundle inspection anymore
- `bundle sync` re-acquires every checkout bundle and catalog source pin,
  including local path snapshots; `dev`, `bootstrap` and `container up` update
  a stale pin themselves (`update = "manual"` opts a source out), and
  inspection never fetches, checks or writes
- `deploy model` is intentionally JSON-only. It reads the active rendered
  `[deploy.model]` section from the effective manifest and resolves the named
  `[deploy.<env>.model]` overrides over it, recording key provenance and
  refusing an undeclared environment
- a project with one declared `[deploy.<env>]` may omit `<ENV>` for one release
  with a notice; with two or more, `<ENV>` is required
- `deploy export <PROVIDER> [<ENV>]` requires a matching
  `[deploy.providers.<provider>]` package with an export capability, and the
  environment's `[deploy.<env>.provider]` must select the same provider
- a non-development environment never inherits shared `.test`/`.localhost`/
  `.local` domains or development-only task commands; they are withheld, listed
  under `missing` and warned about with `environment-missing-field`
- the external Render provider package currently writes `render.yaml`
- the external Railway provider package currently writes service-local
  `railway.toml` files plus `report.json`
- `deploy plan` lists intended obligations and runs configured provider preflight;
  a plan is not proof of execution
- `[deploy.providers.<name>]` resolves path/git provider packages for export,
  preflight and status. These scripts use `deploy::provider_context()` and
  `deploy::provider_report(...)`; a declared `apply.rhai` is not invoked in v0.15
- `deploy apply` and `deploy redeploy` are not yet supported in v0.15, regardless
  of `--yes`, provider setup or saved history. Both exit nonzero and name the
  unexecuted obligations; see [guide 074](074-deployment-guide.md). The full
  execution design is in Queue's deploy lane (Q-003)
- `bootstrap` runs `[bootstrap].start` after setup by default; pass `--no-start`
  to skip that phase; it fails fast on dirty existing checkouts or remote
  mismatches
- task execution locks on `task:<name>` by default; `tasks.<name>.lock` opts
  several tasks into a shared lock scope; `test:focused` locks per invocation
  by the arguments its command receives, so different filters run together and
  identical ones wait for each other
- managed `mode = "tui"` tasks also acquire `profile:<task>/<profile>`
- managed `concurrent` entries accept `shutdown_on_exit = true`
- managed `start` ranks define spawn order in both TUI and headless modes; ties
  preserve manifest order
- managed readiness probes only container-owned routes started by the lifecycle
  entry, and `health_wait_timeout_secs` controls the deadline
- managed headless runs keep task-local state and logs under
  `.effigy/runtime/managed/`; `EFFIGY_MANAGED_HEADLESS=1` is equivalent to
  `--headless`. Headless runs skip shell-role processes (no interactive
  TTY is ever allocated), stop on SIGINT/SIGTERM through the same path
  as `effigy <task> stop`, and record the lifecycle-bound container
environment in `session.json` so a whole-environment handover can end
  the live owner bound to the losing environment
- `--verbose-root`, `--env-schema`, and `--lock-wait-ms` apply to manifest task
  invocations; `EFFIGY_LOCK_WAIT_MS` is the env fallback when the flag is
  omitted. The default wait is `0` except for `test:focused`, which waits about
  two minutes for an identical invocation; an explicit value wins. The passthrough-style built-ins `doctor` and `scan` reject
  `--verbose-root` and `--env-schema` on the built-in invocation itself (use
  `effigy <builtin> --help` and
  [`050-env-schema-integration.md`](050-env-schema-integration.md))
- `effigy <selector> --plan` inspects the resolved task, catalog, and command
  without starting a task process. `--json` is output format only:
  `effigy --json <selector>` still executes. Pass `-- --plan` when the task
  itself should receive `--plan`. Task text runs keep the framed header on
  stderr so stdout is only the task bytes.
- all scan commands accept either `--json` or `--markdown`, not both
  findings and reports refreshed readiness when a scan family does not enrich
  findings yet
- scan `--out <PATH>` values resolve relative to the scanned repo root
- `config --minimal` requires `--schema`
- `config --inspect` cannot be combined with `--schema`
- `config --path` requires `--inspect`
- `config --runner` requires `--schema --target test`
- a task's lock is its run's gate: it is released when that run exits and is
  never removed by hand; `effigy tasks status <task>` names a live holder
- release operator flows should prefer built-in `effigy release ...` commands,
  not wrapper scripts
- there is no top-level `effigy distribution` command, and the
  `effigy release validate|preflight|proof|check-binary|verify-install|evidence`
  commands retired in v0.15: Effigy's own checks are the `release:` repo tasks
  in `config/tasks.toml`, and a project declares its own as `[release.gates]` or
  a target's `verify`. `effigy release receipt <run>` replaces the evidence
  summary
- top-level `[env]`, task-local `env`, and run-array env directives all
  participate in task execution; use the env guide for the full fallback and
  indirection rules
- config completion candidates are cached and include both built-ins and
  effective task selectors
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
## Common Recipes

Routing diagnosis:

```sh
effigy tasks --resolve test
effigy --plan app/build
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
effigy --json scan duplicate-blocks
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
effigy release --plan
effigy release run --to certify
effigy release receipt --json
effigy --json release status --check-gates
```

Installed skill task source with an independent consumer target:

```sh
effigy skill tasks --path ~/.agents/skills/northstar
effigy skill run --path ~/.agents/skills/northstar northstar/check --repo /work/app
effigy skill run northstar/retired-concepts --stdio passthrough < payload.json
effigy --json skill run --path ~/.agents/skills/northstar northstar/check --repo /work/app
```

Documentation and code navigation:

```sh
rg -n "<symbol>" crates src
rg -n "<question>" docs
```

`docs/README.md` is the documentation index; `effigy test --plan` answers which
tests run for a change.

Live run inspection:

```sh
effigy tasks status dev
effigy --lock-wait-ms 600000 dev
```

## Related Guides

- [`017-json-output-contracts.md`](017-json-output-contracts.md)
- [`021-quick-start-and-command-cookbook.md`](021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md)
- [`024-ci-and-automation-recipes.md`](024-ci-and-automation-recipes.md)
- [`026-json-payload-examples.md`](026-json-payload-examples.md)
- [`055-everyday-workflows.md`](055-everyday-workflows.md)
- [`057-bootstrap-repo-bringup.md`](057-bootstrap-repo-bringup.md)
- [`036-release-notes-authoring-template-and-examples.md`](036-release-notes-authoring-template-and-examples.md)
- [`051-release-orchestration.md`](051-release-orchestration.md)
- [`052-changelog-workflows-and-northstar-profile.md`](052-changelog-workflows-and-northstar-profile.md)
- [`034-task-and-command-glossary.md`](034-task-and-command-glossary.md)
- [`072-artifact-commands-guide.md`](072-artifact-commands-guide.md)

## Expected Outcome

After this guide, you should be able to:

- choose the right Effigy command without scanning multiple docs first
- confirm the key flags and JSON schema for a command quickly
- jump from the reference surface to the deeper workflow page only when needed

## Next Step

After using this matrix to find the right command, move to the matching workflow
guide and simplify the corresponding repo path so people can rely on that
command directly instead of a local wrapper or tribal workaround.
