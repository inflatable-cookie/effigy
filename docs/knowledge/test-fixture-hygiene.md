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
