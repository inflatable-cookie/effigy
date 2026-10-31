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
does not prove group closure, so nothing here self-exits on those grounds and
the sweep never signals a custody anchor, supervisor, sentinel or payload.
Contract 052 settlement owns them: it stops workload members only on that
evidence, stops the anchor last, and then releases the shared checkout lease.
An anchor ends on SIGTERM (Q-014), which retires evidence a harness stopped
rather than leaving it to survive indefinitely.
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
(`wait_kernel_helpers`), and never follows a symlink inside it.

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
root while its owner lives, and stops them by identity once the owner is gone.
The normal cleanup reads that ledger too. A settled `down` clears the listener's
record, so a member captured before the stop would otherwise be lost. The interrupted-parent proof
(`managed_host_adapterless_cleanup_stops_survivors_after_an_interrupted_parent`)
runs that path through the real adapterless fixture.

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
template.

Live Colima container fixtures stage under `~/.cache` until the `effigy`
profile mounts `/Volumes/Builds/scratch`. The profile's `colima.yaml` has
`mounts: []`, and Effigy writes no mounts, so the change is a host edit that
needs a profile restart. Its gate is Q-016 in `questions.md`; no repository
change can close it.

## Kernel fault fixtures

The kernel fault-injection sweep is no longer an exception: it runs in the
run's temp root like every other fixture. The kernel fault selectors are its
proof on the Builds volume.
