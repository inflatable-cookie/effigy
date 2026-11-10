# Test fixture ownership

No test helper outlives its test. The suite owns the sweep; a fixture owns only
its own process group.

## The suite sweep

`scripts/oxide-selector.sh` starts `tests/scripts/suite_guard.py` for every
`nextest` and `test` selector. The guard generates one `EFFIGY_TEST_SUITE`
value, exports it to the runner and holds an open marker descriptor that every
descendant inherits, including a daemon that calls `setsid` and a process the
real `effigy` binary launches. Nothing is recorded and nothing is repaired: the
sweep reads live kernel state once.

After the runner ends — passing, failing, killed by the guard's `--timeout`, or
killed with the guard — the guard lists the current user's processes that still
carry the suite (the marker descriptor on macOS, `/proc/<pid>/environ` on
Linux), naming none whose executable is an installed Effigy (`.local-install`).
It sends TERM, waits two seconds, KILLs the survivors, and **fails the
selector**, printing every PID with its command. A leaked process is the one
hard boundary: it fails the run even when the tests passed.

macOS 26 no longer returns another process's environment (`ps -E`, and
`KERN_PROCARGS2` for a PID other than the caller), so the marker descriptor is
the discovery primitive there; the environment marker still reaches every
descendant and is what Linux reads.

The guard clears the suite's own unit: negative controls live in
`crates/effigy-test-support/tests/suite_guard.rs` — a `setsid` child that traps
TERM and HUP and then panics, and a runner killed mid-test. Both require the
failure report to name the child and the child to be gone afterwards.
`test:fixtures:hygiene:guards` runs them.

### The built-in test route

`effigy test` starts its own suite instead of a selector, so it owns the same
sweep in `crates/effigy-builtin/src/test/sweep.rs`: one fresh token per
invocation, the `EFFIGY_TEST_SUITE` environment for Linux, and a marker file
whose descriptor a host suite's own shell opens (`exec 9>>…`) before running.
That prologue matters because the route runs suites through kernel custody,
whose descriptor plan would otherwise drop an inherited marker. After the last
suite, the sweep names, TERMs and then KILLs whatever still carries the suite
and fails the route with the same one-hard-boundary rule. Container suites run
on another host and are not swept. `test:fixtures:hygiene:builtin-sweep` runs
the `run_manifest_task_builtin_test_sweeps_a_leaked_suite_process` proof: a
suite that leaves a process in its own process group, which the route must name
and reap.

The handoff descriptor controls re-exec a fresh test process for each source
slot (11, 3 and 10). Its launcher first moves inherited non-stdio descriptors
above fd 11, preserving suite markers and runner pipes while leaving the
controlled slots available. Each cell then claims only its own source slot.

### The kernel-custody boundary

