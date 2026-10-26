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

## In-test ownership

`effigy-test-support::process` gives a fixture child its own group and stops
that group (TERM, 300 ms, KILL) on `Drop`. `Drop` runs during unwinding, so a
panic is covered. `OwnedChild::group_closed` reads the live group, so a member
that outlived its leader is still reported. This is deliberately the only
in-test mechanism: a ledger, a receipt, a nonce and a pre-exec socket barrier
recorded state the suite now reads from the kernel directly.

`Fixture` removes its tree on drop, waiting for kernel sentinels first
(`wait_kernel_helpers`), and never follows a symlink inside it.

## Placement

Fixtures live in the run's temporary directory: with `[scratch]` configured
that is the run's directory on the Builds drive, so nothing lands in `/tmp`,
`/private/tmp` or `/Users/Shared`. `short_tempdir` and `Fixture::short_home`
keep a path short enough for a Unix control socket by using the temp root, or
its parent (the scratch root) when the root is too long; `SHORT_PATH_MAX` is
the budget the gateway's socket needs. A harness whose temp root cannot bound a
socket is named as such instead of falling back to the internal `/tmp`.

Shell harnesses follow the same rule through `${TMPDIR:-/tmp}` `mktemp`
templates; `effigy qa:docs:mktemp` fails a bare `mktemp` or a `/tmp`-shaped
template.

## Kernel fault fixtures

The kernel fault-injection sweep is no longer an exception: it runs in the
run's temp root like every other fixture. The kernel fault selectors are its
proof on the Builds volume.
