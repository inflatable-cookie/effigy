# 013 - Testing Orchestration

`effigy test` is Effigy's single test entrypoint. It runs named
`[test.suites]` when configured, or detects applicable runners when they are
not. In polyglot roots, detection can schedule both JavaScript and Rust suites.

## Commands

- `effigy test`
- `effigy test --plan`
- `effigy test --verbose-results`
- `effigy test --tui`

## Detection Order

Per target root:

1. `vitest` when the selected package declares it in `package.json` or owns a
   `vitest.config.*` file, or a `vite.config.*` with a `test` block. A
   transitive `node_modules/.bin/vitest` is not package intent. When that
   config sets `test.dir`, the detected command includes `--dir`.
2. `cargo nextest run` when `Cargo.toml` exists and `cargo-nextest` is available.
3. `cargo test` when `Cargo.toml` exists and `cargo-nextest` is unavailable.

When the root `Cargo.toml` declares `[workspace]`, both Cargo runners add
`--workspace` automatically. Use `[test.runners]` or `[test.suites]` only when
the repository intentionally needs a narrower or otherwise custom board.

### Oxide route

When this machine has a usable `oxide`, the detected Rust suite runs through
Oxide's leased slot, so its build intermediates stay in the shared pool instead
of landing unmanaged in the checkout's `target`:

- `cargo nextest run …` runs as `oxide nextest run …`
- the `cargo test` fallback runs as `oxide test …`

`effigy test --plan` shows the chosen command and the evidence for it. The
gate is `oxide pool describe --json` answering `oxide.pool.describe/1`; an
`oxide` that is present but cannot answer fails the run and names the escape
instead of quietly using plain Cargo. Container-runtime targets and
`[test.runners]` overrides keep their own command.

Escape to plain Cargo for one run when Oxide is broken:

```sh
EFFIGY_OXIDE_ESCAPE=cargo effigy test
```

Without `oxide` on PATH the route is plain Cargo, unchanged. A routed suite
that fails repeats the escape in its results, and Oxide's own diagnostic stays
in the suite's output. A harness that runs `effigy test` under a temporary
`HOME` passes `OXIDE_POOL` through from the real environment first, or Oxide
creates a cold pool under that home.

### Cargo package scope in passthrough

Passthrough arguments reach the runner command verbatim, and Cargo treats
`--workspace` and package selection additively — `--workspace -p app` runs
every workspace member, not just `app`. Built-in routing therefore keeps the
auto-added workspace flag out of the way when passthrough narrows scope:

- `-p <package>` / `--package <package>` (including `--package=<package>`,
  attached `-p<package>`, and repeated selections) runs exactly the named
  packages; the auto-added `--workspace` is omitted and `effigy test --plan`
  shows the scoped command.
- An explicit `--workspace` or `--all` in passthrough keeps the full-workspace
  run (the auto flag yields to avoid repeating the flag, which the runners
  reject as a usage error).
- Combining an explicit workspace selection with an explicit package selection
  is rejected as ambiguous instead of silently broadening: keep one request.
  `--workspace --exclude <package>` is the supported way to run the workspace
  minus members; `--exclude` without `--workspace` keeps the auto workspace
  flag, which is what makes the exclusion valid for Cargo.

`effigy test --plan` reports the composed command either way, so the plan is
the source of truth for the exact package scope before execution.

`effigy test --plan` prints selected runner, command, evidence, fallback chain,
and per-target `cargo-env-match` mode. Planning never runs setup, suite, or
teardown commands.

## Nested Effigy Invocations

A host suite runs with `EFFIGY_EXECUTABLE` naming the Effigy binary that
started the test run, unless the environment already sets it. Steps that invoke
Effigy from inside a test (a task reference, a Rhai `effigy.*` call) therefore
reuse that binary instead of rebuilding the workspace binary with `cargo run`.
A rebuild can replace the running binary under another suite, and it costs a
Cargo invocation per step. Set `EFFIGY_EXECUTABLE`
yourself to test against a different build; the TUI and parallel paths carry the
same value.

## Configuration Authority

`[test]`, `[test.suites]`, and `[test.runners]` are the only test configuration
surfaces. `tasks.test` was removed in v0.11 because it made `--plan` ambiguous
and could turn inspection into execution. Move custom test commands into named
suite entries.

## Lifecycle-Aware Configured Suites

Builtin `test` can also use configured suite tables instead of plain command strings:

```toml
[test.suites.integration]
run = [
  { task = "db:test:prepare" },
  "cargo nextest run --workspace"
]
env = "TEST_DATABASE_URL"
env_file = ".env"
setup = [{ run = "cargo run -p app-db --bin migrate_test_db" }]
teardown = [{ run = "cargo run -p app-db --bin reset_test_db" }]
teardown_policy = "always"
```

