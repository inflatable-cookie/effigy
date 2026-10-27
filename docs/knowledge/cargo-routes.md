# Cargo routes

Status: active. Owner: this file. Guarded by `effigy qa:docs:cargo-routes`.

Every Effigy route that runs Cargo is exactly one of:

- **transitioned** — it runs through `oxide`, through
  `scripts/oxide-selector.sh` where it is a selector;
- **pending one-use** — labelled *one-use workspace*, left on Cargo until
  Oxide ships one-use workspace support, because it builds a private
  one-shot workspace with its own `CARGO_TARGET_DIR`;
- **genuine exception** — stays on Cargo, with one reason.

"Transitioned" is the default and needs no row here. The drift selector
`effigy qa:docs:cargo-routes` (`scripts/check-cargo-routes.py`, part of
`qa:docs`) fails on a plain-`cargo` command word in a catalog route or a route
script that this file does not record, and fails on a row whose route no
longer runs plain Cargo. Comments, `cargo-*` binaries, `$CARGO`,
`EFFIGY_OXIDE_ESCAPE=cargo` and a `scripts/oxide-selector.sh exec -- <tool>`
route (the nested `cargo` word belongs to the tool Oxide leases) are not
routes and never need a row.

## Transitioned routes

- Ordinary build, check, clippy, test, nextest and format selectors call
  `sh scripts/oxide-selector.sh <cargo subcommand> …`. The script owns the two
  escapes: `EFFIGY_OXIDE_ESCAPE=cargo effigy <selector>` runs the whole
  selector as plain Cargo, and `GITHUB_ACTIONS=true` does the same on hosted
  runners that have no Oxide. A local run without `oxide` fails loudly and
  names the escape; it never falls back silently. No selector has a
  plain-Cargo sibling task: the whole-selector escape is the only one.
