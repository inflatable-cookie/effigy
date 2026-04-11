# Effigy

Effigy gives a repo one way to ask for work.

Instead of remembering whether a task lives in `package.json`, Cargo, a shell
script, or a nested workspace, you use one CLI for task routing, built-in
workflows, and automation-safe output.

The goal is simple: common repo work should feel direct. When a workflow still
needs too much ceremony, the answer should usually be to improve Effigy or the
manifest, not to teach people more wrapper scripts.

## What Effigy Puts Up Front

- See what a repo can do with `effigy tasks`.
- Run local or nested tasks with `effigy <task>` or `effigy <catalog>/<task>`.
- Standardize everyday workflows with built-ins such as `effigy doctor`,
  `effigy test`, `effigy watch`, `effigy init`, and `effigy migrate`.
- Move CI and agent automation onto stable JSON with `effigy --json <command>`.
- Replace scattered release and validation scripts with built-in command
  surfaces as adoption grows.

## Install

Homebrew (macOS):

```bash
brew install inflatable-cookie/tap/effigy
```

Prebuilt binary (macOS and Linux):

```bash
curl -fsSL https://github.com/inflatable-cookie/effigy/releases/latest/download/effigy-$(uname -m | sed 's/arm64/aarch64/')-$(uname -s | tr A-Z a-z | sed 's/darwin/apple-darwin/;s/linux/unknown-linux-gnu/') -o /usr/local/bin/effigy && chmod +x /usr/local/bin/effigy
```

Linux note: GNU/Linux release binaries are built against an Ubuntu 22.04
baseline and should run on systems with `glibc >= 2.35`. If your distro is
older, use `cargo install` instead.

From source:

```bash
cargo install --git https://github.com/inflatable-cookie/effigy --tag v0.2.12
```

## Start In 5 Minutes

1. Scaffold a starter manifest:

```bash
effigy init
```

2. Add a few obvious tasks:

```toml
[catalog]
alias = "app"

[tasks]
dev = "bun run dev"
"db:reset" = "./scripts/reset-db.sh"
build = "bun run build"
```

3. Ask the repo what exists, then run it:

```bash
effigy tasks
effigy dev
effigy test --plan
effigy tasks --resolve test
```

Leave `test` to the built-in runner unless you intentionally want
`tasks.test` to override it.

If you want the guided version of that flow, start with
[`021-quick-start-and-command-cookbook.md`](./docs/guides/021-quick-start-and-command-cookbook.md).

To clone and bootstrap a repo in one shot from another directory:

```bash
effigy bootstrap git@github.com:inflatable-cookie/effigy.git
```

## Main Workflows

### Find and run work

Use Effigy when you want one entrypoint for repo tasks instead of hunting
through package managers and subdirectories.

```bash
effigy tasks
effigy tasks --resolve test
effigy api/build
```

Details:
- [`055-everyday-workflows.md`](./docs/guides/055-everyday-workflows.md)
- [`016-task-routing-precedence.md`](./docs/guides/016-task-routing-precedence.md)
- [`022-manifest-cookbook.md`](./docs/guides/022-manifest-cookbook.md)

### Keep the repo healthy

Use built-ins when you want consistent health, test, and watch flows instead of
custom shell glue.

```bash
effigy doctor --verbose
effigy test --plan
effigy watch --owner effigy --once test
effigy scan god-files
```

Details:
- [`018-doctor-explain-mode.md`](./docs/guides/018-doctor-explain-mode.md)
- [`019-watch-init-migrate-foundation.md`](./docs/guides/019-watch-init-migrate-foundation.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./docs/guides/048-built-in-test-suite-lifecycle-and-env.md)
- [`023-troubleshooting-and-failure-recipes.md`](./docs/guides/023-troubleshooting-and-failure-recipes.md)

### Shape the manifest instead of more scripts

Use `effigy.toml` to make common setup, env, test, and task routing explicit.

```bash
effigy init
effigy migrate --from package.json
effigy config --schema --minimal
effigy config --inspect
```

