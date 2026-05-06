# Effigy

Effigy is a **unified task runner** and **repo runtime** for developer work: one
CLI discovers and runs what each repo declares, instead of you memorizing whether
something lives in `package.json`, Cargo, a shell script, a nested workspace,
or a local container stack.

Each repo opts in with **`effigy.toml`** (often split across included files).
Effigy loads that manifest, merges composition, and routes `effigy <selector>` to
the right task or built-in.

**One command to run anything in your repo:**

```bash
# See what this repo can do
effigy tasks

# Run this repo's dev task (name is defined in effigy.toml — not a special CLI verb)
effigy dev

# Run all tests, across all languages (built-in test runner unless tasks.test overrides)
effigy test
```

No more hunting through `package.json`, Makefiles, or shell scripts.

## What It Does

**Before:** New team members need to know that tests run with `npm test` in the
web directory, `cargo test` in the api directory, and `./scripts/e2e.sh` from
the repo root — and that the database needs to be running first.

**After:** `effigy test` runs everything (via the built-in test orchestration
unless the manifest defines `tasks.test`). `effigy dev` runs whatever **dev**
task the repo declared—often your whole local stack. One command from any
directory.

Effigy gives you:

- **Run repo and nested tasks** with `effigy <task>` or `effigy <catalog>/<task>`
- **Discover what's available** with `effigy tasks`
- **Keep local services off your host** with `container`, `system`, and `workspace`
- **Scriptable output** for CI and automation with `--json`
- **Proof demos and release flows** built into the same CLI instead of repo-local shell wrappers

## Install

macOS with Homebrew:

```bash
brew install inflatable-cookie/tap/effigy
```

Linux with a prebuilt binary:

```bash
curl -fsSL https://github.com/inflatable-cookie/effigy/releases/latest/download/effigy-x86_64-unknown-linux-gnu -o /usr/local/bin/effigy && chmod +x /usr/local/bin/effigy
```

Linux ARM64:

```bash
curl -fsSL https://github.com/inflatable-cookie/effigy/releases/latest/download/effigy-aarch64-unknown-linux-gnu -o /usr/local/bin/effigy && chmod +x /usr/local/bin/effigy
```

macOS direct binary:

```bash
# Apple Silicon
curl -fsSL https://github.com/inflatable-cookie/effigy/releases/latest/download/effigy-aarch64-apple-darwin -o /usr/local/bin/effigy && chmod +x /usr/local/bin/effigy

# Intel
curl -fsSL https://github.com/inflatable-cookie/effigy/releases/latest/download/effigy-x86_64-apple-darwin -o /usr/local/bin/effigy && chmod +x /usr/local/bin/effigy
```

Linux note: release binaries currently target an Ubuntu 22.04 baseline and expect
`glibc >= 2.35`. If your distro is older, install from source instead.

From source:

```bash
cargo install --git https://github.com/inflatable-cookie/effigy --tag v0.3.3
```

## Start Fast

Most people hit Effigy in one of two states:

- the repo already uses Effigy
- the repo is adopting it for the first time

If the repo already has `effigy.toml`, start here:

```bash
effigy tasks
effigy tasks --resolve test
effigy test --plan
effigy doctor --verbose
```

If the repo does not use Effigy yet, start here:

```bash
effigy init
```

For a brand-new repo, add a few obvious tasks to `effigy.toml`:

```toml
[catalog]
alias = "app"

[tasks]
dev = "bun run dev"
build = "bun run build"
"db:reset" = "./scripts/reset-db.sh"
```

Then ask the repo what exists, and run something small:

```bash
effigy tasks
effigy tasks --resolve test
effigy test --plan
effigy dev
```

Leave `test` to the built-in runner unless you intentionally want
`tasks.test` to override it.

## Common Paths

### Run tasks without hunting for them

```bash
effigy tasks
effigy tasks --resolve test
effigy api/build
effigy --json tasks
```

Read next:
- [`021-quick-start-and-command-cookbook.md`](./docs/guides/021-quick-start-and-command-cookbook.md)
- [`055-everyday-workflows.md`](./docs/guides/055-everyday-workflows.md)

### Shape repo behavior in `effigy.toml`

```bash
effigy init
effigy migrate --from package.json
effigy config --inspect
effigy config --schema --minimal
```

Read next:
- [`022-manifest-cookbook.md`](./docs/guides/022-manifest-cookbook.md)
- [`059-manifest-composition-guide.md`](./docs/guides/059-manifest-composition-guide.md)

### Keep local services off your host

Use containers when a repo needs databases, queues, or language workspaces
without installing that full stack directly on your machine.

```bash
effigy container up
effigy gateway status
effigy dev   # still the repo's dev task; containers are started separately
```

Read next:
- [`063-container-system-guide.md`](./docs/guides/063-container-system-guide.md)
- [`064-system-workspace-and-dev-contract.md`](./docs/guides/064-system-workspace-and-dev-contract.md)

### More paths

For demos, CI automation, release workflows, and advanced topics, see the
[docs front door](docs/README.md).

## Agent Skill

Cross-repo agent skill teaches Claude Code, OpenAI Codex, Cursor (and 50+
other agents) how to use Effigy in any repo:

```bash
npx skills add inflatable-cookie/effigy -g
```

Drop `-g` for project-local install. Source: [`skills/effigy/`](./skills/effigy/).

## Documentation

- **Goal paths (read this next):** [`docs/README.md`](./docs/README.md) — short
  “choose your job” links into the guides.
- **Quick start:** [`021-quick-start-and-command-cookbook.md`](./docs/guides/021-quick-start-and-command-cookbook.md)
- **Everyday workflows:** [`055-everyday-workflows.md`](./docs/guides/055-everyday-workflows.md)
- **Full practical guide map** (inventory + journeys): [`docs/guides/README.md`](./docs/guides/README.md)
- **Command lookup:** [`025-command-reference-matrix.md`](./docs/guides/025-command-reference-matrix.md)
- **Release history:** [`CHANGELOG.md`](./CHANGELOG.md)

## Contributing to this repository

The **Effigy** source tree uses its own `effigy.toml` tasks (names like `qa:…`
exist only here). For day-to-day work on the project, not for consumers of the
tool in other repos:

```bash
effigy qa:ci:fast
effigy qa:ci:local
effigy release status --check-gates
```

If `effigy` is not on `PATH` yet:

```bash
cargo run --bin effigy -- bootstrap:local
```

Local maintainer install and PATH-first usage:
- [`010-path-installation-and-release.md`](./docs/guides/010-path-installation-and-release.md)
