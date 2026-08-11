# 047 - Agent and Cross-Repo Adoption

Use this guide when you want AI agents to treat Effigy as the default project
surface instead of falling back to raw tool commands. The goal is a job-based
agent loop: route to the right Effigy surface first, understand code when
needed, execute through Effigy, then validate through Effigy.

## Vision Alignment

- Primary tags: `OPERATE`, `ROUTE`, `MAINT`
- Target movement: agent execution becomes deterministic, repo-portable, and
  aligned with the same task surface used by human contributors.

## 1) Default Agent Contract

For repos that adopt Effigy as the primary local loop, the default contract
should be job-based, not ritual-based:

- use `effigy graph` when the job is code understanding and the agent needs a
  bounded repo map before broad file scanning (see Section 2a)
- use `effigy tasks` when the agent needs runnable selectors or QA surfaces
- use `effigy doctor` when routing is unclear or repo health is itself the task
- use `effigy test --plan` when test execution shape matters
- use `effigy <task>` for supported project work
- use `effigy test ...` for supported test flows
- use `effigy --json <command>` when the caller needs machine-safe parsing
- fall back to raw tool commands only when Effigy does not yet cover the path

For work inside the Effigy repo itself:

- use `cargo run --bin effigy -- ...` when validating the current checkout
- use `effigy ...` when validating the installed/local binary

## 2) What Agents Should Assume

Agents should assume:

- Effigy routing is job-based, not a mandatory `doctor` -> `tasks` -> `test --plan` greeting
- `effigy tasks` is the first task-inventory surface when selector discovery is needed
- `effigy doctor` is the routing or repo-health diagnostic surface when ambiguity or drift is present
- `effigy test --plan` is the test-routing inspection step when test shape matters
- `--repo` is only needed when intentionally targeting a different repo outside
  the current working tree
- built-in `test` prefers `cargo-nextest` when available and falls back to
  `cargo test` only when `cargo-nextest` is unavailable
- explicit `tasks.test` in `effigy.toml` overrides built-in test auto-detection
- shell wrappers and direct scripts are compatibility or external-contract
  surfaces unless the repo explicitly documents otherwise
- a vendored `.agents/skills/effigy` copy is repo-authoritative when present,
  and may be marked internal so generic `npx skills` repo scans do not treat it
  as the public install source
- task cost follows one explicit ladder: `health` is cheap orientation,
  `validate` is the mid gate, and `qa` is the full board; never map `health`
  directly or transitively to `qa` because doctor delegates to `tasks.health`

Agents should not assume:

- that every raw script in a repo is canonical
- that `cargo test` is the preferred Rust path when Effigy documents a built-in
  test flow
- that adding a current-directory repo override is a better or safer default
  than just running the command from the repo root
- that `cargo run --bin effigy -- ...` is the normal daily interface outside
  bootstrap or explicit source-run fallback

## 2a) Code Graph Assist

When the job is code understanding before editing or review, prefer the local
graph over spraying `rg` across the whole repo:

```sh
effigy graph explore "<task-shaped question>" --max-files 6 --max-bytes 12288 --json
git diff --name-only | effigy graph affected --stdin --json
```

Rules:

- use `graph explore` first for task-shaped navigation
- do not insert `graph` ritualistically for unrelated work; deployment, state,
  docs, release, and direct task execution should use their matching Effigy
  surfaces first
- phrase graph queries as implementation questions such as:
  - `where are redirect responses handled`
  - `where are config migrations validated before apply`
  - `where does shell exit cleanup prompt run`
- trust returned excerpts for first-pass orientation; open files only when the
  excerpt is insufficient for the edit or review
- use `graph affected` to narrow validation after edits, not as exhaustive test
  proof
- use graph-aware scans when the question is review risk rather than
  navigation, for example:
  - `effigy scan boundary-violations --json`
  - `effigy scan dead-code --json`
  - `git diff --name-only | effigy scan validation-gaps --stdin --json`
- use `rg` for exact token verification and final checks before editing
- graph queries refresh stale or missing indexes before reading; use
  `graph status` only for the report-only pre-refresh view

Full workflow and limits:
[`076-code-graph-and-agent-workflows.md`](./076-code-graph-and-agent-workflows.md)

## 3) Reusable `AGENTS.md` Snippet

Copy and adapt this into a consumer repo `AGENTS.md` when Effigy is intended to
be the default loop:

