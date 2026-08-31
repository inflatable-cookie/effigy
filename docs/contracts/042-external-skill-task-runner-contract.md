# 042 External Skill Task Runner Contract

Status: active
Owner: task routing and execution
Created: 2026-08-31

## Purpose

Define explicit execution of tasks shipped inside an installed skill while the
consuming repository remains Effigy's runtime target.

## Command Contract

Supported v1 commands:

```text
effigy skill tasks --path <SKILL_DIR|EFFIGY_TOML> [--json]
effigy skill run --path <SKILL_DIR|EFFIGY_TOML> <SELECTOR> [--repo <CONSUMER>] [--json] [-- <ARGS>]
```

Rules:

- `--path` is required and selects only the task-definition source.
- A directory resolves its direct `effigy.toml`; a file resolves itself.
- Paths are canonicalized before comparison and reporting.
- `skill tasks` loads and lists the isolated source without needing a consumer.
- `skill run` resolves the consumer from invocation CWD unless `--repo` is
  present.
- `--repo` never selects the skill source.
- Task arguments after `--` retain normal Effigy forwarding behavior.
- Existing selectors and existing `--repo` behavior do not change.

## Source Contract

The first version accepts one composed root skill catalog.

- manifest includes, bundle defaults, and Rhai/script assets resolve from the
  source manifest/root
- every Rhai/script asset reachable from the selected task graph resolves
  source and bundle path tokens with execution semantics, then is canonicalized
  before execution and must remain inside the source root; relative, absolute,
  and symlink escapes are rejected
- `[catalog.members]` is rejected on this surface
- ambient discovery outside the source root is forbidden
- consumer catalogs never join the skill selector set
- nested task references resolve only within the loaded skill catalog
- missing tasks and ambiguous/escaping source paths fail before execution

`skill tasks` text and JSON identify the source manifest, source root, catalog
alias, and available selectors.

## Target Contract

For `skill run`:

- target root uses normal nearest-root resolution or explicit `--repo`
- invocation CWD remains distinct runtime evidence
- host process execution CWD is the target root
- `{repo}` and `{project}` render the target root
- `{skill}` renders the source root
- task env files, cache inputs, and cache outputs resolve against the target
- Rhai/runtime context reports the target as command/repo root and exposes the
  task source root separately
- nested task and built-in dispatch preserve both source and target

## Isolation Contract

Skill execution is isolated by default.

- no consumer task defaults, env schema, systems, containers, secrets config,
  or bundle config is inherited
- skill tasks must resolve to host execution; container-bound/default-container
  sources fail before side effects
- managed, TUI, and concurrent task shapes are outside v1 and fail before
  process launch or managed runtime-state creation
- consumer built-ins invoked through typed nested dispatch target the consumer
  root but do not acquire consumer manifest task configuration
- skill source files are read-only to Effigy unless the task itself explicitly
  mutates them
- the consumer manifest is never rewritten to install or register the skill

Secret isolation is a runtime boundary, not only a manifest boundary. A skill
run never resolves or unlocks the consumer vault, so consumer-declared required
secrets cannot block an unrelated skill task on a non-interactive host, and
`secrets::get`, `secrets::has`, `secrets::set`, and `secrets::set_many` are
refused inside an isolated source. Consumer secret values are not injected into
an isolated task's environment even when the task names them.

This boundary is additive. Wider runtime inheritance needs a later contract and
must not appear as implicit fallback.

## Output Contract

Text diagnostics and the command envelope must expose enough evidence to audit
the split:

- canonical source root and manifest
- resolved target root for `run`
- invocation and execution CWD
- selected catalog alias and selector
- source and target resolution evidence
- exit status and normal task output

JSON uses a versioned skill payload. Text and JSON must agree on these facts.
No output may imply that the source root is the consumer repository.

## Failure Contract

Fail before task side effects when:

- the source does not resolve to one readable manifest
- source composition escapes the accepted boundary
- `[catalog.members]` is present
- the selector is missing or resolves outside the isolated catalog
- the task requires consumer/container runtime inheritance or managed execution
- the target cannot be resolved
- source and target evidence cannot be represented consistently

Errors name the failing source or target class and provide one direct recovery
step where possible.

## Compatibility

- `effigy --repo <SKILL_DIR> <SELECTOR>` retains its existing meaning.
- ordinary manifest tasks keep catalog-root process CWD and path semantics.
- no automatic migration or compatibility alias rewrites old Northstar command
  examples.

## Review Oracle

Invariant: one explicit source supplies code; one independently resolved target
owns runtime effects.

Smallest adversarial counterexamples:

1. Source and consumer define the same selector. Only the source task runs.
2. Source task reads `{repo}` and `{skill}`. They resolve to different canonical
   roots with the declared meanings.
3. Consumer declares default container execution. The skill task stays host
   bound and does not start the consumer container.
4. Skill manifest declares a member outside its root. Preflight stops before
   task output or filesystem mutation.
5. A nested skill task calls another skill task. Source stays the skill catalog;
   target stays the consumer.
6. `--repo` points at a second consumer while invocation CWD is in the first.
   Target and execution CWD use the explicit consumer; invocation evidence keeps
   the first CWD.

Required proof: parser/help tests, isolated routing fixtures, path-resolution
tests, nested task/Rhai tests, no-side-effect rejection tests, JSON contract
validation, and one read-only installed-Northstar skill smoke.
