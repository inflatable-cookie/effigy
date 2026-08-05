# Effigy

Stop memorizing whether tests run with `npm test`, `cargo test`, or `./scripts/e2e.sh`. Stop telling new hires they need Postgres running before the dev server starts. Stop maintaining shell wrappers that only work on your machine.

**Effigy is a single CLI that controls your project.** One manifest (`effigy.toml`) declares what your repo can do. One command runs it from anywhere.

```bash
# See what this repo can do
effigy tasks

# Run the dev stack (however the repo defines it)
effigy dev

# Run all tests across every language and framework in the repo
effigy test
```

That's it. If the repo uses Effigy, you already know how to work in it.

---

## What Effigy Actually Does

**Before:** You `cd` into `web/`, run `npm test`, then `cd ../api`, run `cargo test`, then remember the e2e suite needs `./scripts/e2e.sh` and a running database.

**After:** `effigy test` runs everything. `effigy dev` starts your whole local stack. `effigy demo run login-smoke` proves the login flow still works. `effigy release prepare` cuts a release without you touching version files. One command from any directory.

Effigy is not another task runner. It is a **repo runtime** — the layer between you and the chaos of modern polyglot repos. It is also **agent-native**: the manifest is self-describing, every command speaks JSON, and an AI agent can discover the repo surface, inspect code, execute work, and validate changes through one structured CLI.

---

## Install

```bash
# macOS
brew install inflatable-cookie/tap/effigy

# Linux (x86_64)
curl -fsSL https://github.com/inflatable-cookie/effigy/releases/latest/download/effigy-x86_64-unknown-linux-gnu -o /usr/local/bin/effigy && chmod +x /usr/local/bin/effigy

# From source
cargo install --git https://github.com/inflatable-cookie/effigy
```

---

## Start in 60 Seconds

Already in a repo with `effigy.toml`?

```bash
effigy doctor             # Is the repo healthy and routable?
effigy tasks              # What can this repo do?
effigy test --plan        # See what tests would run
effigy dev                # Start the local stack
```

Adopting Effigy for the first time?

```bash
effigy init               # TTY wizard on a terminal; idempotent baseline setup otherwise
effigy init --check --json
effigy init --checklist --json
```

Then add a few tasks:

```toml
[catalog]
alias = "app"

[tasks]
dev = "bun run dev"
build = "bun run build"
"db:reset" = "./scripts/reset-db.sh"
```

Run them:

```bash
effigy dev
effigy db:reset
```

Leave `test` alone — Effigy will auto-detect your test runner (`vitest`, `cargo nextest`, `cargo test`) and orchestrate across your whole repo.

---

## Power Features

### Task Chains with Dependencies

Not just aliases. Real orchestration:

```toml
[tasks.validate]
run = [
  { id = "lint", run = "bun run lint" },
  { id = "unit", task = "test", depends_on = ["lint"], timeout_ms = 180000 },
  { id = "contract", run = "cargo test --workspace", depends_on = ["lint"] },
]
```

Run `effigy validate`. It runs lint first, then unit tests and contract tests in parallel, with a 3-minute timeout.

### Nested Catalogs (Monorepo-Native)

Drop an `effigy.toml` in any subdirectory. Effigy discovers it automatically and treats it as a named catalog:

```
repo/
├── effigy.toml          # root catalog
├── api/
│   └── effigy.toml      # catalog alias = "api"
└── web/
    └── effigy.toml      # catalog alias = "web"
```

Run tasks from anywhere without `cd`:

```bash
effigy api/build         # Explicit: run 'build' in the api catalog
effigy web/test          # Explicit: run 'test' in the web catalog
```

Better: if a task name is unique across all catalogs, you can call it bare and Effigy routes it for you:

```bash
effigy db:reset          # Only defined in api/ — routed to api/db:reset automatically
effigy lint              # Only defined in web/ — routed to web/lint automatically
```

Use the prefix only when you need to be explicit or when the same name exists in multiple places. Check routing anytime:

