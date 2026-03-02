# 021 - Quick Start and Command Cookbook

This guide is the shortest path from install to useful daily commands.

Use this page for first-run workflows. Use [`025-command-reference-matrix.md`](./025-command-reference-matrix.md) for full command/flag lookup.

## 1) Quick Start (5 Minutes)

Run from source:

```sh
cargo run --bin effigy -- --help
cargo run --bin effigy -- tasks
```

Install on PATH:

```sh
cargo install --path .
effigy --help
```

Create starter manifest if needed:

```sh
effigy init
effigy tasks
```

## 2) Minimal `effigy.toml`

```toml
[catalog]
alias = "app"

[tasks]
dev = "bun run dev"
api = "cargo run -p api"
"db:reset" = "./scripts/reset-db.sh"
```

Run:

```sh
effigy dev
effigy app/db:reset
```

More manifest patterns: [`022-manifest-cookbook.md`](./022-manifest-cookbook.md).

## 3) Daily Operator Commands

Task discovery and routing:

```sh
effigy tasks
effigy tasks --resolve test
```

Workspace health and explain mode:

```sh
effigy doctor --verbose
effigy doctor --repo /path/to/workspace app/build -- --watch
```

Built-in test orchestration:

```sh
effigy test --plan
effigy test vitest
```

Watch mode (bounded run for automation):

```sh
effigy watch --owner effigy --once test
effigy watch --owner effigy --max-runs 2 --json test vitest
```

Lock recovery:

```sh
effigy unlock task:watch:test
effigy unlock --all
```

Shell completion:

```sh
effigy completion bash > ~/.local/share/bash-completion/completions/effigy
effigy completion zsh > ~/.zfunc/_effigy
effigy completion fish > ~/.config/fish/completions/effigy.fish
effigy completion candidates --prefix farm
```

Completion-candidate memoization TTL can be tuned with `EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS` (ms, bounded `100..60000`, default `2000`).

Completion cache troubleshooting (JSON):

```sh
effigy --json completion candidates --prefix farm
```

Check these fields:
- `cache_state` (`miss_initial`, `hit`, `miss_ttl`, `miss_manifest_change`)
- `cache_ttl_source` (`default`, `env`, `env_invalid`)
- `effective_cache_ttl_ms` (active TTL policy)
- `cache_age_ms` and `cache_ttl_ms` (`hit` responses)

## 4) JSON Mode (Automation-Safe)

Canonical JSON mode:

```sh
effigy --json <command>
```

Examples:

```sh
effigy --json tasks
effigy --json doctor
effigy --json test --plan
```

Envelope schema: `effigy.command.v1`.
Payload examples: [`026-json-payload-examples.md`](./026-json-payload-examples.md).

## 5) Choose the Right Guide

- Need manifest authoring patterns: [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- Need symptom-based diagnosis: [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- Need CI recipes and contract gates: [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- Need command/flag matrix: [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- Need migration paths by scenario: [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)

## 6) Next Step

After completing this quick start, run the docs QA checklist in [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md) before shipping automation or contract-related changes.
