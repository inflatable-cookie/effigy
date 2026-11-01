# Effigy release procedure

Effigy ships a versioned binary through GitHub Releases and a Homebrew tap. The
release configuration is in [`config/release.toml`](../../../config/release.toml)
and it is one declaration under
[contract 054](054-release-platform-contract.md) §4.4. A human must explicitly
request a release and confirm its version. Only `main` can be tagged. Do not
publish Effigy's internal crates to crates.io.

## The declaration today

`config/release.toml` holds one `semver` identity named `effigy`
(`Cargo.toml` → `workspace.package.version`, tag `v{version}`), the gates, the
certified artifact and the two targets:

| Part | Declaration |
| --- | --- |
| `pre` gate | `preflight` — `sh scripts/release/checks.sh preflight --skip-smoke` (metadata and docs on the source tree; the certify `smoke` gate proves the prepared binary) |
| `certify` gates | `secret-audit`, `ci`, `format`, `test`, `qa`, `build`, `smoke`, and `metadata = { task = "release:metadata" }` — all against the prepared commit |
| artifact | `binary` from `release:artifact`, pattern `target/release/effigy`; certify copies it and records `sha256` |
| target `tag` | one annotated tag per identity, pushed at publish |
| target `binaries` | `task` target `release:publish:binaries`, `check = release:check:binaries`, `verify = release:verify:binaries` |

A release with a non-`tag` target stops at `publish` for approval by default
(contract 054 §10 ruling 3). `effigy release approve <run>` records who approved
and continues.

## Repo tasks that replaced the retired commands

