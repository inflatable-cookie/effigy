# Agent Instructions for Effigy

Effigy is a Rust-based unified task runner for monorepos.

## Build & Test

```bash
cargo test                    # run all tests
cargo fmt --all -- --check    # format check
cargo clippy --all-targets -- -D warnings \
  -A clippy::result_large_err \
  -A clippy::too_many_arguments \
  -A clippy::type_complexity   # lint check
```

If `effigy` is on PATH, self-hosted QA tasks are available:

```bash
effigy test --plan   # show test plan
effigy qa            # full QA (test + docs + json contracts)
effigy release gates # release-gate pass for the current repo
```

Otherwise bootstrap with `cargo run --bin effigy -- ...`.

Default local rule:
- do not add a current-directory repo override when already running inside the target repo
- use `--repo <PATH>` only when intentionally targeting a different repo

## Changelog

When making changes that affect user-facing behavior, append an entry to
`CHANGELOG.md` under the appropriate `[Unreleased]` subsection:

- **Breaking** — CLI behavior changes, config format changes, removed features
  (forces MINOR bump)
- **Added** — new features, commands, options
- **Changed** — non-breaking modifications to existing behavior
- **Fixed** — bug fixes

## Release Protocol

Agents must never initiate a release without explicit human instruction.
Full protocol: [`docs/guides/049-ci-binary-distribution-and-release-protocol.md`](./docs/guides/049-ci-binary-distribution-and-release-protocol.md)

Key rules:
- Never modify `.github/workflows/` without explicit human approval
- Never bypass release gates — fix the underlying issue instead
- Never re-tag a failed release — fix goes into the next PATCH

Preferred release command path:
- `effigy release simulate`
- `effigy release status --check-gates`
- `effigy release prepare --plan`
- `effigy release prepare --yes --check-gates`
- `effigy release execute --plan`
- `effigy release execute --yes`
- `effigy release verify-install --tag v0.__.__`
- `effigy changelog extract CHANGELOG.md --version X.Y.Z`

Compatibility backups:
- `./scripts/prepare-release.sh`
- `./scripts/check-release-gates.sh`
- `./scripts/check-release-install-from-tag.sh --tag v0.__.__`

Canonical reference:
- [`docs/guides/051-release-orchestration.md`](./docs/guides/051-release-orchestration.md)

## Key Documentation

- Guides hub: [`docs/guides/README.md`](./docs/guides/README.md)
- Task routing: [`docs/guides/016-task-routing-precedence.md`](./docs/guides/016-task-routing-precedence.md)
- JSON contracts: [`docs/guides/017-json-output-contracts.md`](./docs/guides/017-json-output-contracts.md)
- CI & release: [`docs/guides/049-ci-binary-distribution-and-release-protocol.md`](./docs/guides/049-ci-binary-distribution-and-release-protocol.md)
- Release orchestration: [`docs/guides/051-release-orchestration.md`](./docs/guides/051-release-orchestration.md)
- Agent adoption: [`docs/guides/047-agent-and-cross-repo-adoption.md`](./docs/guides/047-agent-and-cross-repo-adoption.md)
- Northstar + Effigy repo contract: [`docs/guides/056-northstar-effigy-consumer-repo-contract.md`](./docs/guides/056-northstar-effigy-consumer-repo-contract.md)
- Env schema: [`docs/guides/050-env-schema-integration.md`](./docs/guides/050-env-schema-integration.md)

## Terminology

- **selector**: a task request string (`test`, `api/test`)
- **routing**: how a selector resolves to a catalog and task
- **deferral**: fallback execution when no selector matches local tasks