- The built-in test command routes a detected Rust suite the same way:
  `effigy test` runs `oxide nextest run …` (or `oxide test …` for the
  `cargo test` fallback) when `oxide pool describe --json` answers
  `oxide.pool.describe/1`, and plain Cargo otherwise. `EFFIGY_OXIDE_ESCAPE=cargo`
  and a host without `oxide` keep the plain-Cargo route; a present but unusable
  `oxide` fails the run naming the escape. Container-runtime targets and
  `[test.runners]` overrides keep their own command. See
  [contract 038](../knowledge/contracts/038-unified-test-orchestration-contract.md#host-cargo-route).
- The same script owns the private-HOME pool pass-through: when `OXIDE_POOL` is
  unset and `$HOME` is not the account's real home, it resolves the real pool
  from `oxide status` under that home and exports `OXIDE_POOL` before
  `exec oxide`. A review or fixture harness can therefore set a temporary HOME
  without creating a cold pool under it (3.3-3.8 GB per run on the internal
  SSD). It covers every selector and the `exec -- <tool>` route; an unresolved
  pool warns once, naming `OXIDE_POOL`, and the run continues.
- Custom Cargo profiles reach Cargo through the tool: `cargo nextest run
  --cargo-profile <name>` passes through unchanged, and a *modelled* command
  with a custom profile (`cargo test --profile <name>`, `cargo build --profile
  <name>`) runs as `oxide exec -- cargo …`, because Oxide's modelled commands
  own `--profile` and accept only `quick`, `debug` and `release`.
- A tool that runs Cargo itself (the fixture-hygiene and vault-validation
  wrappers) runs as `oxide exec -- <tool> …`: Oxide leases the slot and
  exports the storage environment, and the tool's own `cargo` word stays
  literal.
- Final outputs stay in `<workspace>/target/<profile>/` whatever pool holds
  the intermediates, so a route may keep reading `target/debug/effigy`.
- `install:local` builds this checkout's optimized release binary through
  `oxide build --release --bin effigy` and installs the exact release binary
  Oxide uplifts to `target/release/effigy`; a missing release output fails
  instead of falling back to another profile or to Cargo.
  `EFFIGY_OXIDE_ESCAPE=cargo effigy install:local` runs the same build as plain
  Cargo and produces the same `target/release/effigy`.
- `test:install:local` runs the installer's private fixture suite. The fixture
  builds the driver through the same selector and then stubs `oxide` inside the
  fixture, so the suite exercises release-artifact selection without a real
  release build. `EFFIGY_OXIDE_ESCAPE=cargo effigy test:install:local` runs it
  as plain Cargo.
- The scan covers every repository-relative script path a catalog route names,
  including a crate-owned one
  (`crates/effigy-kernel/tests/scripts/linux-custody.sh`); `./scripts/x.sh` and
  `scripts/x.sh` are one route.
- Rule-2 repair: a test that found a sibling binary by walking up from
  `current_exe()` now resolves `$CARGO_TARGET_DIR/<profile>/` first and keeps
  the sibling path as the fallback, because Cargo's build directory lives in
  Oxide's shared slot. See `crates/effigy-kernel/src/fixture.rs`.

## Recorded routes

| Route | Kind | Rule | Reason |
| --- | --- | --- | --- |
| `scripts/oxide-selector.sh` | transitioned | — | The one place that spells a plain Cargo command: it owns the `EFFIGY_OXIDE_ESCAPE=cargo` and `GITHUB_ACTIONS=true` escapes and the private-HOME `OXIDE_POOL` pass-through for every routed selector. |
| `release.gates.format` | genuine exception | 8 | `config/release.toml` stays on Cargo until formal v0.15: release workflow callers, and hosted runners have no Oxide. |
| `release.gates.test` | genuine exception | 8 | Same: `config/release.toml` gate caller. |
| `release.gates.qa` | genuine exception | 8 | Same: `config/release.toml` gate caller. |
| `release.gates.build` | genuine exception | 8 | Same: `config/release.toml` gate caller. |
| `release.gates.smoke` | genuine exception | 8 | Same: `config/release.toml` gate caller. |
| `scripts/release/checks.sh` | genuine exception | 8 | Release check caller: it proves `cargo install --git … --tag` of the released binary, which is Cargo's own install surface. |
| `scripts/build-linux-workspace-artifact.rhai` | genuine exception | 8 | Container build: the Linux release artifact is built inside the release container with a container-local `CARGO_TARGET_DIR`. |
| `scripts/rehearse-linux-release-container.rhai` | genuine exception | 8 | Container build: same release container rehearse, and it runs the built binary inside it. |
| `scripts/worktree-lifecycle.sh` | genuine exception | 8 | Paseo worktree setup/teardown hook (a warm-merge-style hook) pins `CARGO_TARGET_DIR=target/worktree-hooks` so `CARGO_TARGET_DIR` and `.cargo/config.toml` cannot move the staged binary between the two shells. |
| `tests/scripts/worktree-lifecycle-hooks.sh` | genuine exception | 8 | Cargo is the subject: every case runs against a stub `cargo` that records the pinned target directory and the arguments. |
| `tests/scripts/install-local-bin.sh` | genuine exception | 8 | Cargo is the subject: a stub `cargo` records and refuses any fallback so the fixture can prove `install:local` uses the Oxide-routed release build, and the assertions name that stub. |
| `tests/scripts/assurance-boundary-tests.sh` | genuine exception | 8 | Cargo is the subject: `cargo nextest list --message-format json` is the membership oracle the proof asserts against. |
| `tests/scripts/gateway-linux-adapter.sh` | genuine exception | 8 | Container/VM guest build: the proof deliberately builds and executes inside a Linux container, with a container-local target directory. |
| `crates/effigy-kernel/tests/scripts/linux-custody.sh` | genuine exception | 8 | Container/VM guest build: eight `test:kernel:*:linux` and `proof:runtime-*:linux` routes run this script, which builds and runs `cargo test`/`build`/`clippy`/`nextest` inside a disposable Colima VM with a VM-local `CARGO_TARGET_DIR`. Three more routes use its `task-engine-fixtures` mode for the root task-engine kill matrix and the fixture ledger's foreign-generation refusal (`test:task-engine:kill-matrix:linux`, `test:fixtures:foreign-generation:linux`, `test:task-engine:fixtures:linux`). |
| `tests/scripts/container-render-consumers.py` | pending one-use | 7 | One-use workspace: it copies the tree into a fresh fixture, sets its own `CARGO_TARGET_DIR`, and writes fixture `effigy.toml` routes that build a baseline example. Waits on Oxide one-use workspace support. |
| `tests/scripts/sequence-consumer-baseline.py` | pending one-use | 7 | One-use workspace: it extracts a commit into a private fixture checkout and builds `--profile discovery-proof` there with its own `CARGO_TARGET_DIR`. Waits on Oxide one-use workspace support. |

## Vault-backed Linux proof build

The Parallels Ubuntu arm64 proof VM builds an exact Effigy candidate with plain
Cargo inside the guest. This is a deliberate proof-build exception, not a Mac
selector route: the guest VM and its build output live on Vault, Oxide is not
available there, and the build does not write to the Mac's internal SSD. Record
the source commit and built binary checksum with each Linux proof. The release
workflow is separate and is not invoked for this build.

## Host note: permanent-drive outputs and multi-step runs

With Oxide's permanent-drive placement (`target` is a link to the build
volume), a multi-step Effigy task whose first step rebuilds `effigy` admits its
next step from whatever binary the path names; a state root on another volume
is irrelevant. This is host state, not routing. The routes that used to expose
cross-volume pinning are `proof:containers:isolated-mount-points`,
`test:containers:generations:cli`,
`test:containers:generations:consumers` and `test:doctor:git-timeout-proof`;
they are the ones to re-check on a host that links `target` to another volume.

## Dogfood findings for the Oxide owner

- **Custom profiles on modelled commands.** `cargo test --profile
  discovery-proof` and `cargo build --profile discovery-proof` cannot move as
  written: `oxide test` and `oxide build` consume `--profile` and accept only
  `quick`, `debug` and `release`. Effigy routes those two through
  `oxide exec -- cargo …`, which works, but a first-class answer (pass an
  unlisted Cargo profile through, or name it in the refusal) would remove the
  workaround.
