# Effigy release procedure

Effigy ships a versioned binary through GitHub Releases and a Homebrew tap. The release configuration is in [`config/release.toml`](../../../config/release.toml). A human must explicitly request a release and confirm its version. Only `main` can be tagged. Do not publish Effigy's internal crates to crates.io.

## Approved macOS checksum publication change

Tom's authority relayed by the acowtancy planner on 2026-10-01 permits a bounded
release-workflow change for app-managed Effigy installation: starting with the
next release, publish `<asset>.sha256` beside each macOS raw binary
(`effigy-aarch64-apple-darwin` and `effigy-x86_64-apple-darwin`). The sidecar
contains one line of 64 lowercase SHA-256 hex characters, two spaces, the asset
basename, and a newline, computed from the exact bytes being uploaded.
The release pipeline has no artifact-signing step; this change adds checksum
sidecars, not a signed manifest. Existing release gates, binary names and
platforms remain authoritative. This request permits the checksum workflow
edit, not a release, an old-release mutation, or a tag change. Human release
and version authorization is still required.

## Prepare

### v0.14.1 hotfix preparation authorization — 2026-10-07

Tom reported: "OK, launching acowtancy now, gateway appears to be working.
We can start prepping for a hotfix release" after the reviewed gateway
replacement correction landed at `0bc3e0a2`. This is operator-reported local
success, not a planner rerun or a general live recovery certification.

Prepare v0.14.1 through the maintained admitted preparation selector, including
exact-main validation-only CI, the configured gates, version/catalog/lockfile
consistency, release notes and migration limits. The exact production-source
milestone passed setup, QA and teardown with clean settlement, and workspace
validation exited 0. Neither substitutes for the release gates. The
identity-bearing ambiguous old-sidecar recovery extension remains unapproved
and unimplemented; retain its honest refusal and disclose it in the hotfix
notes. This authorization does not permit release execution, tagging,
publication, binary-workflow dispatch, workflow edits or live operations.

1. Start from clean, pushed `main`. Record `candidate_sha=$(git rev-parse HEAD)`.
2. Dispatch `gh workflow run ci.yml --ref main`. Select the `workflow_dispatch` run for that exact SHA with `gh run list --workflow ci.yml --branch main --commit "$candidate_sha" --event workflow_dispatch --limit 1 --json databaseId,headSha,status,conclusion,url`, verify `headSha`, and wait with `gh run watch <RUN_ID> --exit-status`. Missing, pending, red, cancelled, or different-commit evidence blocks release.
3. Run `effigy release status --check-gates`, `effigy release simulate`, and `effigy release prepare --plan`. Confirm the target version with Tom. During `v0.x`, PATCH is for compatible fixes; MINOR may break behavior with explicit migration notes. Removing a public API is a break (a `Removed` changelog entry), so it proposes MINOR under `pre-1-0` rather than PATCH. CI installs should pin exact versions.
4. Run `effigy release prepare --yes --check-gates`. It updates `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and prepared state. In the same release change, update `support/catalog-pack-update.toml` (`as_of_release`, `required_versions`, and, when needed, `oldest_update_capable_release`), `PackUpdateCapability::for_this_build` and its tests. Review versioned install examples in `README.md`.
5. Draft release notes from `effigy changelog extract CHANGELOG.md --version X.Y.Z` for Tom's review. Keep the reviewed notes with the GitHub release; routine release logs are not repository knowledge.

## Execute

1. Run `effigy release gates`; fix any failure and rerun. The `ci` gate checks the hosted run against the exact candidate source SHA.
2. Review `effigy release execute --plan`. `effigy release execute --yes` commits the prepared files on `main` as `release: vX.Y.Z`, creates and pushes the annotated `vX.Y.Z` tag, and clears prepared state. Use it only after Tom explicitly authorizes the release.
3. Dispatch `gh workflow run release-binaries.yml -f tag=vX.Y.Z` and watch the run. A pushed tag alone does not publish the binaries.

## Hosted gate evidence

Release prepare may reuse a hosted CI result for a gate explicitly configured with `reuse-hosted-evidence = true`. The result must come from this repository's own GitHub Actions runs, verified through authenticated `gh`, and match the exact commit SHA being released (`HEAD` at prepare time). Prepare's version, changelog, and lockfile mutations do not change that SHA. An identical tree at another SHA is insufficient. Gates not named for reuse still run under the full configured gate policy, including against the prepared working tree. Missing, pending, failed, ambiguous, wrong-repository, or wrong-SHA evidence fails closed. The release record names every reused gate and links to its run. The configured `ci` proof remains a prerequisite, not blanket permission to skip other gates.

## Gate checks and an empty Unreleased section

`effigy release status --check-gates` reports the configured gate verdict independently of the optional next-version proposal. An empty `[Unreleased]` section means there is nothing to release, not that the gates failed: passed gates still yield a successful gate check (`gates_passed` / `gate_check_passed`, `Gate check: passed`), while the status keeps `ready: false`, `next_version: null`, and the explicit `unreleased changelog section has no entries` blocker. A failed gate, an invalid changelog, a version mismatch, or an existing tag still fails the check. `simulate`, `prepare`, and `execute` keep requiring a derivable next version.

## Verify and recover

- After artifacts publish, run
  `./target/debug/effigy release:verify-install --tag vX.Y.Z` in Effigy's own
  source checkout. Check the GitHub release and Homebrew tap.
  If the checkout's SSH origin cannot authenticate Cargo's public fetch, use
  the supported `--repo-url https://github.com/inflatable-cookie/effigy.git`
  argument after verifying the remote tag identity. This changes transport,
  not the selected tag or verification checks. The verifier installs into a
  temporary root and checks version, fixture tasks, prefixed built-ins and
  JSON help; it does not replace the global binary or prove live gateway
  migration. Preserve the original fetch failure alongside the corrected
  verification receipt.
