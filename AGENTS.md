# Agent Instructions for Effigy

Effigy is a Rust-based unified task runner for monorepos. Behavior is
**manifest-driven** (`effigy.toml`, often split across included files): most
`effigy <name>` invocations are **repo tasks**; built-ins include `test`, `init`,
`doctor`, and a short list from `effigy --help`. Names like **`dev`** are
usually tasks the repo defines.

## Build & Test

```bash
cargo test                    # run all tests
cargo fmt --all -- --check    # format check
cargo clippy --all-targets -- -D warnings   # lint check
```

First-party code has no repo-wide Clippy allowances. A plain `cargo clippy`
matches CI — no `-A` flags are needed.

If `effigy` is on PATH, this repository's own Effigy tasks are available
(including **`qa:*`** aggregators defined only here):

```bash
effigy test --plan   # show test plan
effigy qa            # full QA (test + docs + json contracts)
effigy release gates # release-gate pass for the current repo
```

Otherwise bootstrap with `cargo run --bin effigy -- ...`.

For first-time local bring-up from outside this repo:
- use `effigy bootstrap git@github.com:inflatable-cookie/effigy.git`

Default local rule:
- do not add a current-directory repo override when already running inside the target repo
- use `--repo <PATH>` only when intentionally targeting a different repo
- do not add `package.json` scripts that re-export Effigy tasks; run
  `effigy <task>` directly and keep package scripts package-native

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
- `gh workflow run release-binaries.yml -f tag=v0.__.__` (explicit human approval)
- `effigy release verify-install --tag v0.__.__`
- `effigy changelog extract CHANGELOG.md --version X.Y.Z`

Canonical reference:
- [`docs/guides/051-release-orchestration.md`](./docs/guides/051-release-orchestration.md)

## Key Documentation

- Guides hub: [`docs/guides/README.md`](./docs/guides/README.md)
- Strict planning lane: [`docs/specs/README.md`](./docs/specs/README.md)
- Working rules: [`docs/contracts/001-working-rules.md`](./docs/contracts/001-working-rules.md)
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

## Strict Continuation Rule

- In the active strict lane, `continue` should resolve through the previous
  `Next Task`.
- If there is an active ready batch card, execution should anchor on that card.
- If there is no ready card, stop in planning instead of improvising execution.
- When the next move is materially ambiguous, ask for intent instead of
  guessing.

## Internal Writing Style

Use the repo-local style reference for internal work and normal replies:

- `docs/policy/internal-writing-style.md`

## Cross-Repo Agent Skill

Agents working in other repos that use Effigy should install the bundled
agent skill: `npx skills add inflatable-cookie/effigy`. Source lives at
[`skills/effigy/`](./skills/effigy/) and works in Claude Code, Codex CLI,
Cursor, and any other agent that consumes the open `SKILL.md` standard.

<!-- BEGIN EFFIGY AGENT CONTRACT -->
## Effigy Agent Contract

Use Effigy as the default command surface for supported project work.

Route by job, not by startup ritual:
- use `effigy graph` for code understanding
- use `effigy tasks` for selector inventory
- use `effigy doctor` for routing ambiguity or repo health
- use `effigy test --plan` when test execution shape matters

Use `effigy graph` when the job is code understanding: ownership, flow,
implementation, or changed-file impact. Do not insert graph into unrelated
deployment, state, docs, release, or direct task-execution work.

Prefer `effigy <task>`, `effigy test`, and the matching built-in surface over
raw package-manager or shell commands when Effigy covers the path. Use
`effigy --json <command>` whenever another agent or tool will consume output.

This repo's local `.agents/skills/effigy` copy is authoritative for this
project. When an agent supports both project-local and global skills, prefer
the project-local copy over any globally installed Effigy skill.

Do not add a current-directory repo override while already inside the target
repo. Do not edit
`.github/workflows/` or run release mutations unless the user explicitly asks.

Reference docs:
- Effigy agent adoption: `docs/guides/047-agent-and-cross-repo-adoption.md`
- Graph workflows: `docs/guides/076-code-graph-and-agent-workflows.md`
- JSON contracts: `docs/guides/017-json-output-contracts.md`
<!-- END EFFIGY AGENT CONTRACT -->
