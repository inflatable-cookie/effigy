# Agent Instructions for Effigy

Effigy is a Rust-based unified task runner for monorepos (v0.2.0).

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
effigy test --plan --repo .   # show test plan
effigy qa --repo .            # full QA (test + docs + json contracts)
effigy qa:release --repo .    # release gates
```

Otherwise bootstrap with `cargo run --bin effigy -- ...`.

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
- Never move workflows from `.github-bak/` to `.github/` without human instruction
- Never bypass release gates — fix the underlying issue instead
- Never re-tag a failed release — fix goes into the next PATCH

Use `./scripts/prepare-release.sh` to check the recommended version bump.

## Key Documentation

- Guides hub: [`docs/guides/README.md`](./docs/guides/README.md)
- Task routing: [`docs/guides/016-task-routing-precedence.md`](./docs/guides/016-task-routing-precedence.md)
- JSON contracts: [`docs/guides/017-json-output-contracts.md`](./docs/guides/017-json-output-contracts.md)
- CI & release: [`docs/guides/049-ci-binary-distribution-and-release-protocol.md`](./docs/guides/049-ci-binary-distribution-and-release-protocol.md)
- Agent adoption: [`docs/guides/047-agent-and-cross-repo-adoption.md`](./docs/guides/047-agent-and-cross-repo-adoption.md)

## Terminology

- **selector**: a task request string (`test`, `api/test`)
- **routing**: how a selector resolves to a catalog and task
- **deferral**: fallback execution when no selector matches local tasks