- If publication fails after tagging, keep the tag. Fix the cause and release the next PATCH; never re-tag a failed release.
- For a broken published binary, pause new publishes, tell consumers the affected version, point install guidance at the last good version, and prepare a PATCH fix.

[Release workflow guide](../../guides/051-release-orchestration.md) documents the CLI. [Distribution policy](../../guides/049-ci-binary-distribution-and-release-protocol.md) covers the channel and platform matrix.

## v0.14.0 preparation boundary — 2026-10-05

Tom requested a release-readiness assessment of all work since v0.13.1,
targeting v0.14.0: code quality, gaps and flaws, documentation and general
readiness. This authorizes the assessment and bounded repairs through approved
Queue briefs, with independent exact-head review and CI. The baseline is the
published v0.13.1 tag, not an intermediate local install.

Readiness requires a complete change inventory, explicit migration guidance
for breaking surfaces, reconciled findings and known limitations, and exact
candidate validation and distribution evidence. A prior passing milestone or
a local consumer smoke alone does not establish release readiness. Preparation
is distinct from publishing: the release execution, tag and binary workflow
still require Tom's explicit release authorization.

Use the [v0.14.0 consumer migration checklist](../../guides/083-v0.14.0-consumer-migration.md)
for upgrade actions; the linked contracts remain the owners of each behavior.
The current catalog-pack policy is checked against Effigy v0.14.0 and the
embedded official pack is v1.1.1. Effigy's release configuration opts in with
`sync-catalog-pack-support-policy = "support/catalog-pack-update.toml"`.
Prepare validates the policy against the current version, then plans its
`as_of_release` and `required_versions` update beside the release version,
changelog, and lockfile mutations. The selected release is added at most once;
existing supported versions, the update-capability marker, and the minimum
support floor are retained. The policy file uses the normal prepare snapshots,
so a failed gate restores it with the other mutations. The capability is
already present in released Effigy, with v0.13.0 recorded as its oldest
supported version today.

### Preparation authorization — 2026-10-06

