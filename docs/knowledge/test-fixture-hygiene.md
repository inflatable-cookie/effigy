# Test fixture ownership

`crates/effigy-test-support` owns Rust fixture directories. `Fixture::new`
allocates a private directory under the system temporary directory, or under
`EFFIGY_TEST_FIXTURE_ROOT` when the suite wrapper supplies it. It refuses a root
inside this checkout. Permission proofs can select an external shared parent
with `Fixture::in_parent`; macOS container-path proofs require `/Users/Shared`.
All writes remain within the newly allocated directory. The operator-daemon
fixture selects a short `/tmp` parent for its Unix control-socket pathname;
its daemon retains the existing kernel run and `operator::down` guard. It no
longer discards the directory guard on a failed startup.

Dropping the guard restores owner read/write permissions (and directory search
permission), then removes the entire tree. It never follows a fixture symlink.
Cleanup failure fails a normal test; during unwinding it preserves the suite
registration and reports the failure without a second panic. Tests returning
paths retain a thread-local owner for their nested workspaces and Git remotes.
Process-wide state uses `Fixture::process_lifetime`, whose owners are explicitly
drained at process exit because Rust statics do not run destructors.

## Workspace placement

There are currently no in-repository fixture exceptions. Scheduler retirement
removed the authenticated-parent CWD restriction. `Fixture::workspace` allocates
externally, using `/Users/Shared` on macOS because public container paths enforce
Colima's share boundary even with a fake transport. Unit workspace, bootstrap,
CLI, gateway, builder and kill-matrix helpers all use this owner. The Python
catalog adapter also allocates externally. Production catalog staging remains
runtime-owned rather than a test fixture.

## Suite leak assertion

`python3 tests/scripts/fixture_hygiene.py -- <test runner>` creates a private
suite root and directory ledger. Registered fixtures record their canonical
path and device/inode identity. After the runner exits, the wrapper fails if a
registered directory survives, any new entry remains under `crates/`, or the
suite fixture root contains unregistered residue. It unseals and removes only
its registered, same-owner, exact-identity directories; pre-existing worktree
entries are never removed. It also sweeps after runner timeout or forced exit.

`test:fixtures:hygiene:guards` exercises the guards and a deliberately forgotten
fixture. Negative controls require the wrapper to fail and identify directory
and process leaks, including process exit and runner timeout that skip destructors. `test:fixtures:hygiene:guards:shared` runs the same proofs with shared
libtest processes. Nextest retains its forced-exit and leak timeout policy; the
wrapper bounds the complete shared-process invocation.

## Process ownership

`effigy-test-support::process::FixtureCommandExt::spawn_owned` creates a dedicated
child group and records its kernel `ProcessReceipt` in the private ledger.
The guard stops the entire group and reaps the direct child on drop, including
unwind. Exited direct leaders remain unreaped until group closure, using `waitid`
with `WNOWAIT`; once the whole group is closed, `wait` reaps immediately so
parent-death observers see the native process lifetime. Detached recovery uses the kernel generation and nonce checks;
unknown evidence fails cleanup and retains the registration. Output collection has a
bounded deadline. `output_owned` configures captured streams and uses the same
owner for synchronous CLI helpers. The CLI fixtures use a process-local mutex for shared libtest state; they no
longer create or stale-reclaim a fixed lock file in the system temporary root.
The old CLI, gateway and private-startup guard implementations
are replaced by this shared owner.

A fake backend's newly spawned detached child, and each newly launched private
gateway, are registered while their launch receipt is available. Registration
observes the live kernel identity; a PID file is not later treated as kill
authority. A directory guard settles its registered processes before unsealing
or removing files. The suite wrapper invokes `effigy-fixture-process sweep`
before its directory sweep. The runner receives no inherited outer-command custody
or probe nonce; fixture launches allocate fresh nonces. The wrapper fails for surviving registrations even when cleanup
succeeds, and retains normal nextest timeout/leak policy. If custody remains
unknown, it retains the exact ledger and fixture paths and fails. A changed
directory identity is retained too, including during the wrapper's final cleanup;
`test:fixtures:hygiene:wrapper` proves that refusal. A failed
registration does not prevent settling other known registrations. The directory
guard keeps its `TempDir` from deleting files after such a cleanup failure. Directory
cleanup also accepts paths a test has already removed explicitly.

The kernel's feature-gated `custody::test_support` adapter exposes its existing
identity observation and bounded group recovery for native fixture `Child`
handles. It does not replace journalled production `Custody::spawn`.
`test:fixtures:hygiene:kernel` proves closure after direct-leader exit, registration-failure rollback and
refusal of an inconsistent receipt. Guard proofs additionally cover unwind, foreign-child
preservation, deliberately leaked processes and an exit that skips destructors.

The kill-matrix reclaim proof holds the first `serial` workload at an explicit
barrier, selected only for that holder. It cannot complete before SIGKILL;
successors still have to produce alternating start/end records under the real
Task gate. `test:fixtures:hygiene:reclaim` runs that proof with the suite assertion.

`test:fixtures:hygiene:acceptance` runs the gateway (including the managed matrix),
CLI and builder-recovery fixtures three times under independent suite ledgers.
The shared-process CWD proof restores the original directory even during unwind,
before its fixture can be removed. Documentation path-returning helpers retain
thread-lifetime owners instead of discarding temporary-directory guards.