```bash
effigy tasks             # Every task in every catalog, with aliases
effigy tasks --resolve db:reset    # "This will run api/db:reset"
```

Root tasks can orchestrate across sub-catalogs:

```toml
[tasks.validate]
run = [
  { task = "api/test" },
  { task = "web/test" },
  { run = "./scripts/e2e.sh" },
]
```

Effigy handles routing, working-directory context, and result aggregation. No turbo.json, no pnpm workspaces, no `cd` dance. Just tasks, discovered and callable by name.

### Built-In Repo Scans

Effigy ships with scanners that catch drift before it becomes a problem. No external tools, no configuration required:

```bash
effigy doctor --verbose              # Run all enabled scans + health check
effigy scan god-files                # Find files that grew too large
effigy scan comment-ratio            # Spot files with suspicious comment balance
effigy scan generated-in-src         # Catch generated files polluting source trees
effigy scan attention-markers        # Surface TODO/FIXME/SECURITY markers
```

Each scanner respects `.gitignore`, supports `--json` for CI integration, and can be configured in `effigy.toml` with thresholds, include/exclude globs, and automatic `doctor` inclusion. Findings write to `.effigy/reports/doctor/` as markdown for review.

For scripts and agents, `effigy --json <command>` emits stable envelopes (see
`docs/guides/017-json-output-contracts.md`). Manifest tasks also accept a
leading `--repo`, `--env-schema <PATH>`, or `--verbose-root` before task-specific
arguments when you need an alternate root, schema file, or broader diagnostics
(`docs/guides/050-env-schema-integration.md`).

### Built-In Test Orchestration

`effigy test` is not just a passthrough. It auto-detects your test runners and orchestrates them across your whole repo:

```bash
effigy test --plan          # See what will run before it runs
effigy test                 # Auto-detect and run everything
effigy test vitest          # Run only the vitest suite
effigy test --verbose-results
```

Effigy detects `vitest`, `cargo nextest`, and `cargo test` per catalog root. From a workspace root it fans out across all discovered catalogs and aggregates results, running up to 3 in parallel by default.

For suites that need setup and teardown, declare them in the manifest:

```toml
[test.suites.integration]
run = "cargo nextest run --workspace"
env = "TEST_DATABASE_URL"
env_file = ".env"
setup = [
  { run = "cargo run -p app-db --bin migrate_test_db" },
]
teardown = [
  { run = "cargo run -p app-db --bin reset_test_db" },
]
teardown_policy = "always"
```

Setup runs before the suite, teardown runs after — even if the tests fail. No more wrapper scripts just to reset a test database.

### Rust-First Scripting with Rhai

Stop writing shell wrappers for file transforms, HTTP calls, or structured subprocess orchestration. Effigy embeds Rhai — a Rust-native scripting language — as a first-class task step:

```toml
[tasks.seed]
run = { rhai = "scripts/seed-db.rhai" }
```

```rust
// scripts/seed-db.rhai
let db_url = env("DATABASE_URL");
let response = http::get("https://api.example.com/seed-data");
let data = json::parse(response.body);

for item in data.records {
    process::run("psql", [db_url, "-c", "INSERT ..."]);
    log("Seeded: " + item.id);
}
```

The host API gives you files (`fs::read_file`, `fs::write_file`, `fs::replace_in_file`), HTTP (`http::get`, `http::post`), JSON/TOML parsing, process execution (`process::run`, `process::stream`), container control (`container::up`, `container::exec`), task invocation (`task::run`), and more — all in a typed, reviewable script file.

Use Rhai when the logic is too complex for a one-liner but too small to justify a Bun install or Python dependency.

### Host-Clean Local Dev Stacks

Declare services and run your dev task inside a container:

```toml
[containers.app]
services.db.catalog = "postgres"
services.cache.catalog = "redis"

[tasks.dev]
run_in = "container"
run = "bun run dev"
```

```bash
effigy dev
```