Tom answered "Go for it" to the proposal to prepare v0.14.0 as a reviewed
release change, complete the remaining gates, and present release notes and
migrations before separate approval to tag and publish. This authorizes
bounded preparation work, including version and catalog-pack compatibility
files, install examples, release notes, and repairs needed to preserve the
existing preparation gates. Tagging, publication, binary-workflow dispatch,
workflow edits, installation and live operations remain outside this ruling.

The version and catalog-pack support policy must be consistent before gates
run against the prepared tree. The opt-in policy mutation is planned with the
version file, changelog, and Cargo lock synchronization, then all gates inspect
the prepared tree. Invalid or stale policy data blocks planning; gate failure
restores every planned file and writes no prepared state. The catalog parser's
version-consistency check remains unchanged.

### Publication authorization — 2026-10-06

Tom answered "Approved" to publishing the prepared v0.14.0 release: supported
release execution, the release commit and annotated tag, dispatch of the
existing binary workflow, and artifact/install verification before announcing
availability. This is separate from the earlier preparation ruling. It does
not authorize workflow edits or live gateway/container migration.

The reviewed prepared state aged beyond its one-hour threshold while awaiting
this approval. Its exact source and all four fingerprints still matched and
all seven gates had passed; the supported `--allow-stale` option acknowledges
that deliberately reviewed age without waiving a gate or fabricating state.

### v0.14.1 publication authorization — 2026-10-08

Tom answered "Authorise" to the inspected v0.14.1 publication proposal:
execute the genuine prepared release, create and push the new annotated tag,
dispatch the existing binary workflow, and verify its artifacts and tagged
consumer installation. This does not authorize workflow edits, live gateway
operations or catalog-pack publication. The immutable v0.14.0 tag remains.

While approval was pending, the prepared state exceeded the one-hour age
threshold. Main and all four prepared-file fingerprints still matched, and
all seven gates had passed. The supported `--allow-stale` acknowledgement
covers that deliberately reviewed age only; source drift and failed gates
remain blockers. Historical readiness and child-termination causes remain
unestablished; passing instrumented assurance is not a retrospective cause
claim or a production termination repair.

### Admission for authorized preparation

Ordinary milestone full QA remains planner-owned through Queue
`project.qa.run`, with its result read through `project.qa.get`. Authorized
release preparation has a different persistence requirement: its mutations
and prepared source fingerprints must stay in the actual clean, pushed main
checkout. Queue milestone QA uses a disposable checkout and cannot supply
that in-place preparation or transferable fingerprints.

Run the complete preparation command through a maintained Effigy task marked
`admission = "heavy"`, so Queue/Nucleus admits the entire operation before
compilation, prepared-file writes and configured gates. The release built-in
alone does not provide heavy admission. This route is for explicitly
authorized preparation, not worker per-task QA, a milestone bypass, or release
execution. It must preserve the configured gates and same-run nested
admission; no direct raw gate bundle, scheduler override, copied prepared
fingerprints or temporary Queue validation setting is a substitute. The
selector itself must be independently reviewed before use. Preparation
approval still does not permit tagging, publishing, installing or live
operations.

The maintained selector is `effigy release:prepare`, declared
`admission = "heavy"` in [`config/tasks.toml`](../../../config/tasks.toml) and
wired to [`scripts/release-prepare.sh`](../../../scripts/release-prepare.sh).
The entry point fixes the subcommand to `release prepare`, rejects `--repo`,
refuses before any effect unless a mutating run names an explicit
`--version <SEMVER>` or an inner `--plan`/`--dry-run` is requested, always
enforces `--check-gates` for the mutating run, and never forwards `execute`,
`resume`, tag or publish. It requires branch `main`, no tracked working-tree
changes, and `HEAD == origin/main`, then builds and runs the Effigy from the
invocation checkout (`cargo run --bin effigy`), never an installed or PATH
binary that may predate the preparation repairs.