```md
## Effigy-First Execution

Use Effigy as the default command surface for supported project work.

Default flow:
1. Route by job, not by startup ritual
2. Use `effigy graph explore` before broad repo scanning when codebase context is needed
3. Use `effigy tasks` when you need selector inventory
4. Use `effigy doctor` when routing is unclear or repo health is the task
5. Use `effigy test --plan` when test execution shape matters
6. Prefer `effigy <task>` and `effigy test ...` for execution and validation
7. Use `effigy --json <command>` for machine-readable output
8. Use `--repo <PATH>` only when intentionally operating on another repo
9. Fall back to raw tool commands only when Effigy does not yet cover the path

Portfolio maintenance:
- use `effigy --json papercuts --scope <projects-dir>` to inventory root
  papercut queues for periodic triage
- treat entries as observations until the owning project's planning process
  promotes them

Testing policy:
- treat `effigy test` as the default test entrypoint when available
- if `tasks.test` exists in `effigy.toml`, that explicit task is the source of truth
- for Rust repos without explicit `tasks.test`, Effigy prefers `cargo-nextest` when available

Repo maintenance policy:
- keep main contributor loops represented in `effigy.toml`
- update this file when Effigy task coverage or fallback policy changes
```

If the repo is Effigy itself, replace `effigy` with
`cargo run --bin effigy -- ...` for current checkout validation and keep
`effigy` for installed/local-binary checks.

Recommended enforcement task for adopted repos:

```toml
[tasks]
"qa:docs:agent-defaults" = "effigy docs check forbidden AGENTS.md README.md .github/workflows/ci.yml --forbid '--repo .'"
```

Adjust the file list to match the repo's real agent-facing surfaces. The point
is to fail the docs/agent QA bundle when current-directory repo overrides start
showing up as copied defaults.

## 3a) Cross-Repo Agent Skill

For agents working in repos that use Effigy but don't host Effigy-specific
guidance, install the bundled agent skill:

```bash
npx skills add inflatable-cookie/effigy
```

For project-local adoption managed by Effigy itself, use:

```bash
effigy init --checklist --json
effigy init --apply-actions manifest.effigy_toml,agents_md.effigy_contract,skill.codex_project,gitignore.effigy_local_state --json
effigy init
```

The checklist mode reports the wider setup inventory with applicability, safety
class, and recommended commands. Plain `effigy init` writes only deterministic
managed surfaces and preserves existing project manifests and READMEs, while
prompting only when the call is on a real TTY without conflicting flags.

When both a project-local and global Effigy skill are present, treat the
project-local `.agents/skills/effigy` copy as authoritative for that repo. The
global install is fallback convenience, not the source of truth for a vendored
project skill.

