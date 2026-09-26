# External Skill Task Execution

Created: 2026-08-31

## Purpose

Installed skill projects may own reusable Effigy tasks while the task operates
on a separate consuming repository. Effigy must not force one path to own both
the task definition and the runtime target.

## Context Model

Skill execution carries five distinct paths:

- invocation CWD: where the operator invoked Effigy
- source manifest: the explicit or named-resolved external `effigy.toml`
- source root: the directory containing that manifest
- target root: the consuming repository resolved from invocation CWD or
  `--repo`
- execution CWD: the target root for host task processes

Ordinary task execution keeps its current coupled catalog/root behavior. The
split activates only through the explicit `effigy skill` surface.

## Ownership

- `effigy-cli` owns `skill tasks` and `skill run` grammar and help.
- `effigy-context` owns typed source/target path evidence without changing the
  meaning of the existing command root.
- `effigy-manifest` and `effigy-routing` load one explicitly selected composed
  skill catalog.
- `effigy-execution` carries source and target paths through preflight and
  nested execution.
- the runner binds source-relative assets and target-relative runtime paths,
  then renders text/JSON evidence.

Do not make the skill directory a repository override. `--repo` remains the
consumer target.

## Path Classes

Source-relative:

- manifest includes and bundle materialization
- typed Rhai/script step paths, after source/bundle token rendering and
  canonical source-containment validation
- the `{skill}` task placeholder

Target-relative:

- host task process CWD
- `{repo}` and `{project}` placeholders
- task env files, cache inputs, and cache outputs
- state, artifacts, secrets, graph, docs, and nested built-in targets

Invocation CWD stays available as runtime evidence. It does not replace the
resolved target root.

## Isolation Boundary

The first surface loads one composed skill catalog and runs standard host/Rhai
tasks. Managed, TUI, and concurrent shapes remain outside v1.
It does not:

- merge skill tasks into the consumer's normal selector surface
- inherit consumer task defaults, systems, containers, env schema, or secrets
  declarations
- scan outside the contracted project/user agent-skill roots
- admit external catalog members or container-bound skill tasks
- mutate the consumer manifest to install the skill

Nested task references stay inside the selected skill catalog while preserving
the consumer target.

`run_skill_task` marks the process with
`EFFIGY_INTERNAL_EXTERNAL_TASK_SOURCE_ISOLATION` for the duration of the run.
Rhai steps execute in child `effigy script run` processes, so the marker
travels through the inherited environment rather than the in-process runtime
context. Under the marker, `resolve_rhai_secret_store` returns an isolated
store instead of reading the consumer manifest, and `resolve_task_secret_env`
injects nothing.

## Implementation Map

- `crates/effigy-cli` owns grammar, scoped help, and JSON-mode recognition.
- `crates/effigy-routing::load_isolated_catalog` loads exactly the composed
  source and rejects members, runtime-backed mounts, and escaping paths.
- `effigy-context::TaskSourceContext` carries canonical source identity beside
  the unchanged target runtime context.
- `effigy-execution` propagates that source through dispatch and preflight.
- `src/runner/skill_command.rs` resolves source/target evidence and recursively
  validates the selected skill task graph before execution.
- CLI entry/output dispatch recognizes passthrough before rendering so the
  selected child process can own raw stdio and exit status.
- `src/runner/execute` and `effigy-rhai` preserve the split through commands,
  env/cache paths, nested task references, and script steps; recursive preflight
  rejects managed shapes and canonically escaping scripts before execution.

## Public Surface

```text
effigy skill tasks --path <SKILL_DIR|EFFIGY_TOML> [--json]
effigy skill run [--path <SKILL_DIR|EFFIGY_TOML>] <SELECTOR> [--repo <CONSUMER>] [--json] [--stdio passthrough] [-- <ARGS>]
```

A directory resolves exactly `<directory>/effigy.toml`. A file resolves that
file. Missing, non-file, ambiguous, member-bearing, or non-host-compatible
sources fail before task execution.

When `skill run` omits `--path`, the qualified selector's first path segment
selects an agent skill name. Resolution is project-first at
`<invocation-project>/.agents/skills/<name>`, then unique-global across
`~/.agents/skills`, `~/.codex/skills`, `~/.claude/skills`, and
`~/.cursor/skills`. Canonical paths collapse symlink aliases; distinct global
matches fail closed. Discovery never follows `--repo` and never merges the
consumer catalog.

`--stdio passthrough` switches only the process transport. Preflight still
resolves and validates the isolated task graph first. After launch, the child
inherits stdin/stdout/stderr and its status becomes Effigy's status. The normal
renderer and JSON envelope are bypassed. `--json` is rejected with passthrough
before launch.

## Trust Posture

`skill run` executes code from an operator-supplied path or contracted installed
skill name. Normal verbose/JSON output must show the canonical source and
target. Raw passthrough deliberately emits no Effigy evidence; its explicit
flag and fail-closed discovery/preflight are the trust boundary.

## Related Authority

- [`011-runtime-context-contract.md`](../contracts/011-runtime-context-contract.md)
- [`013-task-execution-request-contract.md`](../contracts/013-task-execution-request-contract.md)
- [`037-explicit-catalog-membership-contract.md`](../contracts/037-explicit-catalog-membership-contract.md)
- [`042-external-skill-task-runner-contract.md`](../contracts/042-external-skill-task-runner-contract.md)