The CLI remote fake launches its payload through `run_recorded`, which owns the
actual child group before publishing its completion marker. The process helper
and a private test-binary entrypoint provide the same launch path; both are
covered by `test:fixtures:hygiene:regressions`. Recovery fixtures pin nested
script dispatch to the built CLI with the existing scoped executable override,
avoiding a nested Cargo build while the test runner owns its build lock.
Acceptance uses the optimized `fixture-proof` profile with debug assertions enabled:
doctor CLI proofs require its debug-only process evidence seams, while cold CLI
timing proofs retain optimization and bounded per-test and suite deadlines.

The parent-death matrix controller launches its lifecycle actor in a separate,
registered group. Killing and reaping the controller leaves the actor able to
observe parent death and run its EXIT trap. Its host actor registration is
outside the fake container's engine scope, so fake container down cannot kill
its own host cleanup caller.

The forked-workload gate proof retains its helper guard after observing leader
exit, until it has checked gate release while the workload is still alive.
Cleanup therefore cannot turn a leaked-descriptor failure into a passing proof.

The read-only observer in `effigy-test-support/src/kernel_helpers.rs` is separate
from the native process ledger in `src/process.rs`. A fixture HOME can still have
a kernel sentinel finishing settlement after its client has been killed and reaped. Before directory removal, the guard reads
helper generations through the existing `run::recent` and `run::metadata` observations and waits for
those other processes to cease writing. Kernel helper startup and typed `HelperExited` failures remain owned by `supervise`; the fixture adapter adds no handshake or retry implementation. This observer never signals a sentinel
or infers authority from a PID file. Unknown evidence or an expired wait retains
the tree; the suite sweep uses the same check before deleting leaked homes.
`fixture_home_waits_for_kernel_sentinel_after_client_kill` proves the boundary.
The doctor hang negative control uses the shared native child guard as well;
its PID marker remains an oracle rather than cleanup authority.

Doctor Git timeout fixtures freeze their immutable checkout's real index before
starting doctor, then serve that response from the private Git shim. This keeps
an unrelated Git prelude from consuming the unchanged status-hang deadline;
the real doctor still owns the controlled status child's deadline and cleanup.
`test:fixtures:hygiene:doctor-git` covers the ready, serial-observer and delayed
descendant controls without extending budgets or weakening their assertions.

The shared-process selector mirrors nextest's runner-slot reservation for CLI
doctor timing proofs: that family runs in one serial libtest process; the
disjoint remaining CLI family and gateway fixtures keep eight test threads.
Every case remains selected. This preserves the existing deadline origins and
process-leak assertions instead of oversubscribing the timing controls.

The previous-generation authority matrix keeps unstarted children scheduled
beyond the bounded suite window and explicitly stops the holder during startup.
It observes the live lifecycle PID before injecting the stale record; the proof
still requires refusal, no foreign down, one owner-authorized down and closed
owned processes. `test:fixtures:hygiene:startup-authority` runs this case in both
ordinary and shared-process modes without a four-second observation race.

The scan Git command factory's debug-only `EFFIGY_TEST_DOCTOR_GIT_SCRIPT` seam
runs that private shim through `/bin/sh`; production release builds keep normal
Git resolution. Whole-CLI fixtures therefore share the actual bounded probe and
cleanup path without depending on macOS execution of a newly written script.

Ordinary acceptance uses two nextest workers, matching the maintained managed
matrix selector. Each admitted matrix test already launches a private gateway,
supervisor and fake workload processes. The four-worker attempt encountered
backend startup delays and matrix timeouts; the two-worker setting keeps
the existing command and per-test deadlines, scenario count and leak assertions.

Fake detached launchers install a fresh custody nonce before spawning and pass
that same nonce with their launch registration. A second fixture owner of an
`OwnedChild` copies its existing receipt rather than re-reading its environment.
This preserves descendant recovery evidence when a test runner kills the native
parent before Drop. Kernel generation and nonce checks remain unchanged.

The forced-exit negative control also reaps a detached group leader before
leaving its background child behind. The suite must reject and settle that
known-nonce descendant without retaining unresolved ownership.

A reaped leader currently leaves a kernel custody limitation on macOS when the
OS exposes a live descendant's arguments without its environment. Even the
launcher's known nonce cannot be verified from that observation. Recovery must
remain `Unknown` and retain the ledger; the detached-leader negative control
exposes this boundary rather than granting numeric group-kill authority.

The reaped-leader descendant control is retained under a tracked Unix ignore
for papercut `aedb6038-fe8c-4b02-ab6f-a0a7ce8aab50`, owned by
`v015-task-engine`. The planner's 2026-10-20 ruling defers that kernel custody
case from fixture-hygiene acceptance. Ordinary process leaks, forced exits and
runner timeouts remain active controls; no fixture group-kill authority is added.

Builder recovery additionally uses a fresh owned directory under the host HOME
on Linux: the real Colima shared-path preflight rejects OS temp paths there too.
macOS continues to use `/Users/Shared`. The constructor selects this parent
before isolating its fixture HOME, and all writes remain inside the new directory.
The placement proof requires the shared fixture to pass and an OS-temp control
to remain rejected, adapting the saved #233 fixture-isolation patch without
restoring its replaced sealed-tree guard or changing production validation.