Kernel custody is an explicit boundary, not a sweep target. A helper started
through `custody::spawn` inherits no suite marker: the payload's descriptor plan
overwrites slots 3 and 4 and closes everything above them, its environment holds
only the custody nonce, and `__kernel-anchor` exists as positive evidence of the
original group identity. Owner death, a settled checkout or a missing state root
does not prove group closure, so nothing here retires on those grounds and
the sweep never signals a custody anchor, supervisor, sentinel or payload.
Contract 052 settlement owns them: it stops workload members only on that
evidence, stops the anchor last, and then releases the shared checkout lease.
What does retire an anchor is the group itself: it stays while any workload
member is live, and once the group has no live member it exits by itself within a
bounded deadline (effigy#520), so an owner killed without settling leaves no
anchor behind. SIGTERM stays the cooperative exit (Q-014), which retires evidence
a harness stopped rather than leaving it to survive indefinitely.
The §12.1 fault drives use that retirement boundary without racing it: each
scenario pins the anchors it spawned (SIGSTOP) for the length of the faulted
run, because a free-running anchor can retire itself within one poll of its
group closing — legitimately, before settlement reaches it — and the required
`AnchorStop`/`AnchorStopped` crossings then miss delivery under shard load
(effigy#618). A stopped anchor is still positive evidence (`Stopped` verifies),
settlement still stops it last by identity, and recovery stops it the same way,
so the pin freezes exactly the boundary the points observe; dropping it
continues the anchors, so an abandoned run still leaves no anchor behind. The
pin checks the launch's own recorded generation immediately before it stops and
records a pid: a bare state observation, like a bare `kill` return, says nothing
once the anchor has retired and the kernel has given its pid to another process
(effigy#678). The check and the signal are separate calls, so a retirement plus
reuse between them is narrowed, not excluded.
The one pinned proof that has to be positive about its own premise —
`faults_anchor_retirement_delivery_is_pinned_to_settlement` — holds its workload
at the launch's own owner-written stdin until the recorded anchor reads
`Stopped`, then releases it. That workload exit is what makes the pin a premise
rather than a race: a workload that exits as it starts closes its group at once,
so the anchor can retire itself (effigy#520) before a pin runs, and the stop
proof then has nothing of this launch's to observe. The test's first arm keeps
that old premise as an observed control. The source mechanism is that ordering;
the later-stage occurrence of `bounded wait expired: pinned anchor stopped` at
effigy#666 is consistent with it under load, but no retained record identifies
what that run's pin actually reached.
The recovery step's rerun settles on the recovery budget, not the one-second
injected-scenario budget a faulted settle waits out (effigy#618).
The six fault shards rotate the starting shard at each traced crossing. With
six fault types, plain modulo assigned every Stall and its parent-side
inspection to one process. Rotation spreads those jobs without changing the
crossing/fault or observation inventory, increasing concurrency, or changing
any deadline. Each shard retains plan order. Whole-job receipts include HOME
preparation, delivery, recovery, oracle checks and HOME cleanup; the parent
checks that every planned job appears exactly once on its assigned shard.
Progress names unfinished work; a missing cost row is unknown, never zero.
Old versus rotated cost sums are a scheduling model using measured jobs,
not a measured speedup, causal timeout repair or proof of runtime closure.
Tom's operator decision `f6d749dc-b601-4e3b-9eca-c17373752e4f` v2
(2026-11-04) admits this coverage-preserving scheduling improvement on that
basis. It supersedes the causal-repair outcome, not the evidence limits in
[review 5770051968](https://github.com/inflatable-cookie/effigy/pull/756#pullrequestreview-5770051968).

The environment handover timeout in QA `80c1ea79-08d2-4465-8371-a3d0e438359c`
at `34999093f5c6c32b71e1770a3f5364c176bd6fb4` remains unexplained
(600.004s). Planner lead `0d76b0ea-5e87-4ec2-9f47-d4f923ecaa07` owns
that residual and subsequent integrated QA; shard balancing does not close it.
Private evidence manifests under `/Volumes/Builds/scratch/effigy/` are
`636-handover.jGCpYT/MANIFEST.json` (assignment/cost receipts) and
`636-phase-diagnostic.OwegYV/MANIFEST.json` (the additional original-assignment
diagnostic passed in 400.638s without identifying a causal dependency).
The original QA records remain in `milestone-gateway-bundle-integrated.mm55sff6/`.
A custody group that owes the caller no terminal also gets its own session, so a
suite, a nested task session or a harness census never meets a custody helper of
another run in the session it started.
`custody_deleted_checkout_lease_survives_owner_kill_until_positive_group_closure`
proves the anchor and lease survive deletion until closure; the same test proves
the anchor is gone once settlement completes.

Helpers started any other way — `__kernel-sentinel`, the gateway daemon, the
supervisors, fixture shells — inherit the marker and are swept.

## In-test ownership

`effigy-test-support::process` gives a fixture child its own group and stops
that group (TERM, 300 ms, KILL) on `Drop`. `Drop` runs during unwinding, so a
panic is covered. `OwnedChild::group_closed` reads the live group, so a member
that outlived its leader is still reported. This is deliberately the only
in-test mechanism: a ledger, a receipt, a nonce and a pre-exec socket barrier
recorded state the suite now reads from the kernel directly.

`Fixture` removes its tree on drop, waiting for kernel sentinels first
(`wait_kernel_helpers`), and never follows a symlink inside it. An unreadable
journal is never read as proof that no helper exists: the wait keeps retrying
until its bounded budget and then fails the removal naming the last
observation. A fixture that deliberately makes a store unreadable — a corrupt
`kernel.db`, a schema-less one or a symlink standing in for it — is holding
its own test data, not a HOME that launched kernel helpers, so the test
removes that data before its `Fixture` drops and cleanup settles on the
journal's absence (effigy#550). The observation's own controls pin both
halves: unreadable and untrusted journals are never accepted as settled, and
a readable journal with no recorded helper settles without waiting.

An adapterless listener proof starts plain servers through Effigy, so the
child is a generation Effigy supervises, not the fixture's own. Its state file
records the child's PID with its boot and start identity, and the private
gateway's `gateway.identity` records the daemon the same way.
`tests/gateway_private/fixture_sweep.rs` signals a recorded leader only while it
still holds that identity. A background job in the leader's group is captured
while the leader lives, by its own identity, so it is still stopped after the
leader exits. A group member the sweep never captured is left alone once its
leader is gone: nothing then proves it belongs to the record. A PID reused by
the OS, or a changed boot, is never signalled.

The fixture's `Drop` runs the sweep after the proof's own `container down`, so
an unwinding or unsettled proof is covered. A killed process runs no `Drop`, so
each `AdapterlessSetup` also starts a watchdog: it keeps capturing the fixture's
generations and their group members into `sweep-ledger.json` under the fixture
root while its owner lives, and stops them by identity once the owner is gone,
publishing an exit receipt that names its owner's PID and its own boot/start
identity as its final write. The normal cleanup reads that ledger too. A
settled `down` clears the listener's record, so a member captured before the
stop would otherwise be lost. The interrupted-parent proof
(`managed_host_adapterless_cleanup_stops_survivors_after_an_interrupted_parent`)
runs that path through the real adapterless fixture, and its root removal
follows recorded-generation liveness with two positive waits: the watchdog's
exit receipt, and the same kernel helper settlement `Fixture` removal uses
(`close_root`). The owner's detached host-process supervisor may still owe
its stop report and journal settlement after the watchdog killed the listener
child; recorded liveness proves nothing about such writers, and QA
918de4a0's DirectoryNotEmpty removal shows the gap without naming the racer
(effigy#617).

## Gateway publication and readiness

A managed listener's route is published to the endpoint store before the
gateway daemon consumes it, and its certificate is written before the daemon's
SNI resolver loads it. Both steps are asynchronous, so a proof that reads the
store or the listener state has not proven the proxy serves the route. QA
130acca8 surfaced both observations in real private fixtures: a handshake
answered with the TLS `access_denied` alert rustls sends when the SNI resolver
resolves no certificate, and `503 No Route` while the daemon still served its
previous snapshot. Both are not-ready observations. QA cbef4a23 showed the
certificate half can stay not-ready after the daemon has consumed the route:
the live `/_effigy/routes` view named the domain while HTTPS still answered
`tlsv1 alert access denied` for the whole listener budget, because only the
cert-directory watcher loaded SNI and that notification never arrived. The
daemon now reloads the SNI resolver from the selected private root on the same
route observation that applies the snapshot (effigy#737), so a missed
cert-directory event cannot strand a published TLS route.

The gateway-private probes therefore follow evidence instead of a first probe
or a hidden window. `wait_for_listener_https` and `curl_https` poll through a
handshake the daemon cannot serve yet inside a fixture-owned bound: the caller's
own deadline where it has one, otherwise the 30-second managed-listener bound
that matches the widest readiness window those fixtures declare
(`readiness.timeout_secs`, 20-30 s). It is a fixture bound on asynchronous
daemon consumption, not a production timeout. Only the wanted verified response
ends a wait, and a timeout names the last probe, the last response and the
gateway's own record. The adapterless fixtures first wait for
the daemon's applied publication through `GET /_effigy/routes` on the plain
HTTP proxy `gateway up` reports - the same live snapshot the proxy routes from -
so a store read is never mistaken for consumption. A proof that reads the store
before the daemon applies it, or forces its own snapshot, is testing publication
rather than readiness.

`milestone_130acca8_https_waits_poll_through_a_missing_certificate_and_an_unconsumed_route`
holds each producer behind the wait with the real route snapshot loader, TLS
acceptor and private certificate producer: a certificate the resolver has not
loaded, and a route the consumer has not installed past the five-second window
the fixtures used to assume. Each wait must reach the verified response, and the
same control reads an unpublished route as absent from a real daemon's live view
before the publication appears there.

`milestone_cbef4a23_applied_route_serves_verified_https_from_a_late_certificate`
produces the pair in the selected private root, waits for the daemon's live
route view, then requires a CA/SNI-verified response from that domain and a
TLS refusal for `unknown.host.test`. The crate's `milestone_cbef4a23_`
controls prove the route worker loads that pair without a filesystem event.

A server that double-forks and calls `setsid` has left every recorded process
group, so the generation sweep can never name it. The double-fork proof records
that server's own published PID together with the live boot and start identity
while it is alive, and closes it by that identity both on the success path and
on an unwinding proof, reporting the terminal absence it observed. QA 130acca8
leaked exactly such a server from this fixture when the proof panicked before
its own stop. A PID whose start identity has changed is someone else's and is
left alone; the negative control
`milestone_130acca8_double_fork_server_cleanup_leaves_a_foreign_identity_alone`
pins that.

The container workspace handoff fixtures (the scoped-clone switch, the managed
clone lifecycle) acquire their Linux guest artifact from a private
checkout-shaped host: this run's own build hard-linked at
`<root>/target/debug/effigy`, the artifact and a producer-shaped receipt
beside it (`scripts/build-linux-workspace-artifact.rhai`'s field set: the two
paths, `release_triple`, `sha256`, `build_kind=workspace-artifact`,
`completed_at`), and the artifact source set to `local`.
They no longer seed a release-shaped cache and force `download`: a local Effigy
build refusing the published release as its substitute is product truth
(effigy#701), covered by `artifact_acquisition`. The artifact bytes are never
executed - the private guest adapter stands in for the guest - and the fixture
still exercises the real stack-to-listener switch and its ownership assertions.

## The fixture CLI environment

Fixtures that need a clean CLI environment clear it and copy only their declared
host-tool and runtime inputs. `fixture_environment_command`, managed-matrix case
commands and in-process `clean_fixture_environment` use an allowlist; new
agent, runner or queue variables cannot silently enter a fixture. Fixture-specific
values are supplied explicitly after that boundary. Do not extend a strip list
when a new ambient variable appears.

The product classifier independently handles ordinary ambient settings: values
shorter than eight bytes and signed ASCII decimal integers are not inferred to be
secrets. Schema-declared sensitive values and Effigy-injected secrets keep full
protection at every length, as do aliases containing their bytes (contract 052).
Tests of that boundary construct classification inputs directly, rather than
relying on the test runner's environment.

Native prerequisites remain part of a proof. The planner's ruling for task 812
(2026-11-09, Nucleus question `71b475bca204ee3436eb0a4af8d3c44af8490fd9271fec08d139d3f25793f9ec`)
permits a named prerequisite skip only when `GITHUB_ACTIONS=true` and a probe
positively establishes that distinct loopback aliases or the selected real
Compose backend are unavailable. Other probe errors remain failures. Local and
milestone QA still fail when a prerequisite is missing; supported hosts execute
every original assertion. This is not permission to substitute an adapter for a
native-equivalence assertion.

`hosted_prerequisite_skip` records the exact test, missing prerequisite and
detection in stderr and `GITHUB_STEP_SUMMARY`; failure to publish either record
fails the test. Narrow nextest success-output overrides expose those records in
hosted logs. The author lists every gated test and observed skip in the PR.
Call the helper before clearing a fixture's environment. Runner setup that
could satisfy a prerequisite belongs to a separately approved workflow change.

The disposable Docker command adapters model the production Compose
invocation, including the detached bring-up's `--project-directory
<render-dir>` pin: a value-taking long option is two tokens. An adapter whose
argument parser strands a flag's value silently no-ops its `up`/`down`
branches, which shows up as an activation that reports the primary service
not running after a seemingly successful bring-up.

## Placement

Fixtures live in the run's temporary directory: with `[scratch]` configured
that is the run's directory under `<root>/effigy/` on the Builds drive, so
nothing lands in `/tmp`, `/private/tmp` or `/Users/Shared`. `short_tempdir` and
`Fixture::short_home` keep a path below the Unix socket budget by using the
temp root, then its nearest fitting `effigy/` ancestor when a nested selector
adds path components. Outside that namespace only the immediate parent is
considered; an unusable root is named instead of falling back to `/tmp` or the
internal SSD.

Shell harnesses follow the same rule through `${TMPDIR:-/tmp}` `mktemp`
templates; `effigy qa:docs:mktemp` fails a bare `mktemp` or a `/tmp`-shaped
template. A single-quoted literal, a double-quoted mention, and an escaped
dollar-paren or backtick opener are not calls; an unescaped `$(...)` or
backtick substitution inside double quotes is a call and still needs a
`${TMPDIR:-/tmp}` template. The detector is line-local lexical matching of
that subset (eval, aliases, and substitutions that span lines are out of
scope), not a shell parser. `effigy test:qa:static-guards` proves those
diagnostics on an owned synthetic tree, including crate `tests/scripts` and
the guest `mktemp-exempt` annotation; it does not replace the live check.

Live Colima container fixtures stage under `~/.cache` until the `effigy`
profile mounts `/Volumes/Builds/scratch`. The profile's `colima.yaml` has
`mounts: []`, and Effigy writes no mounts, so the change is a host edit that
needs a profile restart. Its gate is Q-016 in `questions.md`; no repository
change can close it.

## Shared-process controls

Controlled capture clocks are local to their test thread. Companion capture or
controller threads enter an explicitly shared clock; unrelated tests keep the
wall clock. One-shot process-global cleanup tests run in their own test process,
and publisher fault hooks belong to the thread that installs them. Preflight
staging hooks are bound to their owned state root, including across the sealing
helper thread; a sibling root must never consume another fixture’s handoff. Follow retry
budget overrides are scoped to the test thread and restored on drop. Proofs of
fixed descriptor numbers or permanently adopted secret grants run their original
assertions in an exact-test child process. Foreground gate controls retain an
unrelated pre-exec fork copy through successful settlement, and separately prove
that an unconfirmed descendant still retains its ancestors’ gates. Dispatch
fixtures own a cleared environment through context capture and final status.

All in-process fixture environment writers use the same reentrant guard. Setup
and bounded read workers retain that ownership until their observations finish.
Start a private server's deadline after acquiring the fixture environment, so
waiting for another fixture cannot consume its serving budget. FIFO timeout
fixtures release their owned writer and replace the FIFO after the assertion;
the fragment deadline proof waits for actual read completion before teardown.

Local submodule fixtures scope `GIT_ALLOW_PROTOCOL=file` under the shared
environment guard, restoring inherited policy afterward. Fake container
lifecycle proofs omit unrelated fixed host publications; their real health
listeners use OS-assigned ports. Status-contract fixtures retain the explicit
port declarations they assert. A shared CLI cohort exercises those lifecycle
proofs together and retains the JSON failure and private backend call logs.

The detached exit-141 writer waits for the returned caller to acknowledge its
reaped client, then counts real writes at its original cadence. A fixed runtime
does not imply a minimum number of writes on a loaded host. The proof retains
its regular-file, no-SIGPIPE, write-count, elapsed-time and cleanup assertions.
Terminal redaction fixtures wait for verified group closure and complete capture
after leader exit before reading their final sink bytes. Leader exit alone does
not prove the final PTY read or redactor flush has happened.
A scratch-retention proof reads the allocated scratch scope directly and asserts
that it has no kernel process effects before abandonment. Subprocess TMPDIR
delivery and custody recovery have their own controls: recording a leader exit
and discarding live custody does not prove recoverable group closure.

An admission hook that tests EOF after report delivery consumes and records that
complete frame before exiting without acknowledgement. Immediate exit races the
decision poll, frame write and EOF read, so it cannot select one error branch.

Umask proofs run in exact-test child processes: restoring the old mask does not
protect another thread that creates a file during the mutation. Trusted unit
fixtures establish and assert their directory/file permissions before testing a
stale executable, so an unsafe directory cannot select an earlier refusal.

Recorded-generation closure requires positive absence, zombie state or an
identity mismatch. An unknown identity or state stays unresolved within the
fixture's existing wait bound; repeated failed observations do not prove closure.

## Kernel fault fixtures

The kernel fault-injection sweep is no longer an exception: it runs in the
run's temp root like every other fixture. The kernel fault selectors are its
proof on the Builds volume.

The adapterless republication fixture holds its initial serving phase until
verified HTTPS succeeds, then keeps the port closed until route withdrawal and
listener state agree. A private control connection releases each phase; elapsed
server uptime does not schedule the outage. Same-child/generation/address
assertions and the real three-miss probes remain. This is owned fixture ordering
proof, not a reconstruction of historical QA scheduling or native qualification.

The producer-ordering discriminator shares that fixture's Python HTTPServer
source and parks the initial HTTP consumer at outage admission. For the original
input, an observer pauses the producer after its real three-second timer and
socket close, before the 25-second recovery wait: the first connection refuses.
For the fixed input, admission reports before waiting for the consumer's release,
so the same consumer receives the expected body. The observer selects a legal
thread ordering, not a historical schedule. The lifecycle case separately keeps
verified HTTPS and route/child/generation/address assertions.

## Bundle import deadline proof

`bundle_pins::first_legacy_cli_plan_finishes_without_waiting_for_sentinel_readiness`
runs the first real CLI plan in a fresh fixture with the original eight-second
limit. With `fault-injection` only, `EFFIGY_TEST_BUNDLE_SENTINEL` can select a
wrapper inside that fixture's canonical private HOME. Ordinary builds contain
no override and always use their own executable.

The wrapper blocks before the real sentinel acknowledges readiness; it is
released only after the CLI returns or reaches its unchanged deadline. The
fixture records the CLI and helper start identities, stops and waits its own CLI
group on timeout, then verifies helper quiescence before removing its root.
Unproved quiescence fails and retains the fixture. Import must finish without
starting a helper; acquisition controls still require real custody and Git's
expected absent-remote error. The controlled first-call failure establishes a
causal source path, not the schedule of an earlier QA occurrence.

Run this case through `test:focused` with `--features fault-injection` and its
exact test filter. `lint:bundle:deadline-proof` covers that feature's targets;
the ordinary bundle controls also run without the feature.

Worktree archive controls use private synthetic scratch roots and retain the explicit archive-base and fixture override routes. They must not probe or move real host archives.

## Test scheduling

Nextest's default suite caps cold-start fixtures at one concurrent test in the
`cold-start` group; the durable supervisor matrix has its own `journal-matrix`
group. These caps bound contention within each class while ordinary tests keep
using the remaining runner slots. All managed-host gateway listener fixtures
share that cold-start cap, including terminal readiness failure. Focused exposure checks use the same cold-start
cap. No test reserves every runner thread; activation deadlines and assertions
remain unchanged.

Hosted nextest runs use four hash partitions on each operating system. Shared-
process CI uses four Effigy library families plus whole CLI, gateway and
remaining-binary cohorts through `scripts/ci-shared-process.py`:

| Library family | Test namespace |
| --- | --- |
| `library-workspace` | `runner::workspace_command::` (all builder and provisioning matrices) |
| `library-core` | `runner::tests::runner_core_tests::` (the core runner fixtures and their hooks) |
| `library-runner` | Remaining `runner::` tests (including their fixture hooks and registries) |
| `library-other` | All other Effigy library tests |

The runner lists the real compiled library, then lists each family's exact
libtest filter selection. Every name, including existing ignored names, must
appear in exactly one family and agree with the namespace map. Missing,
overlapping or misrouted names fail CI. Test bodies, default libtest threads,
ignored status, assertions and deadlines are unchanged on both operating
systems. `test:ci:inventory` proves the real map without executing test bodies;
`test:ci:scheduling` proves the missing/overlap refusal controls.

Planner ruling (2026-11-10, Nucleus question
`5b794989f6fa2d96927bf233121bc6a8e17ec529a7da8b2af7b1af89acd6bbd3`)
admits these hosted families within the duration brief: related fixture/hook
families stay together, while generic EnvGuard/HOME/PATH use can be isolated
between processes. The release gate remains full, unpartitioned
`cargo test --workspace`, retaining cross-family shared-process interference
coverage. Ordinary `effigy qa` and milestone QA retain full unpartitioned
nextest; they do not use the hosted family filters.

The initial remaining-runner cohort passed on Linux in 13m20s but reached the
20-minute cap on macOS with 1,952 of 1,953 tests complete. Separating the intact
`runner::tests::runner_core_tests::` family bounds that tail while retaining the same
inventory checks, default threads and unpartitioned release coverage.

Non-library cohorts discover every compiled workspace target; new binaries enter
the remaining cohort. Library jobs build workspace libraries and the small
bootstrap integration target, which supplies the real CLI without compiling
unassigned integration binaries. Cargo owns runtime cwd/environment and identical
workspace selections prevent per-package feature rebuilds. Hosted jobs omit
debug symbols, retaining the default debug assertions and optimization levels.
The remaining-binary cohort retains Cargo's default whole-workspace selection,
including normal example compilation and doc tests on both operating systems.
Its `--no-fail-fast` keeps doc tests running after a binary fails and preserves
the failure status; `--tests` alone would omit the default example builds.

The healthy headless lifecycle fixture retains its 35-second cold engine start,
but its host window now ends when status positively reports both exec children
exited zero, rather than waiting 60 seconds on each activation. The cold-start
regression, immediate reuse, permission preparation and final cleanup assertions
are unchanged.

Workspace-client fixture contexts and peer replies reuse one observation of
their immutable compiled executable while the active-version input is unchanged.
Changed version inputs call the real observer again, keeping its wire-version
format authoritative rather than reconstructing that version in a fixture. Rehashing that
same file for each context, live-run record and peer response inflated every
matrix cell. Production dispatch still calls the real binary observer, and the
version/digest mismatch controls retain their original assertions.

Planner ruling (2026-11-10, Nucleus question
`6c7a912f7c11c5b27ef562003add8ffc14d8190764a15a73be32ece9c94ba199`)
assigns the fixture regressions exposed by integrating task 830 to task 814.
Guest adapters must model successful optional reads as `P\n` plus bytes or
`A\n` for absence, including hashes and links, and create selector parents
inside their private guest tree. The staged-source preflight models an existing
non-temporary mount without writing there: a sibling under `/tmp` cannot prove
the positive path. Fake volume inspection must also model Compose's default
project-scoped names for mask volumes. These fixture repairs retain every
assertion and deadline; they do not change production behavior.

Provisioning-guest fixtures take the existing reentrant environment lock before
allocating temporary paths, including standalone guest homes and staged files.
Taking it only later in `EnvGuard` can allocate beneath a concurrent builder
fixture's TMPDIR; that fixture then removes the still-needed checkout when its
own test ends. The lock protects allocation, not a larger runner reservation.

The serving-supervisor closure proof binds a distinct loopback client source
port while its listener is still live, then retains the original TCP-refusal
assertion after settlement. An unbound connect to a retired ephemeral port can
self-connect on Linux if its automatically chosen source is the destination.
The hosted failure did not record peer addresses, so that occurrence's cause
is unproven; the bound probe removes this false-positive path without accepting
an open listener or changing the production shutdown path.

The queued-resize lifetime proof releases its owning row from inside the fake
writer's first full resize attempt, then opens the handle. Its receipt replaces
the fixed sleep: the next pump turn must re-read the released row before any
effect. Releasing from the parent and then opening the handle could instead
race a turn that had already observed a live row. The original no-geometry,
retirement and post-retirement assertions and existing wait bounds remain.

## Custody descriptor remapping (effigy#814)

The hosted macOS core cohort exposed a missing secret grant in
`run_manifest_task_required_secrets_generate_missing_key_in_existing_vault`
(PR #884, job 129663652328). Its source descriptor was not recorded, so the
exact cause of that occurrence remains unproven. A bounded production custody
control independently proved the descriptor-plan defect: the source-11 control
preserved its payload, but source 3 delivered empty bytes after a successful
child exit and complete settlement. The startup barrier replaced descriptor 3
before custody copied that handoff source into slot 11.

Planner continuation `e10ce6da0f5bf7f65e13a50e21e155ef26ef46b9f0d79bc430e1fdca6526d81c`
authorizes the bounded production repair in task 814. Before fork, every source
used by the child's `dup2` plan is copied with `F_DUPFD_CLOEXEC` above slot 11,
the highest destination. No destination can overwrite a source still needed.
The existing child sweep closes the temporary copies; parent RAII closes its
copies on return or error. Stdio, startup barrier/status, anchor, cleanup pin
and handoff retain their existing targets and meanings.

The isolated regression retains sources 11 and 3 and adds source 10, covering
handoff, startup-barrier and cleanup-pin target aliases. Each cell uses a fresh
test process, since kernel signal descriptors persist for that process's lifetime. Only descriptors
the cell claimed are replaced. The original payload and child-exit assertions
and settlement requirements remain; ordinary custody terminal tests cover
copies of the harness-owned standard descriptors.

The workspace lifetime integration proof checks the contract's exact causes:
owner EOF settles as `OwnerLost`, while the detached owner stays live until an
explicit `Requested` stop. Its foreign/resolved-stop cells must not mask owner
loss as a requested stop. The production cause mapping is unchanged.

Deploy fixtures invoke their in-process command under the shared environment
lock. Provider phases set temporary context/report variables under a private
production lock; that lock alone does not exclude test fixtures that scrub the
ambient environment. The Linux core cohort exposed a missing provider-report
variable during a pinned-cache status observation. A direct concurrency control
holds the ambient scrubber guard, observes the provider worker queued behind it,
then restores the environment and requires a valid provider plan. No preview,
offline-cache or provider-report assertion is relaxed.

The dependent-replacement gateway proof uses `fixture_listener_address` for its
dependent listener, outside the OS client ephemeral range. A hosted Linux start
was refused because that listener's reserved port already answered before its
child existed; the log did not record the peer identity, so its precise cause
is unproven. The bounded address avoids client traffic taking the dependent's
port. Every app still uses `:0`, preserving automatic allocation in all three
scopes and both text/JSON fronts. Dependency generation, route replacement,
foreign identity, restart-port retention and scope teardown assertions remain.
Startup failures in this proof and the leader-exit proof snapshot listener
owners (`lsof`/`ss`), Linux owner working directories and the owned listener
records before cleanup. An occupied-port refusal alone does not identify a
foreign listener, self-connect or another generation; retain those facts before
assigning a cause.

Functional bootstrap-command fixtures acquire the same clean environment guard
before root allocation, Git setup or bootstrap invocation, as the failing-setup
fixture already did. Hosted shared-process execution caught a Git reader using
a prerequisite fixture's deleted fake-Git path. Hold the guard across the whole
functional case, preserving every selector, plan/schema and failure assertion.

Managed invalid-definition and invalid-reference matrices likewise acquire a
clean environment guard before fixture setup and validation. Their expected
manifest errors must not observe the headless retirement proof's temporary
`EFFIGY_MANAGED_STREAM` flag. The retirement proof keeps all three values and
its before-effects assertions; every matrix cell and expected error stays.
