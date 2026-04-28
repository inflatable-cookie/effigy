# Effigy

Effigy is a repo runtime for developer work.

It gives a repo one CLI for:
- task routing
- manifest-owned local environments
- proof demos
- machine-safe JSON output
- release and distribution workflows

Instead of remembering whether something lives in `package.json`, Cargo, a
shell script, a nested workspace, or a local container stack, you ask Effigy.

## What It Does

- Run repo and nested tasks with `effigy <task>` or `effigy <catalog>/<task>`
- Inspect what a repo exposes with `effigy tasks`
- Keep local web/service stacks off the host with `effigy container ...`,
  `effigy system ...`, and `effigy workspace`
- Route local domains through the built-in gateway with TLS, DNS, and port
  management
- Define proof demos in the manifest and operate them with `effigy demo ...`
- Use stable JSON output for CI, agents, and other machine consumers
- Run release and distribution workflows from first-class built-ins instead of
  repo-local shell wrappers

## Install

Homebrew on macOS:

```bash
brew install inflatable-cookie/tap/effigy
```

Prebuilt binary on macOS or Linux:

```bash
curl -fsSL https://github.com/inflatable-cookie/effigy/releases/latest/download/effigy-$(uname -m | sed 's/arm64/aarch64/')-$(uname -s | tr A-Z a-z | sed 's/darwin/apple-darwin/;s/linux/unknown-linux-gnu/') -o /usr/local/bin/effigy && chmod +x /usr/local/bin/effigy
```

Linux note: release binaries target an Ubuntu 22.04 baseline and expect
`glibc >= 2.35`. If your distro is older, install from source instead.

From source:

```bash
cargo install --git https://github.com/inflatable-cookie/effigy --tag v0.3.0
```

## Start Fast

Initialize a manifest:

```bash
effigy init
```

Add a few obvious tasks to `effigy.toml`:

```toml
[catalog]
alias = "app"

[tasks]
dev = "bun run dev"
build = "bun run build"
"db:reset" = "./scripts/reset-db.sh"
```

Ask the repo what exists, then run it:

```bash
effigy tasks
effigy dev
effigy tasks --resolve test
effigy test --plan
```

Leave `test` to the built-in runner unless you intentionally want
`tasks.test` to override it.

If you want the guided version:
- [`021-quick-start-and-command-cookbook.md`](./docs/guides/021-quick-start-and-command-cookbook.md)
- [`055-everyday-workflows.md`](./docs/guides/055-everyday-workflows.md)

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

Use the system/container surface when a repo needs databases, queues, blob
stores or language stack workspaces without installing that full stack
directly on the machine.

```bash
effigy container up
effigy container status
effigy gateway status
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
- [`051-release-orchestration.md`](./docs/guides/051-release-orchestration.md)
- [`062-distribution-system-guide.md`](./docs/guides/062-distribution-system-guide.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./docs/guides/049-ci-binary-distribution-and-release-protocol.md)

## Shipped `v0.3` Surface

Effigy `v0.3` ships:
- task routing and manifest composition
- built-in test, doctor, watch, init, migrate, config, docs, contracts, and
  release surfaces
- local systems, workspaces, containers, and gateway-backed DNS/TLS routing
- service catalog fragments such as `workspace-rust-bun`, `php-fpm`,
  `postgres`, `mariadb`, `dbgate`, `mailpit`, `minio`, and `phpmyadmin`
- proof demos with receipts, history, artifacts, and the interactive demo
  browser
- local bundle export and repo-local bundle customization
- built-in distribution and release orchestration

For the version-by-version story, see [`CHANGELOG.md`](./CHANGELOG.md).

## Documentation

- Docs front door: [`docs/guides/README.md`](./docs/guides/README.md)
- Full command reference: [`025-command-reference-matrix.md`](./docs/guides/025-command-reference-matrix.md)
- Everyday workflows: [`055-everyday-workflows.md`](./docs/guides/055-everyday-workflows.md)
- Quick start: [`021-quick-start-and-command-cookbook.md`](./docs/guides/021-quick-start-and-command-cookbook.md)

## Working On Effigy Itself

This repo self-hosts Effigy. Common local checks:

```bash
effigy qa:ci:fast
effigy qa:ci:local
effigy release status --check-gates
```

If `effigy` is not on `PATH` yet:

```bash
cargo run --bin effigy -- bootstrap:local
```
