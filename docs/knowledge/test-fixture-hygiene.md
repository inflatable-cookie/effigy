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
Colima's share boundary even with a fake transport. Environment reconciler, unit
workspace, bootstrap, CLI, gateway, builder and kill-matrix helpers all use this
owner. The former Python catalog handoff adapter and production catalog-pack
staging were retired with the catalog-pack path in #234; neither is restored by this
fixture migration.

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
The kernel test adapter holds a bounded pre-exec socket barrier while a parent
observer captures the child's identity and group. Payload execution starts only
after registration succeeds, so even an immediate exit has a generation receipt.
The child hook uses only lock-free state and async-signal-safe syscalls, owns no
stdio descriptors, and becomes inert before its sockets are dropped. Failed
launches wake the observer; failed registration rejects exec. Reusing a `Command`
keeps previous hooks inert. The rollback and immediate-exit/reuse controls cover
these boundaries in the kernel and ordinary/shared guard selectors.
The guard and in-process fixture sweep share the same native `Child` through a
mutex. A weak registry is keyed by PID, but native cleanup requires the complete
stored receipt to match (identity, group, nonce and grace). The registry grants
no authority to a modified or detached receipt. The native owner is installed
before its ledger receipt is published, with publication and reaping serialized
by the same owner mutex. Cleanup uses the kernel's
`stop_child` while this native owner pins its group; only a detached registration
uses generation/nonce recovery. Native state caches reaping so a fixture can
settle the child before its guard drops without double-waiting. Stdio remains
on the guard; exposing the raw `Child` would allow callers to release the pin.
The nonce-free background-member proof forces an exited leader and requires
fixture removal to share this still-unreaped native authority. The foreign-
generation proof still refuses its injected receipt while settling every known
native owner, regardless of ledger iteration order.

The foreign-generation proof injects the observed identity of another live
process with the protected child's PID, so the receipt names the child's group
with a generation that is genuinely not its own. A start identity is a clock
tick on Linux: with a 100 Hz clock two processes started inside one tick share
it, and the injected receipt then *is* the child's identity, so the sweep
settles the foreign group and the fixture's `Drop` does not fail. That was the
unexplained non-panic: it depended on which side of a tick boundary the test
process and its child fell. The proof now starts probe processes until the
observed start identity differs from the protected child's, asserts that
difference, and requires the refusal to name `fixture process residue` rather
than failing for any reason. `test:fixtures:foreign-generation:linux` and
`test:task-engine:kill-matrix:linux` run the proof on Linux through the private
disposable-VM route, which also covers the plain-task signal matrix there.

The guard stops the entire group and reaps the direct child on drop, including
unwind. A leader can exit between generation observations after TERM; cleanup
uses the remaining existing 15-second budget to observe verified group closure.
Uncertainty never authorizes another signal or deletion, and a group that remains
live or unobservable still fails and retains its files. Direct-child cleanup
rechecks WNOWAIT exit evidence if generation lookup races exit. Exited direct
leaders remain unreaped until group closure, using `waitid`
with `WNOWAIT`; once the whole group is closed, `wait` reaps immediately so
parent-death observers see the native process lifetime. Detached recovery uses the kernel generation and nonce checks;
unknown evidence fails cleanup and retains the registration. Output collection has a
bounded deadline. `output_owned` configures captured streams and uses the same
owner for synchronous CLI helpers. The CLI fixtures use a process-local mutex for shared libtest state; they no
longer create or stale-reclaim a fixed lock file in the system temporary root.
The old CLI, gateway and private-startup guard implementations
are replaced by this shared owner.

A fake backend's newly spawned detached child is registered while its launch
receipt is available. A private gateway launched with an explicit fixture ledger
publishes its own kernel generation, dedicated group and fresh launch nonce to
that same ledger before the server publishes state or serves listeners. This
handshake runs in all build profiles; without a fixture ledger, the private
startup path is unchanged. Failed registration refuses daemon startup. The old
post-success CLI PID-file registration is removed, so a failed response or killed
client cannot skip ownership. A PID file is never later treated as kill authority. A directory guard settles its registered processes before unsealing
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
`test:fixtures:hygiene:shared:recovery` exposes the final builder partition
separately when an earlier partition fails. Every case remains selected. This preserves the existing deadline origins and
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

