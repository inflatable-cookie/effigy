# 061 - Rhai Script Steps Guide

Use this guide when a Rust-first repo wants Effigy-native scripting instead of
another shell wrapper, Bun install, or repo-local Python glue script.

This is the front door for the Rhai-backed task step surface: when to use it,
how to declare it, what the v1 host API includes, and what it still does not
try to replace.

## Vision Alignment

- Primary tags: `ROUTE`, `CONTRACT`, `ADOPT`
- Target movement: Rust-first repos can move small automation glue into a
  bounded Effigy-native scripting surface without inventing a second runtime
  policy.

## 1) What Rhai Script Steps Are

Effigy now supports Rhai-backed run steps inside task `run = [ ... ]` arrays.

Use exactly one step entrypoint per step:

- `{ run = "..." }`
- `{ task = "..." }`
- `{ rhai = "scripts/example.rhai" }`

Rhai steps are for repo automation glue:

- small file/path transforms
- structured subprocess calls
- lightweight validation/report helpers
- Rust-first repo task glue that should not require Bun or shell

They are not the new universal runtime for every repo.

## 2) Use File-Backed Scripts

File-backed example:

```toml
[tasks.link:local]
run = [{ rhai = "scripts/rhai/install-local-bin-links.rhai" }]
```

Use `rhai = "..."` as a repo-relative Rhai script path when:

- the script is non-trivial
- you want normal file diffing/review
- the repo is building up a real native scripting surface under
  `scripts/rhai/`

## 3) Rhai v1 Host API

Current v1 helpers:

- logging:
  - `log(message)`
  - `log_warn(message)`
- context:
  - `args`
  - `cwd`
  - `repo_root`
  - `task_name`
- env and path helpers:
  - `env(name)`
  - `now_utc()`
  - `path_join(base, child)`
- file helpers:
  - `make_temp_dir(prefix)`
  - `append_file(path, contents)`
  - `read_file(path)`
  - `write_file(path, contents)`
  - `write_lines(path, lines_array)`
  - `path_exists(path)`
  - `is_file(path)`
  - `is_symlink(path)`
  - `create_dir(path)`
  - `remove_path(path)`
  - `create_symlink(target, link)`
- structured data helpers:
  - `json_parse(raw)`
  - `json_stringify(value)`
  - `toml_parse(raw)`
  - `toml_stringify(value)`
- execution helpers:
  - `stop_requested()`
  - `process_id()`
  - `sleep_ms(milliseconds)`
  - `run_process(program, args_array)`
  - `run_task(task, args_array)`

`run_process(...)` is structured subprocess execution, not shell parsing.

That means:

- good: `run_process("cargo", ["test", "--workspace"])`
- not v1: shell pipelines, shell quoting tricks, or arbitrary shell emulation

## 4) Practical Patterns

Structured file write:

```toml
[tasks.report:write]
run = [{ rhai = "scripts/rhai/write-report.rhai" }]
```

Structured process call:

```toml
[tasks.test:smoke]
run = [{ rhai = "scripts/rhai/test-smoke.rhai" }]
```

Ephemeral workspace and timestamp:

```rhai
let generated_at = now_utc();
let scratch = make_temp_dir("repo-proof");
```

Long-running lifecycle loop:

```rhai
while !stop_requested() {
    append_file("artifacts/events.log", `heartbeat ${now_utc()}\n`);
    sleep_ms(1000);
}
```

Nested task call:

```toml
[tasks.docs:proof]
run = [{ rhai = "scripts/rhai/docs-proof.rhai" }]
```

## 5) Good Boundary

Use Rhai when the repo is Rust-first and the script is mostly orchestration or
repo automation glue.

Use Bun + TypeScript when the repo is web-oriented and already lives in that
toolchain.

Keep external ecosystem tools when the job is genuinely attached to that
ecosystem:

- frontend build systems
- Electron packaging stacks
- ML/data pipelines that depend on Python-native libraries

## 6) v1 Limits

Rhai v1 intentionally does not provide:

- arbitrary shell emulation
- shell pipelines and shell quoting semantics
- network APIs
- a frontend/build-tool replacement layer
- a promise that every historical shell or Python script should disappear in
  one pass

That narrow boundary is deliberate. The product goal is “native scripting for
repo glue,” not “Effigy becomes a replacement shell.”

## Expected Outcome

After this guide, you should be able to:

- declare a Rhai-backed run step with `rhai = "path/to/script.rhai"`
- use the current v1 host API safely
- decide when Rhai is the right tool versus Bun + TS or an external ecosystem
- migrate small Rust-repo glue tasks without reintroducing shell wrappers

## Related Guides

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
- [`../roadmaps/g02/004-rust-native-scripting-surface-contract.md`](../roadmaps/g02/004-rust-native-scripting-surface-contract.md)

## Next Step

After this guide, use
[`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md) if
the next job is splitting Rhai scripts into focused manifest fragments, use
[`022-manifest-cookbook.md`](./022-manifest-cookbook.md) if you want broader
task-pattern examples, or return to the active `g02.004` spec lane when the
next move is deciding which repo migration slice should land after the
foundation.