`effigy release:prepare --plan` is the runner's no-write task plan: it resolves
and prints the task command without running the entry point. The inner prepare
plan is `effigy release:prepare -- --plan`; that path still builds the
current-source Effigy but performs no mutation. The mutating planner command is
`effigy release:prepare --yes --version 0.14.0`, which becomes
`effigy release prepare --yes --check-gates --version 0.14.0` in this
checkout. The whole operation runs inside one admitted host-run; nested gate
work reuses that run instead of resubmitting, and a failed gate or cancelled
run keeps the built-in's rollback and honest non-zero exit.

### Admission for isolated release install verification

Effigy's published binary also needs an isolated install proof after its
release assets are available. The built-in
`effigy release verify-install --tag vX.Y.Z` installs the tagged source into a
temporary root and exercises its installed command against a fixture. Run that
proof through the maintained `effigy release:verify-install` selector so
Queue/Nucleus admits the complete install and fixture run before the child
starts. This source selector is a follow-up after v0.14.0; the published
v0.14.0 binary does not provide it.

The selector is declared `admission = "heavy"` in
[`config/tasks.toml`](../../../config/tasks.toml) and fixes its command to
`./target/debug/effigy release verify-install {args}`. Normal task arguments
are shell-quoted by the task runner, so options such as `--tag` reach only the
fixed verifier subcommand; forwarded values cannot select `execute`,
`prepare`, or another shell command. Use a compatible source-built Effigy at
`target/debug/effigy`. The selector does not change the installed local Effigy
or define another install channel; the built-in keeps its temporary root.

Invoke the selector through that compatible source-built binary as
`./target/debug/effigy release:verify-install --tag vX.Y.Z`; this avoids
replacing or refreshing the installed local Effigy. The selector is for the
planner's authorized post-publication install proof, not worker validation or
release preparation. It does not authorize a tag, publish, workflow change,
or live operation. Its private acceptance fixture checks fixed argv, refusal
before child start, and inherited host-run behavior without performing an
install or contacting release hosts.

For a public repository whose SSH origin requires authentication, pass the
supported `--repo-url https://github.com/owner/repository.git` option to the
selector. This changes only the verifier fetch URL; it does not require
restoring ambient SSH credentials or changing the installed local binary.

### Unsafe-invariant and public-diagnostics reconciliation

The strict post-v0.13.1 audit's reported unsafe-documentation and public
`Debug` gaps were reconciled in a bounded repair wave. Every reported
production `unsafe` block now carries a per-operation `SAFETY` comment that
discharges its actual libc/FFI contract: descriptor ownership and lifetime,
`MaybeUninit` initialization, `getpeereid`/`getsockopt` length handling, and
the `fork`-to-`exec` `pre_exec` contract. Every `pre_exec` callback under
`src/` and `crates/` now has an allocation-free error path and an adjacent
`SAFETY` comment. The twelve audited callbacks are: nine production `setpgid`
sites (`doctor_ports.rs`, `exec_command/transport.rs`, containers
`spawn_capture_child`, both `effigy-process` spawn helpers,
`effigy-scan` doctor inventory, `effigy-deps` bounded process, demo run,
and `effigy-runtime` inherit spawn), two production `setsid` sites
(`host_process.rs` best-effort ignore, gateway daemon `last_os_error`),
and the test-only containers `spawn_stream_child` `setpgid` callback. The
current tree has thirteen callbacks: those twelve plus the new test-only
missing-binary proof
(`effigy-process` `setpgid_pre_exec_preserves_missing_binary_launch_error`),
which uses the same allocation-free `setpgid` conversion. `setpgid`
failures now use `io::Error::from(nix::Error)`, which preserves errno and
allocates nothing; a failed group setup still fails the spawn. The old
direct mapper (`Error::other(error.to_string())`) returned `ErrorKind::Other`
with a formatted message and no `raw_os_error` (private negative control).
That is not the caller-visible `Command::spawn` error: on rustc 1.97.1 Unix,
std's process layer transmits `raw_os_error().unwrap_or(EINVAL)`
(`library/std/src/sys/process/unix/unix.rs`), so a pre-fix callback error
without `raw_os_error` reached the parent as `EINVAL` /
`ErrorKind::InvalidInput`. The new mapper preserves the true errno through
that pipe. The `setsid` callbacks already
avoided allocation (ignored result, or `last_os_error`) and were left
behaviorally unchanged. This documents and
reconciles existing operations; it changes no trust, uid, mount, lock,
cancellation, scheduling or protocol behavior. Public `Debug` was added only
for the reported `HostRunRoot`, `ScriptContext`, `RouteTableLock` and
`LiveRouteTable` types; descriptor state is redacted, `LiveRouteTable` does not
lock or traverse live routes, and secret-bearing `TokenKeys` remains excluded.
The `rust-toolchain.toml` comment now records the current pin only and claims
no wider MSRV, matching [guide 049](../../guides/049-ci-binary-distribution-and-release-protocol.md).
Passing audit or CI does not assert that all Rust is safe or bug-free.
Named containers test-lint sites that had been excluded from
`effigy-containers --all-targets` (`healthcheck_timer` `cmp_owned`,
`participation` `cmp_owned`, `generated_compose` redundant closure,
`tests/compose.rs` needless borrow) keep their oracles and are linted
all-target through `check:rust:postfork-safety`. Private errno, allocation,
negative-control, argv, group-ownership, and launch-error proofs live under
`test:rust:postfork-safety`.

