# 042 External Skill Task Runner Contract

Status: active
Owner: task routing and execution
Created: 2026-08-31
Updated: 2026-09-13

## Purpose

Define explicit execution of tasks shipped inside an installed skill while the
consuming repository remains Effigy's runtime target.

## Command Contract

Supported commands:

```text
effigy skill tasks --path <SKILL_DIR|EFFIGY_TOML> [--json]
effigy skill run [--path <SKILL_DIR|EFFIGY_TOML>] <SELECTOR> [--repo <CONSUMER>] [--json] [--stdio passthrough] [-- <ARGS>]
```

Rules:

- `--path` remains required for `skill tasks`. It remains the exact, unchanged
  source override for `skill run` when present.
- Without `--path`, `skill run` requires a qualified `<skill>/<task>` selector,
  derives `<skill>`, and resolves that installed agent skill by name.
- A directory resolves its direct `effigy.toml`; a file resolves itself.
- Paths are canonicalized before comparison and reporting.
- `skill tasks` loads and lists the isolated source without needing a consumer.
- `skill run` resolves the consumer from invocation CWD unless `--repo` is
  present.
- `--repo` never selects the skill source.
- Task arguments after `--` retain normal Effigy forwarding behavior.
- Existing selectors and existing `--repo` behavior do not change.

## Named Source Resolution

Named lookup starts from the invocation CWD, never the consumer selected by
`--repo`. It checks the invocation project's direct `.agents/skills/<skill>`
directory first, then the user's installed skill roots under the home
directory: `.agents/skills`, `.codex/skills`, `.claude/skills`, and
`.cursor/skills`.

- the project-local source wins over every global source
- a candidate is an agent skill directory with direct `SKILL.md` and
  `effigy.toml` files
- canonicalized aliases or symlinks to the same directory count once
- multiple distinct global matches are ambiguous and fail with candidate paths
- an authoritative project-local candidate that lacks `effigy.toml` fails; it
  does not silently fall through to a global copy
- missing, unqualified, or invalid named sources fail before task execution
- no registry, network, ancestor walk, consumer catalog, or ambient manifest
  search participates

The full selector still selects the task. Deriving the skill name does not
rewrite catalog aliases or task names.

## Source Contract

The first version accepts one composed root skill catalog.

- manifest includes, bundle defaults, and Rhai/script assets resolve from the
  source manifest/root
- every Rhai/script asset reachable from the selected task graph resolves
  source and bundle path tokens with execution semantics, then is canonicalized
  before execution and must remain inside the source root; relative, absolute,
  and symlink escapes are rejected
- `[catalog.members]` is rejected on this surface
- ambient discovery outside the contracted named roots is forbidden
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

Normal text and `--json` behavior remain unchanged. Named resolution may add
source-discovery evidence without changing the command envelope contract.

### Raw stdio passthrough

`--stdio passthrough` is an explicit `skill run` transport mode. It is not JSON
mode and remains schema-agnostic.

- stdin bytes are inherited by the selected task unchanged
- the task owns stdout and stderr directly; Effigy adds no header, footer,
  spinner, envelope, newline, warning, or status text
- the Effigy process exits with the task's exit status, including non-zero
  status
- preflight and launch failures write a useful diagnostic only to stderr, leave
  stdout empty, and exit non-zero
- `--json` and `--stdio passthrough` are incompatible in either global or local
  flag position and fail before task execution
- all existing source isolation, target routing, graph preflight, and rejected
  task-shape rules still apply

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
7. A project and global root contain the same named skill. The project source
   runs even when `--repo` points elsewhere.
8. Two distinct global roots contain the same named skill. Resolution fails
   before either task runs; symlink aliases of one source do not create false
   ambiguity.
9. A task receives JSON stdin without a trailing newline in passthrough mode.
   Its stdin, stdout, stderr, and exit status cross the Effigy boundary
   byte-for-byte, with no Effigy output.
10. Passthrough preflight failure and passthrough plus `--json` both leave
    stdout empty and launch no task.

Required proof: parser/help tests, isolated routing fixtures, named-source
precedence/ambiguity tests, path-resolution tests, nested task/Rhai tests,
no-side-effect rejection tests, raw subprocess byte assertions for stdin,
stdout, stderr, and exit status, JSON contract validation, and one read-only
installed-skill smoke.