The secret audit pins `v0.14.1` to `20465f1ff690340ee9ef461915ca769da1c76fca`,
refuses shallow or unrelated history, and scans the committed candidate tree
and every post-boundary change. Its [fixture receipt](../../../scripts/release/secret-audit-allowlist.tsv)
records exact path and SHA-256 blob exemptions with their source evidence; a
changed blob is scanned again. The cache key includes that receipt, the rules,
and both commits. Reports identify paths, rules and commits without values.
The fixture dispositions are recorded in [Q-020](../questions.md#q-020-release-secret-audit-history-boundary).

The Effigy-only checks were `effigy release validate|preflight|proof|
check-binary|verify-install|evidence …` until v0.15. They are now repo tasks in
[`config/tasks.toml`](../../../config/tasks.toml) whose code is
[`scripts/release/checks.sh`](../../../scripts/release/checks.sh), and the
commands refuse with these replacements:

| Retired command | Repo task | Wired as |
| --- | --- | --- |
| `effigy release validate` | `effigy release:metadata` | the `metadata` certify gate, and the metadata slice of `release:preflight` |
| `effigy release preflight` | `effigy release:preflight` | the `preflight` pre gate, with `--skip-smoke` (the smoke slice is the `smoke` certify gate) |
| `effigy release proof` | `effigy release:proof` | manual; proves a release asset directory |
| `effigy release check-binary` | `effigy release:check-binary` | used by `release:proof` and by the binary workflow |
| `effigy release verify-install` | `effigy release:verify:install` | the `binaries` target's `verify`, through `release:verify:binaries` |
| `effigy release evidence validate` | `effigy release:evidence:validate` | manual; also the asset check inside `release:proof` |
| `effigy release evidence closeout` | `effigy release:evidence:closeout` | manual; writes a closeout record from captured artifacts |
| `effigy release evidence summary` | `effigy release receipt <run>` | the Run itself |

`release:verify:binaries` checks the GitHub Release assets (four binaries and the
two macOS checksum sidecars), the Homebrew formula version and the
tagged-source install. It is what marks a published release verified in the
receipt. It fails closed when `brew` is missing rather than reporting a release
verified without the tap; pass `--skip-homebrew` deliberately when that is the
intent, and record why.

A gate declared as `task` runs as `effigy <task>` (contract 054 §11), so an
operator's installed `effigy` must be at least as new as the checkout: a stale
one fails the gate loudly while parsing `[release]`. The Run itself always runs
from the source-built binary the `release:run` selector names.

## Before the next publication: the binary workflow's GLIBC step

`.github/workflows/release-binaries.yml` still runs the retired
`./effigy-<target> release check-binary …` in its build job, so that step fails
once a release is cut from a tree that includes this retirement.
`effigy release:metadata` warns about it and names the fix; it does not fail,
because the workflow is outside the release Run. Before publishing:

```
./scripts/release/checks.sh check-binary ./effigy-${{ matrix.target }} --glibc-floor 2.35
```

is the replacement for that step. It needs an approved workflow edit, which
this change does not make.

## Compatibility: `[distribution]` and the shorthand

`[distribution]` retired with the distribution surface (Q-009). A manifest that
still declares it loads and is ignored with one deprecation notice per
manifest, because dropping the table changes nothing else about a v0.14.1
project. Every field Effigy's own release used moved into the repo tasks above
or into `[release]`; no other project used the table.

The single-identity `[release]` shorthand still means what it meant: one
identity named after the repository, gates in `certify`, one `tag` target and no
artifacts. A project keeps working unchanged; it grows identities, artifacts or
targets only when it needs them.

## Workspace and gateway diagnostic disposition before v0.15 freeze

Before freeze, obtain Tom's decision on security lead `8d767d55`: existing gateway
daemon logs can contain raw diagnostics and pre-existing permissive log files
or searchable directories can expose them to other local users. New-file mode
0600 does not repair legacy files and is not global sanitization. Workspace
Phase A apply and workspace preparation separately require qualified private or
discarded output before their production path is accepted. A fake-backend proof
is not a native gateway or release qualification; keep the existing review and
native gates in force.

## Governing v0.15 direction — 2026-10-18

Tom's answered Queue decision `5bcf44e4-7e9d-49ee-99d1-c6a46b47a93b`
supersedes the interim candidate's release authorization and conditional
publication-hold lifting described below:

> "I think I would rather not do interim releases"

Rebuild for v0.15 around a runtime kernel and thoroughly qualify the exact
rebuilt candidate against every bundle-defined workflow that must keep
working. Passing the earlier consumer proofs must not resume the old release
flow. Do not prepare, execute, tag, dispatch publication or publish an interim
release. The earlier authorizations and proof records remain historical
evidence rather than authority to resume that flow.

The agreed direction includes pinned bundles; heavy admission requiring both
explicit `admission = "heavy"` and per-host opt-in; removal of root-bound
gateway functionality; one detached managed supervisor per run; and a CI
workflow correction. Implementation uses Sol High or Opus High, with Astra
High for the hardest work. Approved briefs must explicitly require the
selected author profile and its effective High setting. Independent review
defaults to another capable model, such as Grok High or Deepseek High;
frontier review requires an explicit exception. Healthy reviews already in
progress keep their current reviewer. Low-cost implementation requires a
later explicit exception from Tom.

These are design constraints, not claims of implemented behavior or blanket
permission to edit workflows. The cut list, service-catalog
strategy and heavy-task model remain subject to the operator's Atlas
discussion before rebuild briefs are admitted.

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
2. Dispatch `gh workflow run ci.yml --ref main`. Select the `workflow_dispatch` run for that exact SHA with `gh run list --workflow ci.yml --branch main --commit "$candidate_sha" --event workflow_dispatch --limit 20 --json databaseId,headSha,status,conclusion,url`, and require exactly one match: `scripts/check-release-ci.sh` refuses zero or several and names the candidates, because a sibling dispatch of the same commit is a different run. Verify `headSha`, and wait with `gh run watch <RUN_ID> --exit-status`. Missing, pending, red, cancelled, or different-commit evidence blocks release.
3. Run `effigy release status --check-gates` and `effigy release --plan`. Confirm the target version with Tom. During `v0.x`, PATCH is for compatible fixes; MINOR may break behavior with explicit migration notes. Removing a public API is a break (a `Removed` changelog entry), so it proposes MINOR under `pre-1-0` rather than PATCH. CI installs should pin exact versions.
4. Run `effigy release:run -- --to certify` (heavy admission), which runs the release Run's `validate`, `pre-gates`, `prepare` and `certify` stages in place: it updates `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`, commits the prepared tree, runs the `preflight` pre gate and the `certify` gates, and records the artifact digest of `target/release/effigy`; it publishes nothing. Review versioned install examples in `README.md`.
5. Draft release notes from `effigy changelog extract CHANGELOG.md --version X.Y.Z` for Tom's review. Keep the reviewed notes with the GitHub release; routine release logs are not repository knowledge.

## Execute

1. Run `effigy release gates`; fix any failure and rerun. The `ci` gate checks the hosted run against the exact candidate source SHA.
2. `effigy release run` continues the Run from its `certify` checkpoint and stops at the `publish` approval checkpoint. `effigy release approve <run>` records the approval and continues: `publish` pushes the annotated `vX.Y.Z` tag and dispatches `release-binaries.yml` through the `binaries` task target (`effigy release:publish:binaries` → `gh workflow run`), `verify` runs `release:verify:binaries` and writes the receipt's verification result, and `closeout` pushes the release commit. Use it only after Tom explicitly authorizes the release. An interrupted Run continues with `effigy release resume <run>`, and `effigy release receipt <run>` prints what it recorded.
3. Watch the dispatched `release-binaries.yml` run. A pushed tag alone does not publish the binaries; the Run records the dispatch as the `binaries` destination, and verification reports the GitHub Release and Homebrew results. If the workflow fails, the release is published but unverified: fix forward with a PATCH, never by re-tagging.

## Hosted gate evidence

A `stage = "pre"` release gate may reuse a hosted CI result for a gate explicitly configured with `reuse-hosted-evidence = true`. The result must come from this repository's own GitHub Actions runs, verified through authenticated `gh`, and match the exact commit SHA being released (`HEAD` when the Run validates). The `prepare` stage's version, changelog, and lockfile mutations do not change that SHA, which is why hosted evidence belongs in `pre-gates`, before the release commit exists. An identical tree at another SHA is insufficient. Gates not named for reuse still run under the full configured gate policy, including against the prepared working tree. Missing, pending, failed, ambiguous, wrong-repository, or wrong-SHA evidence fails closed. The release record names every reused gate and links to its run. The configured `ci` proof remains a prerequisite, not blanket permission to skip other gates.

The wait binds the run this release dispatched, never a sibling. It opens a dispatch window when it first needs evidence and accepts only a run of the configured workflow and event, for the exact commit, created inside that window; several matches inside one window fail closed with the candidates named. The bound run's id is journalled on the gate and re-checked by every `effigy release resume`, so a newer dispatch of the same commit cannot replace it. `effigy release gates` has no Run record to open a window in, so it accepts only a commit's single matching run and is equally strict about several.

## Gate checks and an empty Unreleased section

`effigy release status --check-gates` reports the configured gate verdict independently of the optional next-version proposal. An empty `[Unreleased]` section means there is nothing to release, not that the gates failed: passed gates still yield a successful gate check (`gates_passed` / `gate_check_passed`, `Gate check: passed`), while the status keeps `ready: false`, `next_version: null`, and the explicit `unreleased changelog section has no entries` blocker. A failed gate, an invalid changelog, a version mismatch, or an existing tag still fails the check. `release --plan` and `release run` keep requiring a derivable next version.

## Verify and recover

- After artifacts publish, run
  `effigy release:verify:install --tag vX.Y.Z` (or `effigy release:verify:binaries`
  to check the release assets and tap as well) in Effigy's own source checkout. Check the GitHub release and Homebrew tap.
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

[Release workflow guide](../../guides/051-release-orchestration.md) documents the CLI. [Distribution policy](../../guides/049-ci-binary-distribution-and-release-protocol.md) covers the channel and platform matrix. [Guide 062](../../guides/062-distribution-system-guide.md) records the retired distribution surface and where each part went.

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
The catalog-pack support policy and its prepare mutation are retired in
v0.15 with the catalog-pack runtime (Q-005). Release preparation no longer
reads `support/catalog-pack-update.toml`, and
`release.sync-catalog-pack-support-policy` is a manifest error.

### Preparation authorization — 2026-10-06

Tom answered "Go for it" to the proposal to prepare v0.14.0 as a reviewed
release change, complete the remaining gates, and present release notes and
migrations before separate approval to tag and publish. This authorizes
bounded preparation work, including version and catalog-pack compatibility
files, install examples, release notes, and repairs needed to preserve the
existing preparation gates. Tagging, publication, binary-workflow dispatch,
workflow edits, installation and live operations remain outside this ruling.

Gates run against the prepared tree. Gate failure restores every planned file
and writes no prepared state.

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
`admission = "heavy"`, so the launcher's `EFFIGY_ADMISSION_HOOK`, when set,
admits the entire operation before compilation, prepared-file writes and
configured gates (contract [049](049-heavy-validation-admission-contract.md)).
The release built-in alone carries no admission class. This route is for explicitly
authorized preparation, not worker per-task QA, a milestone bypass, or release
execution. It must preserve the configured gates and same-run nested
admission; no direct raw gate bundle, copied prepared
fingerprints or temporary Queue validation setting is a substitute. The
selector itself must be independently reviewed before use. Preparation
approval still does not permit tagging, publishing, installing or live
operations.

The maintained selector is `effigy release:run`, declared
`admission = "heavy"` in [`config/tasks.toml`](../../../config/tasks.toml) and
wired to `./target/debug/effigy release run --to certify {args}`. The stage
boundary is fixed to `certify`, so the selector never tags, pushes or
publishes, and it forwards no subcommand of its own. The Run's `validate`
stage refuses a dirty tree, and `prepare` rolls the tree back when a write, a
lockfile sync or the commit fails, so the selector leaves either a completed
prepare-and-certify checkpoint or the pre-prepare tree.

`effigy --plan release:run` is the runner's no-write task plan: it resolves and
prints the task command without running Effigy. The whole operation runs
inside one admission; nested heavy gate work runs inside it instead of
queueing again, and a failed gate or cancelled run keeps the Run's rollback and
honest non-zero exit. Publishing is a separate, explicitly authorized
`effigy release run` (or `effigy release resume <run>` from the checkpoint).

### Admission for isolated release install verification

Effigy's published binary also needs an isolated install proof after its
release assets are available. The repo task `effigy release:verify:install
--tag vX.Y.Z` installs the tagged source into a temporary root and exercises its
installed command against a fixture. Run that proof through the maintained task
so an admission hook, when set, admits the complete install and fixture run
before the child starts. This source task is a follow-up after v0.14.0; a
published v0.14.0 binary does not provide the command it replaced.

The task is declared `admission = "heavy"` in
[`config/tasks.toml`](../../../config/tasks.toml) and runs
`sh scripts/release/checks.sh verify-install {args}`. Normal task arguments
are shell-quoted by the task runner, so options such as `--tag` reach only the
install proof; forwarded values cannot become another shell command. Invoke it
as `effigy release:verify:install --tag vX.Y.Z`. The task does not change the
installed local Effigy or define another install channel; it installs into its
own temporary root.

This is the planner's authorized post-publication install proof, not worker
validation or release preparation. It does not authorize a tag, publish,
workflow change, or live operation. (v0.15 renamed it from the retired
`release:verify-install` selector, whose command `effigy release
verify-install` also retired; the admission ruling is unchanged.)

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
`SAFETY` comment. The eleven audited callbacks are: eight production `setpgid`
sites (`doctor_ports.rs`, `exec_command/transport.rs`, containers
`spawn_capture_child`, both `effigy-process` spawn helpers,
`effigy-scan` doctor inventory, `effigy-deps` bounded process,
and `effigy-runtime` inherit spawn), two production `setsid` sites
(`host_process.rs` best-effort ignore, gateway daemon `last_os_error`),
and the test-only containers `spawn_stream_child` `setpgid` callback. The
current tree has twelve callbacks: those eleven plus the new test-only
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
`effigy-containers --all-targets` (`participation` `cmp_owned`,
`generated_compose` redundant closure, `tests/compose.rs` needless borrow) keep
their oracles and are linted all-target through `check:rust:postfork-safety`. Private errno, allocation,
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


### Publication hold for local hybrid proof

Tom's later publication ruling applies to decision `0116369e`:

> "I'll bootstral:local first so we can use it locally - we don't need or really
> want v0.15.0 actually released until we've proved this all to be fully working".

CI and preparation may finish, but preserve a successful `certify` checkpoint
without running the Run's `publish` stage, creating the tag, dispatching binary
publication or publishing. The existing owners supply the Reference, compli-me and songsprout
`dev` / `dev:test` proofs and Acowtancy adoption proof on locally bootstrapped
builds. Those proofs grant no runtime or cleanup authority to the release owner.

#### Conditional lifting authority — 2026-10-16

Tom subsequently ruled in the effigy-a6 thread, relayed to the release owner:

> "Accept as is. Lift when ready".

This accepts the reported Underlay results and their disclosed limits:
Reference `dev:test` 161/161 on `v0.15.0+local.7420729.dirty`
(`sha256:9c3ab11b…`), compli-me 185/185 and songsprout 74/74 on
`v0.14.1+local.344f5c6` (`sha256:9498298f…`). These are owner-reported
proofs across two builds, not exact-current-main qualification. The dirty
Reference source was not pinned; a temporary prepare mutation was suggested
but not established as its cause. Root gateway identity for PID 75179 remained
unverified. Tom accepted these caveats for those reported builds. At that
point no replacement Underlay proof was required; the later shared-data ruling
below requires fresh proofs for the expanded candidate.

The initial conditional hold required a passing Acowtancy adoption proof reported by
its existing owner, `2f6b92d1`. Once that authentic result passes, the release
owner may lift the publication hold without another approval request to Tom.
A failed or unclear result keeps the hold in place and is reported to effigy-a6.
Until the condition passes, do not publish, tag, dispatch publication workflows
or publish.

#### Seamless route migration required before the adoption proof

Later on 2026-10-16, Tom ruled in the effigy-a6 thread:

> "This needs a fix in effigy to handle it seamlessly - no errors or refusals unless it fails".

This requires the reviewed container-to-hybrid gateway route migration fix
tracked by task `2f66e3c4-7893-4b6f-9933-ddf86bdaf786` in the v0.15.0
candidate. The hold now requires both that fix's normal reviewed merge and a
passing Acowtancy adoption proof on a local build containing it. Earlier proofs
without the fix do not satisfy that condition. No fresh release Run starts
until this proof passes; publishing, tagging and publication remain held too.
The existing consumer owner admits and runs the proof. The ruling grants no
consumer retry, reset, install or runtime authority to the release owner and
waives no ownership, independent-review or CI requirement.

#### Shared data and stable shared-service identity required in v0.15.0

On 2026-10-16 Tom selected "In v0.15.0" for `data_from`, implementing his
Underlay ruling "Same DB, same s3, all the services should remain the same".
A borrowing environment must use the owner environment's service data volumes
within the same checkout and worktree scope. Activation hands over the data
services; borrower reset, retirement, dead-scope retirement and port reclamation
must not delete the owner's volumes. Manifests without `data_from` must retain
byte-identical output. Task `63a91426-b21b-477a-b46d-d3973f88f143` owns this
approved implementation.

Tom also selected "Yes, in v0.15.0" for pinning shared-service project naming to
zero-key SipHash-1-3 rather than an unspecified `DefaultHasher` implementation.
Existing `effigy-shared-*` names and their volume identities must remain
byte-identical. Task `636fc8a2-ced4-4fc1-8e29-e444750940b5` owns that approved
implementation.

The publication hold requires the route handover, shared-data and stable
shared-service identity fixes, plus the complete environment-handover correction
below, to merge through independent review and all seven exact-head CI checks.
The Underlay hybrid and Acowtancy adoption proofs must then pass on admitted
local builds containing all four fixes.
Earlier proofs remain historical evidence; they do not qualify the expanded
candidate. Existing consumer owners admit and run these proofs. No prepare,
execute, tag, publication workflow or publishing starts before those passing
results. The release owner may lift the hold when these conditions pass without
another approval request, preserving decision `0116369e` and every release gate.

#### Complete environment handover required before the fresh proofs

The reported Underlay proof on source `e0c6ade77` passed shared-data and
borrower-protection cases but failed the live hybrid-to-container switch:
data services were stopped before a later route claim refused the switch,
leaving hybrid listeners without their data services and the target partial.
This partial proof does not qualify the candidate for publication.

Under Tom's seamless-switching ruling, task
`ba4268a3-a647-4797-9b62-a53d01bb2f38` owns the complete environment-handover
correction. Refusal checks must precede handover effects. For paired
`data_from` environments or same-checkout environments with colliding routes,
the approved handover stops the other complete environment cleanly while
keeping its volumes, then starts the target. An environment stopped by handover
must restart through ordinary `up`, rather than being stranded as an
incomplete environment nothing reconciles. Existing ownership and effect-boundary
checks remain required; this ruling waives none of them.

The fresh consumer proofs must use a build containing this correction as well
as the preceding route, shared-data and stable-name fixes. They must exercise
the demonstrated live reverse transition, without a manual prior `down` hiding
the failing path. Publication and new preparation stay held until the existing
proof owners report passing results.

As relayed on 2026-10-18, the Underlay proof requires Tom's direct go before
launch. A peer-relayed go, including one from the release owner, does not
satisfy that requirement. Source and binary handovers may continue, but they
must not launch or reset that proof. The consumer owner retains its finite
allowance and resource boundaries; the publication hold remains until both
required consumer results pass.

On 2026-10-19 Tom narrowed the Underlay proof to `underlay-reference` only,
relayed by underlay-c0 through the effigy-a6 planner. Compli-me and songsprout
are early in development and are out of scope. Acowtancy's separate proof is
unchanged, and Underlay still requires Tom's direct go.

The identified inputs for that proof are Underlay bundle main `f957857` and
`v0.14.1+local.3113774`, SHA-256
`84076b133c59f0656f4a944e6f2923495560b2e602fb72e6a230a6125e087abe`.
This scope ruling does not resume the stopped interim release flow or qualify
the rebuilt candidate: its required cohorts must still be proven against the
exact rebuilt source and binary under the governing v0.15 direction above.

Lifting the hold resumes decision `0116369e` version 2 with no gate waiver:
fresh hosted CI on current clean, pushed main, a fresh prepare with all seven
configured gates, then the supported tag, binary publication and isolated
install verification. Accepted consumer proofs do not substitute for any of
those checks or qualify later source changes.

### Shared-process Rhai generation coverage

The generation marker is process-wide. Test environment restoration alone does
not isolate parallel tests. The production CLI generation and outside-hook
refusal controls run in separate child environments; they own this positive
coverage instead of a redundant library unit test setting the marker globally.
The ordinary missing-required library test remains unchanged, and the focused
release selector runs the Rhai library together with eight test threads.
This changes test placement only; production generation and secret refusal
semantics remain unchanged. The exact interleaving in the earlier prepared-tree
failure was not recorded.