Use this when a suite needs reusable task references, declarative
setup/teardown, or a custom runner command without creating a competing test
task.

Operational notes:
- suite `env` / `env_file` resolve before setup, run, and teardown
- a catalog with a named-container runtime target runs setup, the suite
  command, and teardown inside that container as one lifecycle exec; nested
  effigy re-entries keep their own routing
- `teardown_policy` defaults to `on-success`
- `teardown_policy = "always"` guarantees cleanup after setup failure or runner failure
- use `effigy test managed -- --package my-crate --test my_test` when forwarding raw runner args
- use `effigy test --plan` to inspect `suite-env`, `suite-env-files`, `setup-steps`, `teardown-steps`, and `teardown-policy`

Focused suites can stay off the default board:

```toml
[test.suites.workspace]
run = "cargo test --workspace"

[test.suites.controller]
run = "cargo test -p controller"
default = false
```

Bare `effigy test` runs `workspace`; `effigy test controller` runs the focused
suite.

## Built-in Cargo Env Auto-Apply

When built-in `test` runs cargo suites (detected or configured), Effigy automatically applies manifest `[env]` entries whose keys start with `CARGO_`.

Included sources:
- direct `[env]` entries (for example `CARGO_HOME = "..."`)
- grouped profile arrays under `[env]` (for example `cargo = [{ CARGO_HOME = "..." }, ...]`)
- fallback for missing `CARGO_HOME` / `CARGO_TARGET_DIR`:
  1. process environment
  2. `<target-root>/.env`

Precedence:
- manifest `[env]` wins over process env and dotenv fallback
- process env wins over dotenv fallback

Matching behavior:
- applies to command shapes that resolve to cargo executables (`cargo`, `cargo-nextest`, `oxide`, and their absolute paths), so an Oxide-routed suite keeps the same manifest `CARGO_*` entries; Oxide owns the build directory either way
- supports common wrappers and prefixes before cargo (`env`, `exec`, `command`, leading `KEY=value` assignments)
- in default `prefix-aware` mode, does not apply to shell-wrapped commands where cargo is inside a shell string (for example `sh -lc "cargo test --workspace"`)
- does not apply to non-cargo executables

Matcher tuning:
- default mode is `prefix-aware`
- set `[test].cargo_env_match = "executable-only"` to only match direct cargo executables (no wrappers/prefixes)
- set `[test].cargo_env_match = "shell-aware"` to also match shell-wrapped cargo commands (for example `sh -lc 'cargo nextest run --workspace'`)

Value substitution:
- `{project}` and `{repo}` in `[env]` `CARGO_*` values resolve to the executing catalog root for each built-in test target.

Example:

```toml
[env]
CARGO_HOME = "{project}/.effigy/cargo/home"
CARGO_TARGET_DIR = "{project}/.effigy/cargo/target"

[test.suites]
integration = "env RUST_LOG=info cargo nextest run --workspace"
```

## Task Reference Chains

Task-ref chains (`{ task = "..." }`) can target built-ins (including `test`) and include inline args.

Examples:

```toml
[tasks.validate]
run = [{ task = "test vitest" }, "printf validate-ok"]

[tasks.dev]
mode = "tui"

concurrent = [
  { run = "cargo run -p api", start = 1, tab = 2 },
  { task = "test vitest \"user service\"", start = 2, tab = 1 }
]
```

Notes:
- inline args are parsed with shell-style quoting/escaping.
- quote multi-word args inside the task string.
- parsing is tokenization only; shell expansion features (for example globbing, variable expansion, command substitution) are not applied inside `task = "..."`.

## Workspace Fanout

When built-in `test` is used from a workspace root, Effigy fans out across explicitly declared catalog roots and aggregates results.

Concurrency is configured in root `effigy.toml`:

```toml
[test]
max_parallel = 2
exclude_catalogs = ["api"]
```

`exclude_catalogs` removes aliases only from root fanout. Direct selection such
as `effigy api/test` still works. Use it when a parent workspace suite already
covers a nested child or when mounted sibling catalogs are outside the repo's
test board. `test --plan` reports exclusions and warns about likely nested
Cargo overlap.

If unset, Effigy defaults to `3` workers.

Result rendering:
- default is compact per-target status only,
- `--verbose-results` includes runner/root/cargo-env-match/command details per target.

TUI diagnostics:
- set `EFFIGY_TUI_DIAGNOSTICS=1` when running `effigy test --tui` to emit post-run runtime diagnostics and recent trace lines for emulator/debug troubleshooting.

## Locked Focused Runs

Passthrough reaches the runner verbatim, so a focused Rust run can require a
lockfile that is already up to date:

```sh
effigy test -p app-api --locked
```

