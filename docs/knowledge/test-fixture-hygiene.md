# Test fixture ownership

`crates/effigy-test-support` owns Rust fixture directories. `Fixture::new`
allocates a private directory under the system temporary directory, or under
`EFFIGY_TEST_FIXTURE_ROOT` when the suite wrapper supplies it. It refuses a root
inside this checkout. Permission proofs can select an external shared parent
with `Fixture::in_parent`; macOS container-path proofs require `/Users/Shared`.
All writes remain within the newly allocated directory.

Dropping the guard restores owner read/write permissions (and directory search
permission), then removes the entire tree. It never follows a fixture symlink.
Cleanup failure fails a normal test; during unwinding it preserves the suite
registration and reports the failure without a second panic. Tests returning
paths retain a thread-local owner for their nested workspaces and Git remotes.
Process-wide state uses `Fixture::process_lifetime`, whose owners are explicitly
drained at process exit because Rust statics do not run destructors.

## In-repository exceptions

The following is the complete set of exception families. They call
`Fixture::authenticated` (the Python adapter calls `owned_fixture` with an
explicit checkout). When `HOST_RUN_TOKEN` is present, contract
[049](contracts/049-heavy-validation-admission-contract.md) requires every nested
task CWD to remain beneath the authenticated checkout root. These owners allocate
one fresh `crates/effigy-parent-fixtures-*` directory. Without a parent token,
they allocate externally. No token is removed, broadened or copied.

| Family | Reason |
| --- | --- |
| Unit `contract_test_support::temp_workspace` | Nested task, catalog, state and managed execution |
| CLI `tests/support::temp_workspace` | Public task/skill/bootstrap/secret execution |
| Bootstrap crate support | Bootstrap task execution and its private Git remotes |
| Gateway managed-host and managed matrix | Public managed lifecycle and container tasks |
| CLI private startup, reset, headless, handover and attached cases | Actual CLI lifecycle entry and nested tasks |
| Direct builder and builder recovery | Rhai execution through the real task engine |
| Shared-render and gateway publication proofs | Nested container helper execution |
| Python workspace catalog handoff | Host task and session execution through real CLI entry |

The outside-root token refusal controls remain external and continue to prove
that foreign CWDs are rejected before task effects. Production host catalog
staging is not a test fixture and retains its runtime-owned staging directory.

## Suite leak assertion

`python3 tests/scripts/fixture_hygiene.py -- <test runner>` creates a private
suite root and directory ledger. Registered fixtures record their canonical
path and device/inode identity. After the runner exits, the wrapper fails if a
registered directory survives, any new entry remains under `crates/`, or the
suite fixture root contains unregistered residue. It unseals and removes only
its registered, same-owner, exact-identity directories; pre-existing worktree
entries are never removed. It also sweeps after runner timeout or forced exit.

`test:fixtures:hygiene:guards` exercises the guards and a deliberately forgotten
fixture. The negative control requires the wrapper to fail and identify the
leak. `test:fixtures:hygiene:guards:shared` runs the same proofs with shared
libtest processes. Nextest retains its forced-exit and leak timeout policy; the
wrapper bounds the complete shared-process invocation.

## Process ownership

`effigy-test-support::process::FixtureCommandExt::spawn_owned` creates a dedicated
child group and records its kernel `ProcessReceipt` in the private ledger.
The guard stops the entire group and reaps the direct child on drop, including
unwind. Exited direct leaders remain unreaped until group closure, using `waitid`
with `WNOWAIT`. Detached recovery uses the kernel generation and nonce checks;
unknown evidence fails cleanup and retains the registration. Output collection has a
bounded deadline. The CLI fixtures use a process-local mutex for shared libtest state; they no
longer create or stale-reclaim a fixed lock file in the system temporary root.
The old CLI, gateway and private-startup guard implementations
are replaced by this shared owner.

A fake backend's newly spawned detached child, and each newly launched private
gateway, are registered while their launch receipt is available. Registration
observes the live kernel identity; a PID file is not later treated as kill
authority. A directory guard settles its registered processes before unsealing
or removing files. The suite wrapper invokes `effigy-fixture-process sweep`
before its directory sweep, fails for surviving registrations even when cleanup
succeeds, and retains normal nextest timeout/leak policy.

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
