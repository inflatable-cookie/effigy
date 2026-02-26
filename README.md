# effigy

Effigy is a unified task runner for multi-repo and nested-workspace development.

It provides:
- built-in operational tasks (starting with `repo-pulse`),
- project-defined tasks in TOML catalogs,
- deterministic task resolution across nested catalogs,
- explicit catalog targeting (`catalog:task`) and unprefixed intelligent resolution (`task`).

## Why Effigy

In large workspaces, scripts drift across `package.json`, shell wrappers, and ad hoc per-repo commands. Effigy provides one runner surface with consistent behavior:

- one command surface for humans and automation,
- catalog-based task ownership by repo/sub-repo,
- location-aware resolution so unprefixed tasks behave predictably,
- no forced coupling to Node package scripts.

## Install and Run

Current development mode (recommended while iterating):

```bash
cargo run --manifest-path /abs/path/to/effigy/Cargo.toml --bin effigy -- tasks
```

Typical workspace helper script:

```json
{
  "scripts": {
    "effigy": "cargo run --manifest-path ../effigy/Cargo.toml --bin effigy --"
  }
}
```

Then use:

```bash
bun effigy tasks
bun effigy repo-pulse
bun effigy farmyard:reset-db
```

Planned steady-state:
- install `effigy` on PATH and run `effigy ...` directly,
- keep `bun effigy ...` wrapper as a fallback compatibility surface.

## Command Contract

```bash
effigy <task> [task args]
effigy <catalog>:<task> [task args]
effigy repo-pulse [--repo <PATH>] [--verbose-root]
effigy tasks [--repo <PATH>] [--task <TASK_NAME>]
```

### Built-in tasks
- `repo-pulse`: repository/workspace health and structure signal report.
- `tasks`: enumerate discovered catalogs and task commands.

## Task Catalogs

Canonical manifest name:
- `effigy.tasks.toml`

Compatibility fallback:
- `underlay.tasks.toml` is discovered only when `effigy.tasks.toml` is not present in the same catalog directory.

Example:

```toml
[catalog]
alias = "farmyard"

[tasks.reset-db]
run = "cargo run -p farmyard-db --bin reset_dev_db {args}"
```

Interpolation tokens:
- `{repo}`: resolved catalog root path shell-quoted.
- `{args}`: task passthrough args shell-quoted and joined.

### Deferral fallback

Catalogs can define a fallback process used when no named task matches:

```toml
[defer]
run = "composer global run effigy {request} {args}"
```

Deferral runs only for unresolved task requests and receives:
- `{request}`: original task request (`task` or `catalog:task`)
- `{args}`: passthrough args
- `{repo}`: catalog root path

## Resolution Model

Root resolution:
1. use explicit `--repo` when provided,
2. otherwise detect nearest marker root from cwd (`package.json`, `Cargo.toml`, `.git`),
3. optionally promote to parent workspace when membership signals indicate it.

Task resolution:
1. explicit prefix (`catalog:task`) selects one catalog,
2. unprefixed selects nearest in-scope catalog if cwd is inside matching catalogs,
3. otherwise chooses shallowest depth from workspace root,
4. ties fail explicitly as ambiguous.

## Repository Layout

```text
effigy/
├── src/
│   ├── bin/effigy.rs
│   ├── runner.rs
│   ├── resolver.rs
│   └── tasks/
├── docs/
│   ├── architecture/
│   ├── roadmap/
│   └── reports/
└── Cargo.toml
```

## Development

Run tests:

```bash
cargo test
```

## Documentation System

Effigy uses the same style as Underlay:
- numbered roadmap phases with explicit task checklists,
- architecture docs as stable source-of-truth,
- dated reports capturing sweeps, validation, and checkpoints.

Start here:
- `docs/architecture/`
- `docs/guides/010-path-installation-and-release.md`
- `docs/roadmap/README.md`
- `docs/reports/README.md`
