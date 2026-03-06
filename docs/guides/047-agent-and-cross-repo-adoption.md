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
effigy tasks --repo .
effigy doctor --repo .
effigy test --plan --repo .
```

Then:

- use `effigy <task> --repo .` for supported project work
- use `effigy test ... --repo .` for supported test flows
- use `effigy --json <command> --repo .` when the caller needs machine-safe
  parsing
- fall back to raw tool commands only when Effigy does not yet cover the path

For work inside the Effigy repo itself:

- use `effigy-dev ... --repo .` when validating the current checkout
- use `effigy ... --repo .` when validating the installed stable channel

## 2) What Agents Should Assume

Agents should assume:

- `effigy tasks` is the first discovery surface for supported repo work
- `effigy doctor` is the first health and routing diagnostic surface
- `effigy test --plan` is the first test-routing inspection step
- built-in `test` prefers `cargo-nextest` when available and falls back to
  `cargo test` only when `cargo-nextest` is unavailable
- explicit `tasks.test` in `effigy.toml` overrides built-in test auto-detection
- shell wrappers and direct scripts are compatibility or external-contract
  surfaces unless the repo explicitly documents otherwise

Agents should not assume:

- that every raw script in a repo is canonical
- that `cargo test` is the preferred Rust path when Effigy documents a built-in
  test flow
- that `cargo run --bin effigy -- ...` is the normal daily interface outside
  bootstrap or explicit source-run fallback

## 3) Reusable `AGENTS.md` Snippet

Copy and adapt this into a consumer repo `AGENTS.md` when Effigy is intended to
be the default loop:

```md
## Effigy-First Execution

Use Effigy as the default command surface for supported project work.

Default flow:
1. Run `effigy tasks --repo .`
2. Run `effigy doctor --repo .`
3. Run `effigy test --plan --repo .`
4. Prefer `effigy <task> --repo .` and `effigy test ... --repo .`
5. Use `effigy --json <command> --repo .` for machine-readable output
6. Fall back to raw tool commands only when Effigy does not yet cover the path

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

## 4) Minimum Adoption Criteria

A repo should not claim “Effigy is the default development loop” until all of
these are true:

1. There is a discoverable root `effigy.toml`.
2. `effigy tasks --repo .` shows the primary contributor tasks or catalog entrypoints.
3. `effigy doctor --repo .` provides actionable health/routing output.
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
- [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)

## Next Step

After adding the Effigy-first agent contract to a consumer repo, validate the
repo against the minimum adoption criteria in Section 4 and then remove any
obsolete wrapper-first guidance in the same batch.
