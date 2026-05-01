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
- Need tests, watch mode, init, or migrate: use `effigy test`, `watch`,
  `init`, or `migrate`.
- Need to inspect bundled service fragments or take local ownership: use
  `effigy service`.
- Need one ad-hoc command inside the dev container without opening a shell:
  use `effigy exec`.
- Need host-local DNS, route status, or TLS setup for container domains: use
  `effigy gateway`.
- Need machine-readable output: add `--json`.
- Need repo-health scanners: use `effigy scan`.
- Need to clone or update a repo and run its declared bring-up path: use
  `effigy bootstrap`.
- Need proof/demo discovery, inspection, or one-off proof execution: use
  `effigy demo`.
- Need a host-clean local web/service environment or data lifecycle tools: use
  `effigy container`.
- Need substrate lifecycle (VM + compose + gateway + workspace handoff) for the
  repo's declared system: use `effigy system` and `effigy workspace`.
- Need to discover or inspect a shipped or local bundle before adopting it: use
  `effigy bundle`.
- Need a provider-neutral production model derived from the effective manifest
  and bundle: use `effigy deploy model --json`.
- Need release workflows: use `effigy release`.
- Need distribution validation, GLIBC checks, artifact validation, or
  first-publish evidence: use `effigy distribution`.

## Primary Commands

