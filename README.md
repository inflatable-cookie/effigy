# Effigy

Effigy is a repo runtime for developer work.

Instead of remembering whether something lives in `package.json`, Cargo, a
shell script, a nested workspace, or a local container stack, you ask Effigy.

## What It Does

- Run repo and nested tasks with `effigy <task>` or `effigy <catalog>/<task>`
- Show what a repo exposes with `effigy tasks`
- Keep local web and service stacks off the host with `container`, `system`,
  and `workspace`
- Define proof demos, machine-safe JSON output, and release flows in the same
  CLI instead of repo-local shell wrappers

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
cargo install --git https://github.com/inflatable-cookie/effigy --tag v0.3.0
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
- [`016-task-routing-precedence.md`](./docs/guides/016-task-routing-precedence.md)
- [`022-manifest-cookbook.md`](./docs/guides/022-manifest-cookbook.md)

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
- [`050-env-schema-integration.md`](./docs/guides/050-env-schema-integration.md)

### Keep web-service dependencies off the host

Use the system/container tools when a repo needs databases, queues, blob
stores, or language stack workspaces without installing that full stack
directly on the machine.

```bash
effigy container up
effigy gateway status
effigy container status
effigy dev
```

Read next:
- [`063-container-system-guide.md`](./docs/guides/063-container-system-guide.md)
- [`064-system-workspace-and-dev-contract.md`](./docs/guides/064-system-workspace-and-dev-contract.md)
- [`065-underlay-starter.md`](./docs/guides/065-underlay-starter.md)
- [`067-catalog-services-reference.md`](./docs/guides/067-catalog-services-reference.md)
- [`069-workspace-host-integration.md`](./docs/guides/069-workspace-host-integration.md)

### Define and run proof demos

```bash
effigy demo list
effigy demo inspect <id>
effigy demo run <id>
effigy demo history <id>
effigy demo browser
```

Read next:
- [`058-demo-system-guide.md`](./docs/guides/058-demo-system-guide.md)
- [`060-consumer-demo-migration-guide.md`](./docs/guides/060-consumer-demo-migration-guide.md)

### Automate safely

```bash
effigy --json tasks
effigy --json doctor
effigy --json test --plan
```

Read next:
- [`017-json-output-contracts.md`](./docs/guides/017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](./docs/guides/024-ci-and-automation-recipes.md)
- [`026-json-payload-examples.md`](./docs/guides/026-json-payload-examples.md)

### Release from built-ins

```bash
effigy release status --check-gates
effigy release prepare --plan
effigy release execute --plan
```

Read next:
- [`051-release-orchestration.md`](./docs/guides/051-release-orchestration.md) for the release cut flow
- [`062-distribution-system-guide.md`](./docs/guides/062-distribution-system-guide.md) for distribution commands and evidence
- [`049-ci-binary-distribution-and-release-protocol.md`](./docs/guides/049-ci-binary-distribution-and-release-protocol.md) for maintainer policy

## Documentation

- Start here: [`docs/README.md`](./docs/README.md)
- Quick start: [`021-quick-start-and-command-cookbook.md`](./docs/guides/021-quick-start-and-command-cookbook.md)
- Everyday workflows: [`055-everyday-workflows.md`](./docs/guides/055-everyday-workflows.md)
- Docs front door: [`docs/guides/README.md`](./docs/guides/README.md)
- Full command reference: [`025-command-reference-matrix.md`](./docs/guides/025-command-reference-matrix.md)
- Release history: [`CHANGELOG.md`](./CHANGELOG.md)

## Working On Effigy Itself

This repo uses Effigy itself. Common local checks:

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