Effigy auto-starts the container with Postgres and Redis, runs your dev task inside it, and stops everything on exit. No `docker compose up`, no manual teardown, no host pollution.

Prerequisites: Colima or Docker Desktop must be installed (e.g. `brew install colima`).

For one-off maintenance inside the same environment:

```bash
effigy workspace         # Shell inside the container
# ... run migrations, install packages, etc.
exit
```

### Concurrent Managed Sessions

Any task can become a managed multi-process session. Define what runs together and Effigy gives you one terminal with tabs, process ownership, and a built-in shell:

```toml
[tasks.dev]
mode = "tui"

concurrent = [
  { task = "api", start = 1, tab = 1 },
  { task = "jobs", start = 2, tab = 2, start_after_ms = 1200 },
  { task = "frontend", start = 3, tab = 3, shutdown_on_exit = true },
  { run = "my-other-process", start = 4, tab = 4 },
  { task = "shell", start = 5, tab = 5 }
]
```

Run `effigy dev` (or whatever task name you chose). One TUI. Five tabs. Effigy starts services in order, restarts crashed processes, and shuts the whole session down cleanly when you quit. No `tmux` rituals, no PID files.

### Local HTTPS Gateway

Stop editing `/etc/hosts` and memorising port numbers. Effigy's built-in gateway gives your local stack real domains with automatic TLS:

```toml
[containers.app.dns]
domain = "my-app.test"
tls = true
```

```bash
effigy gateway up       # Start the local DNS + proxy
effigy dev              # Your app is now at https://my-app.test
```

Effigy runs a local DNS server and reverse proxy. `*.test` domains resolve automatically. TLS certificates are generated on-demand via `mkcert` — one-time `mkcert -install` and your browser trusts them forever. No certificate warnings, no `:3000` port suffixes, no host file hacks.

### First-Class Demos and Proofs

Turn scattered QA scripts into discoverable, runnable proof entries:

```toml
[demos.login-smoke]
title = "Login smoke"
run = "python3 demos/run_login_smoke.py"
status = "ready"
```

```bash
effigy demo list
effigy demo run login-smoke
effigy demo browser      # Interactive TUI
```

### Release Without the Ritual

```bash
effigy release simulate         # Dry-run: what would happen?
effigy release prepare --plan   # Preview file mutations
effigy release prepare --yes    # Bump version, update changelog
effigy release execute --yes    # Commit, tag, push
```

No more hand-editing `package.json` and `CHANGELOG.md` and hoping you didn't miss a step.

### JSON Output for CI

Every command speaks JSON:

```bash
effigy --json tasks | jq '.tasks[].name'
effigy --json test --plan | jq '.selected_runner'
effigy --json doctor          # Machine-readable health check
```

### Watch Mode That Isn't a Footgun

```bash
effigy watch --owner effigy --once test   # Run tests on change, once
effigy watch --max-runs 5 dev            # Bounded, explicit, composable
```

### One-Command Repo Bring-Up

Give a git URL. Effigy clones, runs the repo's declared setup, and starts the dev stack:

```bash
effigy bootstrap git@github.com:acme/app.git --plan    # Preview first
effigy bootstrap git@github.com:acme/app.git           # Clone, setup, start
```

The repo declares its own bring-up in `effigy.toml`:

```toml
[bootstrap]
run = { task = "bootstrap deps sync" }
start = "dev"
```

If the repo has a `.gitmodules` file, bootstrap initializes and updates those
submodules recursively before running setup. Set `submodules = "none"` only when
bootstrap should skip them.

Need companion repos alongside it? Declare child repos in the same manifest:

```toml
[[bootstrap.children]]
path = "shared-lib"
repo = "git@github.com:acme/shared-lib.git"
run = { task = "bootstrap deps sync" }
required = true

[[bootstrap.children]]
path = "ops-tools"
repo = "git@github.com:acme/ops-tools.git"
run = { task = "bootstrap deps sync" }
required = false
```