| Command | Purpose | Key Flags | JSON Schema(s) | Deep Dive |
| --- | --- | --- | --- | --- |
| `effigy help` / `effigy --help` | Show CLI help and topic guidance | `--json` | `effigy.help.v1` (inside command envelope) | `021-quick-start-and-command-cookbook.md` |
| `effigy tasks` | List discovered catalogs/tasks and probe routing | `--repo`, `--task`, `--resolve`, `--json`, `--pretty true\|false` | `effigy.tasks.v1`, `effigy.tasks.filtered.v1` | `016-task-routing-precedence.md` |
| `effigy service` | Inspect the layered service catalog and extract bundled fragments into repo-owned overrides | `list`, `extract`, `--repo`, `--dir`, `--json` | service commands render command-envelope JSON with catalog payloads | `063-container-system-guide.md` |
| `effigy exec` | Run one ad-hoc command inside the manifest's dev-context container | `--repo`, `--service`, `--json` | exec commands render command-envelope JSON with exec payloads | `063-container-system-guide.md` |
| `effigy gateway` | Operate the host-native local DNS and reverse-proxy gateway for container-owned routes | `up`, `down`, `status`, `setup-tls`, `--json` | gateway commands render command-envelope JSON with gateway payloads | `063-container-system-guide.md` |
| `effigy doctor` | Run health checks and optional explain-mode selection diagnostics | `--repo`, `--fix`, `--verbose`, `--json` | `effigy.doctor.v1`, `effigy.doctor.explain.v1` | `018-doctor-explain-mode.md` |
| `effigy docs` | Run reusable docs QA checks such as path presence, link validation, heading/content/forbidden-text checks, JSON example validation, markdown index consistency checks, next-action policy validation, workflow-path validation, and log-index entry insertion | `check-links`, `check-json-examples`, `check-headings`, `check-paths`, `check-contains`, `check-forbidden`, `check-index`, `check-next-action`, `check-workflow-paths`, `add-log-index`, `--repo`, `--file`, `--section`, `--min-blocks`, `--require`, `--require-heading`, `--require-block`, `--forbid`, `--policy-index`, `--policy`, `--dir`, `--index`, `--json` | `effigy.docs.link-check.v1`, `effigy.docs.json-examples.v1`, `effigy.docs.heading-check.v1`, `effigy.docs.path-check.v1`, `effigy.docs.contains-check.v1`, `effigy.docs.forbidden-check.v1`, `effigy.docs.index-check.v1`, `effigy.docs.next-action-check.v1`, `effigy.docs.workflow-path-check.v1`, `effigy.docs.add-log-index.v1` | `029-docs-qa-checklist-and-validation.md` |
| `effigy contracts` | Validate reusable JSON contract artifacts such as selection payloads and schema-index contract coverage | `check-json`, `validate-selection`, `--repo`, `--index`, `--fast`, `--full`, `--changed-only`, `--print-selected`, `--contract`, `--artifact`, `--json` | `effigy.contracts.check-json.v1`, `effigy.contracts.selection-validation.v1` | `017-json-output-contracts.md` |
| `effigy distribution` | Run distribution preflight, GLIBC floor validation, first-publish evidence capture, artifact validation, and closeout generation | `preflight`, `validate-metadata`, `check-glibc-floor`, `first-publish`, `validate-artifacts`, `generate-closeout`, `write-summary`, `--repo`, `--tag`, `--skip-docs`, `--skip-smoke`, `--skip-homebrew`, `--artifacts-dir`, `--crate-version`, `--repo-url`, `--brew-formula`, `--output`, `--owner`, `--expect-homebrew`, `--homebrew-executed`, `--log-file`, `--json` | `effigy.distribution.preflight.v1`, `effigy.distribution.metadata.v1`, `effigy.distribution.artifacts.v1`, `effigy.distribution.closeout.v1`, `effigy.distribution.summary.v1` | `062-distribution-system-guide.md` |
| `effigy container` | Operate manifest-defined Colima-backed local environments, data lifecycle, shared-service reuse, and cross-project status views | `up`, `down`, `status`, `stats`, `logs`, `shell`, `data`, `reset`, `eject`, `--repo`, `--attach`, `--detach`, `--service`, `--command`, `--follow`, `--all`, `--keep-data`, `--json` | `effigy.container.up.v1`, `effigy.container.down.v1`, `effigy.container.status.v1`, `effigy.container.logs.v1` | `063-container-system-guide.md` |
| `effigy system` | Operate the manifest's declared default system substrate (VM + compose + gateway) with lifecycle, log streaming, and recovery surfaces | `up`, `down`, `status`, `logs`, `repair`, `reset-runtime`, `--system`, `--repo`, `--follow`, `--json` | `effigy.system.recover.v1` | `064-system-workspace-and-dev-contract.md` |
| `effigy workspace` | Ensure the selected system is up and then open the resolved workspace shell for the repo's declared developer surface | `<WORKSPACE>`, `--system`, `--repo` | (interactive; no JSON payload) | `064-system-workspace-and-dev-contract.md` |
| `effigy bundle` | Discover, inspect, and export shipped top-level bundles referenced from `[bundle]` in `effigy.toml` | `list`, `inspect`, `export`, `--path`, `--json` | `effigy.bundle.list.v1`, `effigy.bundle.inspect.v1`, `effigy.bundle.export.v1` | `065-underlay-starter.md` |
| `effigy deploy` | Derive a provider-neutral production deployment model and export the first bounded provider files from the effective manifest and bundle | `model`, `export render`, `export railway`, `--repo`, `--path`, `--plan`, `--json` | `deploy.model.v1`, `effigy.deploy.export.v1` | `002-production-deployment-model.md` |
| `effigy bootstrap` | Clone or update a repo from a git URL, apply its root bootstrap contract, sync optional submodules, bring along child repos, run setup, optionally start the declared dev task, and expose `bootstrap deps sync` for typed dependency hydration | `<git-url>`, `deps sync`, `--path`, `--branch`, `--start`, `--plan`, `--js-only`, `--rust-only`, `--json` | `effigy.bootstrap.v1`, `effigy.bootstrap.deps.v1` | `057-bootstrap-repo-bringup.md` |
| `effigy demo` | Discover repo-owned proof demos, browse them in the demo browser, inspect active/latest state, query retained attempt history, execute new attempts, and control runner-owned lifecycle for active demos | `list`, `browser`, `inspect`, `history`, `run`, `stop`, `input`, `resize`, `rerun`, `--repo`, `--json` | `effigy.demo.list.v1`, `effigy.demo.inspect.v1`, `effigy.demo.history.v1`, `effigy.demo.run.v1`, `effigy.demo.stop.v1`, `effigy.demo.input.v1`, `effigy.demo.resize.v1`, `effigy.demo.rerun.v1` | `058-demo-system-guide.md` |
| `effigy scan` | Run built-in repo scanners such as oversized code-file detection, duplicate-block detection, comment-ratio detection, bulky generated-asset detection, generated-in-src detection, attention-marker detection, and stale-suppression detection | `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-assets`, `generated-in-src`, `attention-markers`, `stale-suppressions`, `--json`, `--markdown`, `--out`, `--fail-on-findings`, `--show-warnings` | `effigy.scan.god-files.v1`, `effigy.scan.duplicate-blocks.v1`, `effigy.scan.comment-ratio.v1`, `effigy.scan.generated-assets.v1`, `effigy.scan.generated-in-src.v1`, `effigy.scan.attention-markers.v1`, `effigy.scan.stale-suppressions.v1` | `022-manifest-cookbook.md` |
| `effigy test` | Run built-in or explicit `tasks.test` test orchestration | `--plan`, `--verbose-results`, `--tui`, `--json` | `effigy.test.plan.v1`, `effigy.test.results.v1` | `013-testing-orchestration.md` |
| `effigy watch` | Policy-first file-triggered reruns for a target task | `--owner`, `--debounce-ms`, `--include`, `--exclude`, `--once`, `--max-runs`, `--json` | `effigy.watch.v1` (bounded JSON runs) | `019-watch-init-migrate-foundation.md` |
| `effigy init` | Scaffold baseline `effigy.toml` from a named starter (e.g. `minimal`, `underlay`, `northstar`) or list available starters | `<name>`, `--list`, `--dry-run`, `--force`, `--json` | `effigy.init.v1` | `019-watch-init-migrate-foundation.md` |
| `effigy migrate` | Import `package.json` scripts into `[tasks]` | `--from`, `--script`, `--apply`, `--json` | `effigy.migrate.v1` | `019-watch-init-migrate-foundation.md` |
| `effigy config` | Render config reference/schema snippets or inspect the effective composed manifest | `--inspect`, `--path`, `--schema`, `--minimal`, `--target`, `--runner`, `--json` | `effigy.config.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy unlock` | Clear lock scopes manually | `--all`, `--json` | `effigy.unlock.v1` | `020-dag-lock-policy-baseline.md` |
| `effigy cache` | Inspect and invalidate phase-1 cache metadata | `inspect`, `invalidate`, `--all`, `--json` | `effigy.cache.v1` | `022-manifest-cookbook.md` |
| `effigy completion` | Generate shell completion scripts and selector candidates | `bash\|zsh\|fish`, `candidates`, `--repo`, `--prefix`, `--json` | `effigy.completion.v1`, `effigy.completion.candidates.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy changelog` | Validate, format, analyze, and extract Northstar changelog content | `validate`, `format`, `analyze`, `extract`, `--write`, `--preview`, `--version`, `--json` | changelog subcommands render direct output; some results can be wrapped in `effigy.command.v1` with global JSON mode | `052-changelog-workflows-and-northstar-profile.md` |
| `effigy release` | Inspect release readiness, run gates, preview or apply release mutations, resume prepared-state review, execute release flow, and verify tagged installs | `status`, `gates`, `resume`, `simulate`, `prepare`, `execute`, `verify-install`, `--check-gates`, `--plan`, `--dry-run`, `--yes`, `--version`, `--allow-stale`, `--tag`, `--repo-url`, `--json` | `effigy.release.status.v1`, `effigy.release.gates.v1`, `effigy.release.resume.v1`, `effigy.release.simulate.v1`, `effigy.release.prepare.plan.v1`, `effigy.release.prepare.v1`, `effigy.release.execute.plan.v1`, `effigy.release.execute.v1`, `effigy.release.verify-install.v1` | `051-release-orchestration.md` |
| `effigy <task>` / `effigy <catalog>/<task>` | Run manifest-defined tasks with routing rules | passthrough args, `--json` | `effigy.task.run.v1` | `022-manifest-cookbook.md` |

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
effigy tasks [--task <TASK_NAME>] [--resolve <SELECTOR>] [--json]
effigy doctor [--fix] [--verbose] [--json]
effigy doctor <task> -- <args> [--json]
effigy config --inspect [--path <dotted.path>] [--json]
effigy config --schema [--minimal] [--target <manifest|section>] [--runner <runner>] [--json]
```

### Docs, Contracts, and Scans

```sh
effigy docs <check> [ARGS...] [--json]
effigy contracts check-json [--fast|--full] [--changed-only <BASE>] [--print-selected|--print-selected=json] [--json]
effigy contracts validate-selection [--contract <PATH>] [--artifact <PATH>] [--json]
effigy scan <scanner> [SCANNER FLAGS...] [--json]
```

Common values:

- docs checks: `check-links`, `check-json-examples`, `check-headings`,
  `check-paths`, `check-contains`, `check-forbidden`, `check-index`,
  `check-next-action`, `check-workflow-paths`, `add-log-index`
- scanners: `god-files`, `duplicate-blocks`, `comment-ratio`,
  `generated-assets`, `generated-in-src`, `attention-markers`,
  `stale-suppressions`

### Local Runtime and Services

```sh
effigy service list [--json]
effigy service extract <SERVICE> [--dir <PATH>] [--json]
effigy exec [--service <NAME>] [--json] <COMMAND> [ARGS...]
effigy gateway <up|down|status|setup-tls> [--json]
effigy container [<NAME>] <up|down|status|logs|shell|reset|eject> [FLAGS...]
effigy container [<NAME>] data <list|export|import|pull-production> [ARGS...] [--json]
effigy system <up|down|status|logs|repair|reset-runtime> [--system <NAME>] [--json]
effigy workspace [<WORKSPACE>] [--system <NAME>]
```

### Bundles, Bootstrap, and Demos

```sh
effigy bundle <list|inspect|export> [ARGS...] [--json]
effigy deploy model [--repo <PATH>] --json
effigy deploy export render [--repo <PATH>] --path <DIR> [--plan] [--json]
effigy deploy export railway [--repo <PATH>] --path <DIR> [--plan] [--json]
effigy bootstrap <GIT_URL> [--path <DIR>] [--branch <NAME>] [--start] [--plan] [--json]
effigy bootstrap deps sync [<path>...] [--js-only|--rust-only] [--json]
effigy demo <list|browser|inspect|history|run|stop|input|resize|rerun> [ARGS...] [--json]
```

### Testing and Project Workflow

```sh
effigy test [--plan] [--verbose-results] [--tui] [suite] [runner args]
effigy watch --owner <effigy|external> [--debounce-ms <MS>] [--include <GLOB>] [--exclude <GLOB>] <task> [task args]
effigy init [<name>] [--dry-run] [--force] [--json]
effigy init --list [--json]
effigy migrate [--from <PATH>] [--script <NAME>]... [--apply] [--json]
effigy <task> [task args]
effigy <catalog>/<task> [task args]
```

### Cache, Completion, Changelog, and Release

```sh
effigy unlock [--all | <scope>...] [--json]
effigy cache inspect [<selector>] [--json]
effigy cache invalidate [<selector>...] [--all] [--json]
effigy completion <bash|zsh|fish> [--json]
effigy completion candidates [--prefix <value>] [--json]
effigy changelog <validate|format|analyze|extract> [ARGS...] [--json]
effigy release <status|gates|resume|simulate|prepare|execute|verify-install> [ARGS...] [--json]
effigy distribution <preflight|validate-metadata|check-glibc-floor|first-publish|validate-artifacts|generate-closeout|write-summary> [ARGS...] [--json]
```

## Scope Notes and Constraints

Use the deeper guides for full surface detail. The main sharp edges here are:

- `tasks --pretty false` is valid only with `--json`
- `watch --owner` is required
- `watch --json` requires bounded mode such as `--once` or `--max-runs`
- `exec` runs inside the manifest dev-context container and defaults to that
  environment's `primary_service` unless `--service` is supplied
- `gateway up`, `gateway down`, and `gateway setup-tls` may request host admin
  approval
- routes with `tls = true` redirect plain HTTP to HTTPS once the gateway TLS
  listener is available
- `container shell` and `workspace` are interactive and intentionally do not
  support `--json`
- `container status --all` and `container stats --all` are cross-project views
- generated-compose data lifecycle stays on the product-owned path and does not
  widen direct `compose_file` ownership
- `system logs` is streaming and intentionally does not support `--json`
- `system repair` and `system reset-runtime` are the recovery surfaces for
  half-up substrate state
- mounted sibling repos listed in `systems.<name>.mounts` auto-adopt
  producer-declared isolation paths into workspace containers
- `bundle export <BUNDLE> --path <DIR>` writes a local `base_path` bundle
  directory for repo-owned modifications
- `deploy model` is intentionally JSON-only in the first batch and currently
  supports the shipped `underlay` bundle only
- `deploy export render` is intentionally Underlay-first in the first batch and
  currently generates only `render.yaml`
- `deploy export railway` is intentionally Underlay-first in the first batch
  and currently generates service-local `railway.toml` files plus `report.json`
- `bootstrap` is stateless by default, runs `start` only with `--start`, and
  fails fast on dirty existing checkouts or remote mismatches
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
- all scan commands accept either `--json` or `--markdown`, not both
- scan `--out <PATH>` values resolve relative to the scanned repo root
- `config --minimal` requires `--schema`
- `config --inspect` cannot be combined with `--schema`
- `config --path` requires `--inspect`
- `config --runner` requires `--schema --target test`
- `unlock` accepts either explicit scopes or `--all`, not both
- `cache invalidate` accepts selectors or `--all`, not both
- release operator flows should prefer built-in `effigy release ...` commands,
  not wrapper scripts
- top-level `[env]`, task-local `env`, and run-array env directives all
  participate in task execution; use the env guide for the full fallback and
  indirection rules
- completion candidates are cached and include both built-ins and discovered
  task selectors

## Common Recipes

Routing diagnosis:

```sh
effigy tasks --resolve test
effigy doctor --repo /path/to/workspace app/build -- --watch
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
effigy --json scan comment-ratio
effigy --json scan generated-in-src
effigy --json scan attention-markers
effigy --json scan stale-suppressions
effigy --json bootstrap git@github.com:inflatable-cookie/loophole.git --plan
effigy --json test --plan
effigy release simulate
effigy release prepare --plan
effigy release execute --plan
effigy --json release status --check-gates
```

Lock recovery:

```sh
effigy unlock task:watch:test
effigy unlock --all
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

## Expected Outcome

After this guide, you should be able to:

- choose the right Effigy command without scanning multiple docs first
- confirm the key flags and JSON schema for a command quickly
- jump from the reference surface to the deeper workflow page only when needed

## Next Step

After using this matrix to find the right command, move to the matching workflow
guide and simplify the corresponding repo path so people can rely on that
command directly instead of a local wrapper or tribal workaround.