The active forced-exit control reaps a detached group leader while its
background child survives with an explicitly cleared environment. This fixture's
detached receipt has no retained group anchor, so recovery still returns
`Unknown`: it retains process and directory receipts and fixture files, sends no
signal, and leaves the descendant able to answer a pipe probe. Production
custody now records an anchor before workload execution (effigy#252). Recovery
can stop nonce-free descendants after their leader is reaped when the recorded
anchor's identity, nonce and group membership verify uninterrupted group
continuity. A lost or unverifiable anchor, or an older unanchored receipt, still
leaves nonce-free survivors as `Unknown` residue; session escapes remain outside
custody. These boundaries are defined in
[contract 052 §5.7, I-P8](contracts/052-runtime-kernel-contract.md#57-ownedprocess-and-signalrouter).

The test closes its private pipe in `finally`, allowing the descendant to exit
cooperatively. A subsequent kernel custody sweep verifies group closure before
the exact retained ledger is removed. `test:fixtures:hygiene:leak-assertion`
runs this control with the ordinary process-leak, forced-exit and runner-timeout
controls; the tracked Unix defect ignore is removed.

Builder recovery additionally uses a fresh owned directory under the host HOME
on Linux: the real Colima shared-path preflight rejects OS temp paths there too.
macOS continues to use `/Users/Shared`. The constructor selects this parent
before isolating its fixture HOME, and all writes remain inside the new directory.
The placement proof requires the shared fixture to pass and an OS-temp control
to remain rejected, adapting the saved #233 fixture-isolation patch without
restoring its replaced sealed-tree guard or changing production validation.

Builder fixtures load their private backend with its harmless `info` operation,
and private host shims with `--version`, before the real command or signal
readiness deadline starts. These bounded launches use the shared process owner.
Setup traces are retained as `state/setup-*`; product call oracles still begin
empty. This follows the kill-matrix candidate-image setup boundary and leaves
the production budgets, input-admission checks and interruption assertions intact.

`test:fixtures:hygiene:environment` runs the environment reconciler fixture family
with the shared directory/process ledger. Its fault-boundary sweep has separate
Up, Down, Reset and Import test cases, each retaining all traced crossings,
EPERM/ENOSPC injections, SIGKILL and recovery oracles. Every scenario uses the
unchanged 180-second nextest deadline instead of sharing one deadline across
four complete sweeps. Fault children use `spawn_owned` and their exact private
HOME; SIGKILL targets only the fault process itself without launching a kill
helper. The suite wrapper still fails and cleans registered residue on timeout.
`test:fixtures:hygiene:revision` groups
kernel registration, ordinary/shared guards, environment fixtures, reclaim
proofs and the kernel ratchet for the registration-barrier change.


`test:fixtures:hygiene:gateway-failure` proves the real detached gateway path.
A debug-only, bounded startup barrier holds `gateway up` after launch but before
a successful response. The actor waits for the real daemon to serve, kills only
its retained CLI child, then panics, exits without destructors, or exceeds the
wrapper timeout. The panic case uses fixture-drop cleanup; exit and timeout use
the separate sweep process with an empty native registry. Each requires a
nonzero result, an ended recorded generation and a removed fixture root; unresolved
ownership fails the proof. A second proof requires exactly one birth receipt
and runs in both debug-asserting and non-debug optimized profiles. Kernel
unknown-ownership retention is covered by the active nonce-free descendant
refusal control described above.
Gateway cleanup tracks its root before issuing `up`, including failed responses.

Historical host residue must be checked separately from a current suite's log.
A clean suite and clean Git status do not establish that earlier detached daemons
are gone. Closeout records exact PID/start identities and private directory
identities; cleanup targets only those authorized generations and roots.


### Plain-runner fixture regressions

`test:fixtures:hygiene:ci-regressions` covers the hosted failures that the
selector environment previously masked. The real gateway failure proof supplies
its own separate-process sweep entrypoint in the test binary, so ordinary
workspace nextest and shared libtest runs need no prebuilt helper or inherited
`EFFIGY_TEST_PROCESS_HELPER`. Panic, forced exit and timeout retain the same
ended-generation and removed-root assertions.

The kernel's own helper fixture is prepared the same way. `kernel-custody-fixture`
builds without the `fault-injection` gate (`fault_point!` is inert outside a test
build, and its helper entries are identical either way), and
`crates/effigy-kernel/tests/fixture_binary.rs` is the preparation proof: cargo
builds a package's binary targets whenever one of that package's integration
tests is selected, so every ordinary `cargo test`/`effigy test` run builds the
fixture and the proof shows it is a capturable helper that serves kernel entries.
Proofs that re-exec it take `EFFIGY_KERNEL_CUSTODY_FIXTURE` when their own
selector builds and names it - `test:release:runtime-storage` for the release
Run's custody and git proof - and otherwise look beside the test executable's
profile directory and in `target/<profile>/`, which covers plain Cargo and
Oxide's uplift of final binaries. Milestone QA's built-in `effigy test` therefore
runs the release Run proofs instead of failing them with a missing binary.

The matrix parent-death controller executes the rendered `sh -lc` command as
an argument vector. An extra `sh -c` wrapper survives under Linux dash and hides
controller death from the lifecycle's parent observation; the fixture no longer
introduces that shell. The five-second EXIT-trap assertion is unchanged.

Store tests that deliberately create a schemaless or symlinked database keep
that data below a `store-case` directory inside their owned fixture. Those
stores never launch kernel helpers. They are not presented as the fixture's
kernel HOME; corrupt real HOMEs still retain their files and ledger when helper
custody cannot be observed. The selector runs all Store cases in ordinary and
shared modes as well as the gateway failure and matrix parent-death cases with
the external helper environment removed.

## Process environment and secrets

The process environment is one shared slot per key, not per fixture. A crate's
environment guard serialises *writers* inside that crate's test binary, but a
secret reader is ordinary product code and is not covered by that guard: a
fixture that leaves a vault passphrase in `EFFIGY_TEST_SECRETS_PASSPHRASE` shares
it with every other test in the binary, so a neighbouring fixture's restore can
remove the value between the write and the read and the reader then reports a
locked vault. That is papercut 8de0a24a's macOS shared-process failure. Contract
[052](contracts/052-runtime-kernel-contract.md) K12 therefore keeps secrets out
of the global environment.

In-process Rhai secret fixtures inject the passphrase with
`install_test_rhai_secret_passphrase` from `effigy-rhai`'s `rhai_secrets` module.
The value lives in the calling thread's memory for the guard's scope, is read
before the environment, and is cleared on success and on unwinding; the seam is
compiled only in test builds. `test:rhai:linked-worktree-shared` runs the
linked-worktree vault mutation in one shared libtest with the rest of the Rhai
secrets family while both passphrase keys are absent from the environment, and
`rhai_secret_passphrase_in_the_process_environment_belongs_to_the_whole_process`
records the environment boundary as a deterministic control. Fixtures that
launch a child process still pass a passphrase through the child's own spawn
environment; that is the boundary K12 permits.