Details:
- [`022-manifest-cookbook.md`](./docs/guides/022-manifest-cookbook.md)
- [`050-env-schema-integration.md`](./docs/guides/050-env-schema-integration.md)
- [`028-migration-quick-paths.md`](./docs/guides/028-migration-quick-paths.md)

For managed multi-process stacks, `concurrent` entries can also declare
`shutdown_on_exit = true` when one process should act as the lifecycle root for
the whole session, such as an Electron main window closing the rest of the dev
stack.

When one `effigy.toml` starts carrying too many unrelated concerns, split it
with `[manifest].include = [...]` and use `effigy config --inspect` to verify
the effective merged result before relying on it in CI or docs-policy flows.

### Automate safely

Use JSON mode and contract docs when Effigy is feeding CI, agents, or other
tools.

```bash
effigy --json tasks
effigy --json doctor
effigy --json test --plan
```

Details:
- [`017-json-output-contracts.md`](./docs/guides/017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](./docs/guides/024-ci-and-automation-recipes.md)
- [`026-json-payload-examples.md`](./docs/guides/026-json-payload-examples.md)
- [`047-agent-and-cross-repo-adoption.md`](./docs/guides/047-agent-and-cross-repo-adoption.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./docs/guides/056-northstar-effigy-consumer-repo-contract.md)

When you want a repo to adopt the full Northstar + Effigy flow, keep the
bootstrap/scaffolding logic in the `northstar-effigy` skill and use Effigy to
validate the resulting contract with native docs, QA, and release surfaces.

### Release from built-ins

Use the release surface when you want preview-first, repeatable release work
without re-inventing the workflow in shell.

```bash
effigy release status --check-gates
effigy release prepare --plan
effigy release execute --plan
```

Details:
- [`051-release-orchestration.md`](./docs/guides/051-release-orchestration.md)
- [`052-changelog-workflows-and-northstar-profile.md`](./docs/guides/052-changelog-workflows-and-northstar-profile.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./docs/guides/049-ci-binary-distribution-and-release-protocol.md)

## Documentation Paths

- New to Effigy:
  [`021-quick-start-and-command-cookbook.md`](./docs/guides/021-quick-start-and-command-cookbook.md)
- Want the most common day-to-day flows:
  [`055-everyday-workflows.md`](./docs/guides/055-everyday-workflows.md)
- Writing or cleaning up `effigy.toml`:
  [`022-manifest-cookbook.md`](./docs/guides/022-manifest-cookbook.md)
- Need the full command surface:
  [`025-command-reference-matrix.md`](./docs/guides/025-command-reference-matrix.md)
- Need the full docs map:
  [`docs/README.md`](./docs/README.md)
- Need practical guide navigation:
  [`docs/guides/README.md`](./docs/guides/README.md)

## Working On Effigy Itself

This repo self-hosts a root `effigy.toml`, so product development uses Effigy
for its own QA and release flows.

```bash
effigy test --plan
effigy qa
effigy release gates
```

If `effigy` is not yet on `PATH`, bootstrap from the checkout:

```bash
cargo run --bin effigy -- bootstrap:local
```

Compatibility fallbacks remain available for callers that still need them:

```bash
cargo qa
cargo qa-docs
cargo qa-json
cargo qa-release
```

## Current Planning Posture

Effigy's active product lane is `g02.003`.

Use these surfaces before continuing manifest-composition or demo-harness work:

- [`docs/roadmaps/README.md`](./docs/roadmaps/README.md)
- [`docs/roadmaps/g02/002-manifest-composition-and-override-contract.md`](./docs/roadmaps/g02/002-manifest-composition-and-override-contract.md)
- [`docs/specs/README.md`](./docs/specs/README.md)
- [`docs/contracts/001-working-rules.md`](./docs/contracts/001-working-rules.md)

## Repository Layout

```text
effigy/
├── src/
├── docs/
│   ├── architecture/
│   ├── contracts/
│   ├── guides/
│   ├── logs/
│   ├── research/
│   ├── roadmaps/
│   └── vision/
└── Cargo.toml
```

## Next Task

Use the active `g02.003` ready card to define the first bounded demo-runner
implementation slice, using the settled Signal reconciliation as the calibration
point.
