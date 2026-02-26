# effigy
Unified task runner with nested task catalog resolution.

## Usage

```bash
cargo run --bin effigy -- tasks
cargo run --bin effigy -- pulse --repo .
cargo run --bin effigy -- <task> [args...]
cargo run --bin effigy -- <catalog>:<task> [args...]
```

## Task Catalogs

- Canonical file: `effigy.tasks.toml`
- Legacy compatibility: `underlay.tasks.toml` is still discovered when an `effigy.tasks.toml` is not present in the same catalog directory.
