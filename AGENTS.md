# Agent Instructions for Effigy

Effigy is a Rust-based unified task runner for monorepos. Behavior is
**manifest-driven** (`effigy.toml`, often split across included files): most
`effigy <name>` invocations are **repo tasks**; built-ins include `test`, `init`,
`doctor`, and a short list from `effigy --help`, which groups them by job
(`effigy help <group>` for one group, `effigy help <command>` for detail).

## Always-loaded boundaries

- Use canonical docs and ready-card surfaces; do not invent parallel planning
  authority.
- Normal-mode agents use the current checkout. Worker mode activates only from
  an orchestrator handoff declaring `handoff_mode: worker-pr-loop`.
- Worker-mode `git fetch origin` must fail fast on blocked SSH. Prefer
  `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`
  so a prompt does not hang the startup probe.
- Do not add a current-directory repo override when already inside the target
  repo; use `--repo <PATH>` only for a different repo.
- Do not add `package.json` scripts that re-export Effigy tasks.
- Never modify `.github/workflows/` or run release mutations without explicit
  human instruction.
- When planning authority is ambiguous, stop and ask instead of guessing.

## Common commands

```bash
effigy tasks
effigy test --plan
effigy qa                 # test + docs + json contracts
effigy deliver release gates
```

Without `effigy` on PATH: `cargo run --bin effigy -- ...`.
Bootstrap from outside:
`effigy deliver bootstrap git@github.com:inflatable-cookie/effigy.git`.

Rust QA when needed: `cargo test`, `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings` (plain `cargo clippy` matches CI).

Route by job: `effigy repo graph` for code understanding; `effigy repo docs context` for
documentation authority (which contract, decision, or lane governs the work);
`effigy tasks` for selectors; `effigy doctor` for routing/health;
`effigy test --plan` for test shape.

## Docs authority

- `docs/README.md` — docs front door
- `docs/vision/README.md` — long-horizon direction
- `docs/roadmaps/README.md` — active milestone queue
- `docs/logs/README.md` — evidence log
- `docs/contracts/001-working-rules.md` — strict execution rules
- `docs/policy/internal-writing-style.md` — internal writing style

Bare `continue` in the strict lane resolves through the previous `Next Task`.
Anchor on the current ready batch card when one exists; otherwise stay in
planning.

During execution, append solvable friction to `PAPERCUTS.md` per the Northstar
papercuts loop; do not stop the current task to fix papercuts unless already in
scope.

## Changelog

Append user-facing changes to `CHANGELOG.md` under `[Unreleased]`:
**Breaking**, **Added**, **Changed**, **Fixed**.

## Release protocol

Never initiate release without explicit human instruction. Never bypass gates
or re-tag failed releases. Canonical references:
[`docs/guides/051-release-orchestration.md`](./docs/guides/051-release-orchestration.md),
[`docs/guides/049-ci-binary-distribution-and-release-protocol.md`](./docs/guides/049-ci-binary-distribution-and-release-protocol.md).

## Cross-repo agent skill

Other repos: `npx skills add inflatable-cookie/effigy`. This repo's
`.agents/skills/effigy` copy is authoritative here.

## Key documentation

- Guides hub: [`docs/guides/README.md`](./docs/guides/README.md)
- Strict planning lane: [`docs/specs/README.md`](./docs/specs/README.md)
- Task routing: [`docs/guides/016-task-routing-precedence.md`](./docs/guides/016-task-routing-precedence.md)
- JSON contracts: [`docs/guides/017-json-output-contracts.md`](./docs/guides/017-json-output-contracts.md)
- Agent adoption: [`docs/guides/047-agent-and-cross-repo-adoption.md`](./docs/guides/047-agent-and-cross-repo-adoption.md)
- Northstar consumer contract: [`docs/guides/056-northstar-effigy-consumer-repo-contract.md`](./docs/guides/056-northstar-effigy-consumer-repo-contract.md)

<!-- BEGIN EFFIGY AGENT CONTRACT -->
## Effigy Agent Contract

Use Effigy as the default command surface for supported project work.

Route by job, not by startup ritual:
- use `effigy repo graph` for code understanding
- use `effigy tasks` for selector inventory
- use `effigy doctor` for routing ambiguity or repo health
- use `effigy test --plan` when test execution shape matters

Use `effigy repo graph` when the job is code understanding: ownership, flow,
implementation, or changed-file impact. Do not insert graph into unrelated
deployment, state, docs, release, or direct task-execution work.

Use `effigy repo docs context "<question>"` when the job is documentation authority
rather than code ownership. It returns exact repository sections with
provenance, ranked by this repo's committed `[docs_policy.graph]` profile in
`docs/effigy.docs.toml`. That profile is the only runtime authority; no
installed skill or starter is read at query time.

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
- Documentation graph profiles: `docs/guides/079-documentation-graph-profiles-and-context.md`
- JSON contracts: `docs/guides/017-json-output-contracts.md`
<!-- END EFFIGY AGENT CONTRACT -->

<!-- northstar:rust-quality:start -->
## Northstar Rust Quality

Scope: Rust source, Cargo manifests, build files, tests, and directly related
documentation under this directory.

Use Northstar's strict everyday-authoring route for ordinary Rust work. Resolve
the repository-owned profile and deviations under `docs/contracts/`; never
assume a universal MSRV. Re-enter at task start and coherent batch closeout.
Preserve unrelated work. A quality audit, no-slop pass, or audit-and-fix request
is explicit audit intent; never route it through everyday authoring.
<!-- northstar:rust-quality:end -->