The skill follows the open
[Agent Skills](https://agentskills.io/specification) standard and works in
Claude Code, OpenAI Codex, Cursor, and any other agent that consumes
`SKILL.md`. Source: [`skills/effigy/`](../../skills/effigy/) in this repo.

The skill front door is intentionally light (~150 lines) and routes to topic
references for footguns, discovery loop, selector routing, common workflows,
JSON envelopes, graph-first code navigation, config shapes, and release
protocol. Agents read the
references on demand without re-fetching the front door.

Manual install for agents `npx skills` doesn't cover:

```bash
mkdir -p ~/.claude/skills && cp -r skills/effigy ~/.claude/skills/effigy
mkdir -p ~/.agents/skills && cp -r skills/effigy ~/.agents/skills/effigy
mkdir -p ~/.cursor/skills && cp -r skills/effigy ~/.cursor/skills/effigy
```

The skill is the recommended cross-repo entry point. The `AGENTS.md` snippet
in section 3 still serves repos that want Effigy-first execution baked into
their own project instructions; the two are complementary.

## 4) Minimum Adoption Criteria

A repo should not claim “Effigy is the default development loop” until all of
these are true:

1. There is a discoverable root `effigy.toml`.
2. `effigy tasks` shows the primary contributor tasks or catalog entrypoints.
3. `effigy doctor` provides actionable health/routing output.
4. The repo has one supported default test path:
   - built-in `effigy test`, or
   - explicit `tasks.test` in `effigy.toml`.
5. The main contributor loop is represented in tasks, for example:
   - `dev`
   - `test`
   - `validate`
   - `qa`
   - `build`
6. `AGENTS.md` or equivalent repo instructions tell agents to use Effigy first.
7. CI or local automation uses Effigy for supported flows instead of bypassing
   it by default.

If these are not yet true, the repo is still in migration and should document
Effigy as partial coverage rather than the default loop.

## 4a) Northstar + Effigy Boundary

When a repo wants the full Northstar + Effigy operating model, keep the
boundary explicit:

- use the reusable repo contract in
  [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)
  as the source of truth for required files, `qa:northstar`, and docs policy
- let the `northstar-effigy` skill or template bundle scaffold repo shape,
  starter files, and adoption mode
- let Effigy own the generic validation and execution surfaces, for example:
  `check-paths`, `check-index`, `check-next-action`, `check-headings`,
  `check-forbidden`, JSON mode, and release orchestration

Do not collapse bootstrap logic into Effigy unless repeated adoption pain shows
that the skill/template layer cannot cover the gap cleanly.

## 5) Recommended Rollout Waves

### Wave 1 - Baseline Task Coverage

- add or normalize root `effigy.toml`
- represent the main contributor tasks in Effigy
- make `tasks` and `doctor` useful before wider rollout

### Wave 2 - Testing and Health

- make `effigy test --plan` reliable and documented
- decide whether built-in `test` or explicit `tasks.test` is the source of truth
- ensure `doctor` surfaces common routing/config failures clearly

### Wave 3 - Agent Instructions

- update `AGENTS.md` with the Effigy-first contract
- remove old “just run raw scripts” instructions where Effigy owns the path
- add JSON-mode notes for automation consumers
- add a small forbidden-text QA task so `--repo .` does not creep back into
  copied agent examples

### Wave 4 - Automation and CI

- route CI and recurring local validation through Effigy for supported paths
- keep standalone scripts only where an explicit external contract is still required
- pin installed binary or stable channel strategy per repo
- do not mirror Effigy tasks into `package.json` scripts; call `effigy <task>`
  directly

### Wave 5 - Cleanup

- retire obsolete wrappers, cargo aliases, and package-manager script shims once
  their Effigy-first replacement is stable
- keep a short rollback path for release-sensitive repos
- capture the final adoption state in repo docs

## 6) Allowed Fallback Cases

Raw commands are still reasonable when:

- Effigy does not yet expose the required workflow
- the path is an external packaging or release contract kept as a standalone wrapper
- you are bootstrapping the Effigy repo before `effigy` is available
- the repo explicitly documents a temporary migration exception

When using a fallback, keep the reason explicit in repo docs so agents do not
learn the wrong default.

`package.json` is not an allowed fallback for re-exporting Effigy tasks. Keep
Node package scripts for real package-manager workflows only.

## 7) Release Orchestration Across Repo Types

When a consumer repo adopts Effigy's release surface, keep the release config
close to the repo's real version file instead of forcing a Rust-shaped layout.

### Node.js (`package.json`)

```toml
[release]
changelog = "CHANGELOG.md"
tag-format = "v{version}"

[release.gates]
test = "npm test"
```

Expected behavior:
- `effigy release status --check-gates` reads `package.json` version
  automatically
- `effigy release prepare --plan` previews the `package.json` version
  update plus changelog move
- gate commands can stay native to the project (`npm`, `pnpm`, `bun`, shell)

### Python (`pyproject.toml`)

```toml
[release]
changelog = "CHANGELOG.md"
tag-format = "v{version}"

[release.gates]
test = "pytest -q"
```

Expected behavior:
- `effigy release` auto-detects `pyproject.toml`
- version discovery supports `project.version` and `tool.poetry.version`
- `effigy release prepare --plan` previews the pyproject version bump

### Multi-language / Plain `VERSION`

```toml
[release]
version-file = "VERSION"
changelog = "CHANGELOG.md"
tag-format = "release-{version}"

[release.gates]
validate = "sh -lc './scripts/validate-all.sh'"
```

Expected behavior:
- use this when the repo version is intentionally decoupled from language-specific
  manifests
- `effigy release prepare --yes --check-gates` updates `VERSION`,
  writes `.release-prepared.json`, and preserves heterogeneous gate commands
- this is the simplest fit for monorepos with multiple language toolchains

Release adoption policy:
- prefer `effigy release simulate/status/prepare/execute` for operator-driven
  release flow once the repo has a stable `[release]` section
- keep wrapper scripts only when an external automation contract still requires
  them, and describe them as backup channels rather than the default operator
  path
- do not describe wrapper retirement or workflow-level cutover as complete
  until the repo finishes its explicitly human-gated release-adoption steps
- document the repo's chosen version file and gate commands in `AGENTS.md` so
  agents do not fall back to the wrong toolchain defaults

## 8) Demo And Manifest Adoption Boundary

When a consumer repo is also adopting the native demo surface:

- use [`058-demo-system-guide.md`](./058-demo-system-guide.md) for the steady-
  state operator model
- use [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
  when the repo should split demo config into a dedicated fragment
- use [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
  when the repo is still moving from demo scripts or one-wrapper-task-per-demo
  patterns

Do not duplicate that migration guidance inside `AGENTS.md`. Keep agent
instructions short and point them at the native demo surface once it exists.

## Expected Outcome

- AI agents have one short, repeatable contract for using Effigy in project work
- consumer repos can adopt Effigy with a clear minimum bar instead of vague intent
- rollout moves in explicit waves rather than ad hoc script replacement

## Related Guides

- [`010-path-installation-and-release.md`](./010-path-installation-and-release.md)
- [`013-testing-orchestration.md`](./013-testing-orchestration.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)
- [`076-code-graph-and-agent-workflows.md`](./076-code-graph-and-agent-workflows.md)

## Next Step

After adding the Effigy-first agent contract to a consumer repo, validate the
repo against the minimum adoption criteria in Section 4 and then remove any
obsolete wrapper-first guidance in the same batch. If the repo is also adopting
native demos, continue with
[`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
instead of inventing a repo-local demo migration path.
