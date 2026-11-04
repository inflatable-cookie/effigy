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
recovery step's rerun settles on the recovery budget, not the one-second
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

## The fixture CLI environment

A fixture's `effigy` CLI processes never inherit the ambient coding-agent and
queue-harness environment. Those values (`AI_AGENT=pi`, `NORTHSTAR_BASE_BRANCH`,
`CLICOLOR=1`, session paths and tokens) carry no provenance, so Effigy's child
environment classifier marks them secret; a short one then refuses every host
process whose argv mentions it (the substring `pi` is inside `api`, the digit
`1` is inside almost any hashed binary path), and output redaction rewrites
the same values inside a child's captured output, where the proofs read
published domains and recorded pids.
`tests/gateway_private.rs::fixture_environment_command` and the managed-matrix
case command remove the families (the whole `CARGO_*` family - the
fixture binary's path is itself under the cargo target directory, so
`CARGO_TARGET_DIR` and nextest's `CARGO_BIN_EXE_*` collide with any argv
that names the binary - plus `NEXTEST_*`,
`CLAUDE_CODE_*`, `PI_*`, `PASEO_*`, `NORTHSTAR_*`, `AI_AGENT`, `CLAUDECODE`,
`CLAUDE_AGENT_SDK_VERSION`, `CLICOLOR`, `CLICOLOR_FORCE`, `FORCE_COLOR`,
`LS_COLORS`, `LSCOLORS`) before any fixture `effigy` invocation, and every
daemon, supervisor and workload child inherits the stripped environment.
In-process runner fixtures share the test process's own environment, so an
ambient-sensitive one strips the same families for its duration through
`contract_test_support::strip_ambient_agent_environment`. A new ambient
variable of this class is added to the strip list, not worked around in a
proof.

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