The numeric PID domain of the gateway's `process_signal_accessible` probe and
signal paths is checked by the gateway PID-domain repair (099, `3991ecbc1`):
`read_pid_file` and `server::process_is_running` run
`effigy_gateway::server::checked_gateway_pid`, which rejects PID 0, PID 1 and
any `u32` above `i32::MAX` before dispatch, and Unix status requires one exact
`ps` PID/stat row. The `kill(pid, 0)` SAFETY comment therefore discharges a
checked positive signed PID rather than an unchecked cast.

The unknown-probe lifecycle gap is closed (101, `3dea18c91`): an unavailable
or ambiguous `ps` is `Unknown`, never collapsed to stopped, and does not
delete PID/version records or start a replacement. Post-fork callback
conversion and named containers test-lint sites are closed by task 100
(`test:rust:postfork-safety`), not by this unsafe-invariant wave.

The gateway identity correction is implemented and merged in task 104 (PR
220). Current sidecar behavior, legacy refusal, the bounded macOS reader, and
remaining limitations are owned by
[architecture 020](../architecture/020-container-infrastructure-design.md#gateway-process-identity).
The prior PID-only counterexample is historical, not current main behavior.
The last identity comparison and signal syscall still have a TOCTOU gap, and
a live cross-UID read of an elevated daemon was not exercised. Q-001 and Q-002
are answered. The sidecar and fail-closed policy satisfy Q-001's required
correction; publication still requires the release assurance and explicit
operator authorization above. Passing release checks does not remove the
disclosed live-test and signal-race limitations.

### v0.15.0 preparation and publication authorization — 2026-10-16

Tom answered "Go for it" to the concrete v0.15.0 proposal from reviewed main
`ec4130a1d4e6547deaa8aa6a4acae5709206574c`. Operator decision
`0116369e-b234-4299-a5d0-32cdc3d9e0b4` version 2 records the relayed ruling:
"Authorize v0.15.0 preparation, tagging, publication and verification after all
gates pass".

This permits exact-candidate validation CI, the maintained heavy-admitted
in-place preparation with explicit `--version 0.15.0`, all configured release
gates and version/catalog/lockfile consistency, supported execution of the
reviewed prepared state, the new annotated tag, the existing binary publication
workflow, and tagged isolated install/artifact verification. The default
PATCH proposal is not the selected MINOR version. The existing public-update
capability remains `Present` and the 0.13.0 support floor is retained.

It does not permit workflow edits, retagging a failed release, host installation,
live consumer or gateway migration, or cleanup. Optional dead-scope retirement
is not a release prerequisite and needs its own actual operator authority.
Older QA and private artifact receipts remain dated to their original source;
none substitutes for this candidate's release gates or publication evidence.

### Private catalog fixtures during version preparation

Release gates run against the selected prepared version, including its compiled
package version. Positive catalog-pinning fixtures admit that exact build; their
retained-content and source/selection drift assertions remain independent of a
historical published compatibility range. Baseline doctor fixtures that require
no findings use the existing scoped catalog-home test override, so operator
pack selection cannot silently become fixture input. This does not change
production compatibility validation or suppress unhealthy-pack findings. Host
pack migration is a separate operator action, not a way to make tests pass.

### Managed-listener fixture transactions

Private tests that deliberately forge and restore routes while managed
supervisors run must hold the existing route-table transaction lock across
load/edit/save, just as lifecycle registration does. Atomic rename protects
readers from partial JSON; it does not serialize read/modify/write transactions.
The dependent-replacement fixture exercises the native exclusion and preserves
a concurrent lifecycle registration. Network and lifecycle readiness waits stay
outside those mutation transactions.

Candidate CI `38858608096` at `8729ab01` failed one Linux sibling HTTPS assertion
with a managed-listener ownership refusal. Its log did not preserve the inner
predicate; one unchanged private macOS reproduction passed. The unlocked fixture
edits are a demonstrated race repaired separately, not a proven historical cause
for that CI refusal. Private HTTPS failure diagnostics now include the fixture's
gateway stderr. Production ownership checks, deadlines and release gates remain
unchanged; future candidate evidence must pass in its own right.

### Supervision-initialization fixture exclusion

Initialization-failure proofs must hold the existing combined signal-proof and
reentrant environment lock from before injection through owned-tree cleanup.
Holding only the signal-proof lock does not exclude ordinary runner tests,
which use the environment lock. Such a caller can install the process-wide
shared scope during fixture readiness, so the next owned wait legitimately
reuses it without reaching the injected first initialization. Production
shared-scope reuse and signal behavior are unchanged.

The controlled interleaving uses a private owned child and an ordinary
scope-owning thread, created at the observed owned-child wait boundary after
fixture-lock acquisition, so lock-queue time cannot consume its observation
clock. The native test-lock waiter records the ordinary caller queued until the
real injected failure, exact-owned cleanup and unrelated sibling proof complete. The old group-only control must fail, with unconditional
thread settlement. Candidate preparation `31b0634c` at `c780fc69` retained the
unexpected-success failure, rolled back and settled failed without escaped
descendants. The precise competing historical thread is unknown; new source
still requires independent review, candidate CI and every prepared-version gate.


### Listener-handoff fixture descriptor ownership

A handed raw descriptor number can be reused after the handoff closes it.
Post-refusal `F_GETFD` on that number cannot by itself distinguish a leaked
original socket from an unrelated successor. Closure proofs retain an owned
socket duplicate before transfer, pinning the original object, then atomically
duplicate any current slot for read-only identity comparison. Only exact EBADF,
a proven non-socket or a different socket object establishes release of the
original slot; unknown inspection remains a failed assertion. Only the owned
snapshot and witness are closed, never the potentially unrelated observed slot.

Controlled fixtures atomically replace a still-owned descriptor slot and keep
the successor usable, while a genuinely unclosed original still fails. The
real handoff refusal family runs together with eight libtest threads. Production
handoff validation and descriptor cleanup remain unchanged. Preparation
`61f662d8` at `6bd9af00` failed the numeric-only closure assertion and rolled back;
its exact historical successor is unknown. New source checks do not qualify the
prepared 0.15.0 tree or waive any candidate CI or release gate.


### Runtime adapter drift in prepared-version QA

Preparation `8c40d8c6` at `21b297d7` passed the full prepared-0.15 workspace test
gate, then QA refused backend branching in shell preflight/lease bookkeeping
and Compose assembly in workspace catalog preparation. The operation rolled
back and settled failed with no escaped descendants. These calls now use the
existing transport adapter and plan normalization described by
[the architecture boundary](../architecture/022-runtime-architecture-sanity-audit.md#bounded-discovery-and-catalog-preparation).
The real drift guard and allowlists are unchanged; passing source checks does
not replace candidate CI or the actual seven prepared-version release gates.