Use it whenever the checkout resolves sibling crates from local paths, for
example while `effigy deps link cargo` is active, or when a planning checkout
must stay clean. `--locked` fails with the resolver's diagnostic instead of
rewriting `Cargo.lock`, so read-only evidence collection cannot dirty the tree.
An update that is genuinely wanted is a separate, reviewed change to the
lockfile.

This repository routes the same guarantee through `test:focused`, which always
passes `--locked`:

```sh
effigy test:focused -p effigy-deps --lib -E 'test(cargo_unlink::tests::)'
```

## Selector truth

A selector that runs a test runner must fail when it runs nothing. Every
test-running task in `config/tasks.toml` therefore calls nextest with
`--no-tests fail`, and a filter term names a test that exists: `cargo test
<substring>`, which exits green after selecting nothing, is not used for a
selector anymore. When a test is renamed or pruned, fix its selector in the same
change — the selector is the only thing that notices a renamed test.

Each selector names one budget. Ordinary selectors inherit
`tests/config/gateway-nextest.toml`; a selector whose every case owns a durable
journal on the drive-backed scratch root names
`tests/config/journal-heavy-nextest.toml` instead of adding another per-test
override. A case that hangs still dies; a case that is merely slow on a loaded
host does not fail a proof.

## Live consumer shapes

`qa:live:consumer-shapes` is an internal milestone proof, never part of a
per-task validation. It proves the shapes a consumer's checkout generates where
the other fixtures cannot:

- the rendered Compose is handed to the real `compose config`, so every
  `build` names Dockerfile content beside its context, no `aliases: null`
  survives, a shared service carries no checkout path, and a Dockerfile's own
  `$` survives Compose interpolation;
- a client of a shared MariaDB resolves `host.docker.internal` while a local
  MariaDB of the same engine stays local;
- a primary checkout and a linked worktree of the same fixture, in one
  environment and sharing one backing service, keep distinct scoped project
  identities and distinct effective hosts: the linked checkout's first `up`
  takes its own scope, and both checkouts stay up at the same time.

It runs under `/Volumes/Builds/scratch/effigy/` with private scratch checkouts
and removes what it created. It never starts, stops or reconfigures a runtime,
and it does not touch another checkout's shared services: the fixture's shared
service owns its own project name.

The selector runs both halves in one step. The live half brings the same
fixture up on the Colima profile: a build service whose Dockerfile uses
`ARG`/`${VAR}`/`$var`, a shared MariaDB beside a local one of the same engine
with a client resolving `host.docker.internal`, a primary and a linked worktree
in one environment and on one backing service, with distinct scoped project
names and effective hosts, a second `up` that keeps every container ID, no
`EFFIGY_` value in any container, `container <env> restart <process>` on a
host-process listener, and a `down` that leaves no container, listener or route.
The linked checkout is never prepared before its first `up`: the proof records
its unprepared host map (no scope token, declared hosts) first, so that `up`
exercises the fresh-checkout path.
The listener publishes its route into the fixture's own private gateway root
(`EFFIGY_GATEWAY_PRIVATE_STATE_ROOT`, loopback DNS/HTTP/HTTPS with its own CA),
so this proof never uses the machine gateway that the operator-path test owns;
it re-verifies the machine gateway's listeners, PIDs and executables afterwards.
It deletes the fixture's own shared-service container, volume and network: it
asks the runtime for that project's containers by label, removes each by exact
id, and touches a volume or network only once the label query reports no
container left. It never touches the protected shared MariaDBs, and fails
(keeping the staging root) if any owned name survives.

Three modes exist for the operator: no mode runs both halves (what the selector
does), `compose` runs only the Compose-checked render proof, and `plan` prints
the frozen plan — fixture paths, project and container names, port policy, the
binary's identity, the protected container baseline and the cleanup, with a
sha256 for each candidate staging root — while performing no container action.
A one-off run outside the selector uses
`execute <staging root> <plan sha256>`; the hash must match that exact root, so
a stale GO cannot authorise a changed plan. There is no environment marker.

Staging is the environment's decision, not the proof's: the fixture stages
under `/Volumes/Builds/scratch/effigy/` and the preflight refuses when the
profile's VM cannot see that path, naming the mount ask. Until the approved
Colima scratch mount lands, `qa:live:consumer-shapes` fails at that preflight —
deliberately, because a silent `/Users` staging root would put proof checkouts
in the operator's home for ordinary QA. Schedule the mount before milestone QA
on this change.

## Related Guides

- [`077-local-dependency-linking.md`](077-local-dependency-linking.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](048-built-in-test-suite-lifecycle-and-env.md)
- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md)
- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)

## Next Step

After finalizing test routing, capture expected machine payloads in [`026-json-payload-examples.md`](026-json-payload-examples.md) for CI consumers.