One command clones the root repo, clones every child into the right relative paths, runs each repo's setup, and starts the dev stack. After the initial bring-up, keep children in sync:

```bash
effigy bootstrap children status   # See what's ahead/behind or missing
effigy bootstrap children sync     # Pull and fast-forward all children
```

Effigy itself keeps adjacent provider, bundle, and action repos under
`external/` as Git submodules. In a normal checkout, hydrate them with:

```bash
git submodule update --init --recursive
```

Pass `--db-seed ./latest.sql` to stage a database dump before setup runs. Pass `--fresh` to isolate runtime state from earlier local runs. One command, one consistent path, no local folklore.

### Manifest Composition

Split `effigy.toml` across focused files instead of one dumping ground:

```toml
[manifest]
include = [
  "config/tasks.toml",
  "config/containers.toml",
  { path = "effigy.local.toml", override = ["tasks.dev"] },
]
```

---

## Discover the Full Surface

| You want to... | Read this |
|---|---|
| Get started, run daily commands | [`docs/guides/021-quick-start-and-command-cookbook.md`](./docs/guides/021-quick-start-and-command-cookbook.md) |
| See common workflows | [`docs/guides/055-everyday-workflows.md`](./docs/guides/055-everyday-workflows.md) |
| Copy-paste manifest patterns | [`docs/guides/022-manifest-cookbook.md`](./docs/guides/022-manifest-cookbook.md) |
| Run containers, workspaces, local domains | [`docs/guides/063-container-system-guide.md`](./docs/guides/063-container-system-guide.md) |
| Build demos and proofs | [`docs/guides/058-demo-system-guide.md`](./docs/guides/058-demo-system-guide.md) |
| Cut releases | [`docs/guides/051-release-orchestration.md`](./docs/guides/051-release-orchestration.md) |
| Wire up CI | [`docs/guides/024-ci-and-automation-recipes.md`](./docs/guides/024-ci-and-automation-recipes.md) |
| Test against local library edits | [`docs/guides/077-local-dependency-linking.md`](./docs/guides/077-local-dependency-linking.md) |
| Agent repo map / code graph | [`docs/guides/076-code-graph-and-agent-workflows.md`](./docs/guides/076-code-graph-and-agent-workflows.md) |
| Full guide map | [`docs/guides/README.md`](./docs/guides/README.md) |

Command reference: [`docs/guides/025-command-reference-matrix.md`](./docs/guides/025-command-reference-matrix.md)

---

## Built for Agents

Effigy was designed so that AI agents can operate a repo without tribal knowledge. The manifest is the contract, the CLI is the API, and the core workflow is explicit:

```bash
effigy doctor --json             # Repo health and routing diagnostics
effigy tasks --json              # Every task, catalog, and description
effigy test --plan --json        # Resolved test plan without execution
effigy graph explore "<task>" --json   # Bounded code-understanding packet
effigy <task>                    # Execute supported repo work
effigy --help                    # Every command and flag
effigy release simulate --json   # Machine-readable dry-runs
```

An agent dropped into an Effigy repo can start with `doctor`, `tasks`, and `test --plan`, switch to `effigy graph` when the job is code understanding, then run tests, start dev environments, and cut releases through structured CLI output. No grepping Makefiles, no parsing package.json scripts, no guessing which directory to `cd` into.

Graph workflow guide: [`docs/guides/076-code-graph-and-agent-workflows.md`](./docs/guides/076-code-graph-and-agent-workflows.md)

For agents that want a head start, we ship a skill that teaches Claude Code, OpenAI Codex, Cursor, and 50+ other agents the exact patterns:

```bash
npx skills add inflatable-cookie/effigy -g
```

---

## Contributing

This repo uses its own `effigy.toml` tasks:

```bash
effigy qa                  # Full QA pass
effigy release gates       # Check release readiness
```

If `effigy` is not on `PATH` yet:

```bash
cargo run --bin effigy -- bootstrap:local
```
