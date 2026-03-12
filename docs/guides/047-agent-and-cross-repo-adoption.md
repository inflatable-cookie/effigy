# 047 - Agent and Cross-Repo Adoption

Use this guide when you want AI agents to treat Effigy as the default project
surface instead of falling back to raw tool commands.

## Vision Alignment

- Primary tags: `OPERATE`, `ROUTE`, `MAINT`
- Target movement: agent execution becomes deterministic, repo-portable, and
  aligned with the same task surface used by human contributors.

## 1) Default Agent Contract

For repos that adopt Effigy as the primary local loop, the default agent flow
should be:

```sh
effigy tasks
effigy doctor
effigy test --plan
```

Then:

- use `effigy <task>` for supported project work
- use `effigy test ...` for supported test flows
- use `effigy --json <command>` when the caller needs machine-safe
  parsing
- fall back to raw tool commands only when Effigy does not yet cover the path

For work inside the Effigy repo itself:

- use `effigy-dev ...` when validating the current checkout
- use `effigy ...` when validating the installed stable channel

## 2) What Agents Should Assume

Agents should assume:

- `effigy tasks` is the first discovery surface for supported repo work
- `effigy doctor` is the first health and routing diagnostic surface
- `effigy test --plan` is the first test-routing inspection step
- `--repo` is only needed when intentionally targeting a different repo outside
  the current working tree
- built-in `test` prefers `cargo-nextest` when available and falls back to
  `cargo test` only when `cargo-nextest` is unavailable
- explicit `tasks.test` in `effigy.toml` overrides built-in test auto-detection
- shell wrappers and direct scripts are compatibility or external-contract
  surfaces unless the repo explicitly documents otherwise

Agents should not assume:

- that every raw script in a repo is canonical
- that `cargo test` is the preferred Rust path when Effigy documents a built-in
  test flow
- that adding a current-directory repo override is a better or safer default
  than just running the command from the repo root
- that `cargo run --bin effigy -- ...` is the normal daily interface outside
  bootstrap or explicit source-run fallback

## 3) Reusable `AGENTS.md` Snippet

Copy and adapt this into a consumer repo `AGENTS.md` when Effigy is intended to
be the default loop:

```md
## Effigy-First Execution

Use Effigy as the default command surface for supported project work.

Default flow:
1. Run `effigy tasks`
2. Run `effigy doctor`
3. Run `effigy test --plan`
4. Prefer `effigy <task>` and `effigy test ...`
5. Use `effigy --json <command>` for machine-readable output
6. Use `--repo <PATH>` only when intentionally operating on another repo
7. Fall back to raw tool commands only when Effigy does not yet cover the path

Testing policy:
- treat `effigy test` as the default test entrypoint when available
- if `tasks.test` exists in `effigy.toml`, that explicit task is the source of truth
- for Rust repos without explicit `tasks.test`, Effigy prefers `cargo-nextest` when available

Repo maintenance policy:
- keep main contributor loops represented in `effigy.toml`
- update this file when Effigy task coverage or fallback policy changes
```

If the repo is Effigy itself, replace `effigy` with `effigy-dev` for current
checkout validation and keep `effigy` for installed stable-channel checks.

Recommended enforcement task for adopted repos:

```toml
[tasks]
"qa:docs:agent-defaults" = "effigy docs check-forbidden AGENTS.md README.md .github/workflows/ci.yml --forbid '--repo .'"
```

Adjust the file list to match the repo's real agent-facing surfaces. The point
is to fail the docs/agent QA bundle when current-directory repo overrides start
showing up as copied defaults.

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

### Wave 5 - Cleanup

- retire obsolete wrappers and cargo aliases once their Effigy-first replacement
  is stable
- keep a short rollback path for release-sensitive repos
- capture the final adoption state in repo docs

## 6) Allowed Fallback Cases

Raw commands are still reasonable when:

- Effigy does not yet expose the required workflow
- the path is an external packaging or release contract kept as a standalone wrapper
- you are bootstrapping the Effigy repo before `effigy` / `effigy-dev` are available
- the repo explicitly documents a temporary migration exception

When using a fallback, keep the reason explicit in repo docs so agents do not
learn the wrong default.

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
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)

## Next Step

After adding the Effigy-first agent contract to a consumer repo, validate the
repo against the minimum adoption criteria in Section 4 and then remove any
obsolete wrapper-first guidance in the same batch.
