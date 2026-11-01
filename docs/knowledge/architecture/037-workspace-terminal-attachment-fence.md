# 037 - Workspace Terminal Attachment Fence (design, draft)

Status: **draft, pre-implementation.** Revision 2, for independent security
re-review. No source change is authorized until this revision passes review.
Owner task: effigy#525. Base: `6b72af5f6`. Integration base: `main` after
PR #659 (#512) merges, or a stack on #659 with its review. See §10.

Scope: admitted workspace terminals (`src/runner/workspace_client/terminal.rs`)
and every transition that invalidates an attachment's owner: explicit release,
EOF, detach, transport loss, liveness-watch release, run closure, interrupted or
dead-owner settlement. Input and resize both.

## 1. Problem (verified at base)

- The pump (`terminal.rs` `pump`, ~442-522) reads `owner_liveness` through the
  journal, then calls `write_once` or `resize_once`. The journal read ends before
  the effect. A release that commits between them is not seen by the effect.
- **Correction to revision 1:** the outbox mutex *is* held across the writer call.
  `write_once` (~573-596) and `resize_once` (~600-616) take `shared.lock()` and
  call `writer.write` / `writer.resize` while holding it. Revision 1 said the
  opposite. §4 changes that.
- `Outbox` derives `Debug` and holds `pending: VecDeque<u8>`. Any `{:?}` of the
  terminal prints input bytes. §4 replaces that derive.

## 2. Process and path inventory

| Path | Process | Locks taken (in order) | Journal |
|---|---|---|---|
| Pump effect (`write_once`, `resize_once`) | serving host (terminal owner) | M (snapshot) → I-sh → F-sh → L0 probe (nonblocking) → journal read (WAL) → writer call → F-sh release → M (commit) | read only |
| Explicit release / EOF / detach / transport / retry (`Attachment::release`, `release_attachment`) | any host process holding the handle | I-ex → F-ex → L2 txn (`release_row`) | write |
| Liveness-watch release (`supervise.rs` → `Run::release_workspace_attachment`) | supervisor | L0 (Run guard) → I-ex → F-ex → L2 | write |
| Run closure (`settle.rs` → `workspace::closed`) | settling host | L0 → S1 pre-read → I-ex, F-ex (sorted, all of S1) → L2 txn: revalidate S2 == S1, then `DELETE` + phase `closed` | write |
| Interrupted / dead-owner settlement (`settle_interrupted_wait`) | recovering host | L0 (recovered gate) → same as closure | write |
| Attachment creation (`open`, `CreateSecondary`/`Owner`) | any host | create lock file (no lock held; file is new and keyed by a fresh id) → L0 **not taken** → L2 txn: check phase, session, count, insert | write |

Notation: L0 = Run owner gate (existing `Gate`). I = per-attachment intent lock.
F = per-attachment fence lock. M = terminal outbox mutex (in-process). L2 =
SQLite writer transaction (`BEGIN IMMEDIATE`).

Only two statements write `run_attachments` (`workspace.rs` `closed`'s DELETE
and `release_row`'s UPDATE); creation inserts at `workspace.rs` ~879. Every
invalidator therefore reaches the fence through `run/workspace.rs`. `#512`-owned
`workspace_channel.rs` call sites keep calling `Attachment::release` unchanged.

## 3. Lock order

Global order: **L0 → I (sorted by attachment id) → F (sorted by attachment id) → L2**.
M is a leaf, outside this order.

Rules:

1. A thread never waits on L0 or I or F while holding L2.
2. A thread never waits on I or F while holding M.
3. A thread never takes a lock earlier in the order while holding a later one.
4. No thread holds F or I on attachment A and requests a lock on A again
   (flock locks per open file description; a second `open` in the same process
   conflicts with itself). Enforced by a per-thread registry that refuses
   reentry with `RunError::broken`.
5. Every wait is bounded. Shared-side acquisitions are `LOCK_NB` polls with a
   deadline. Exclusive-side waits are bounded by `FENCE_WAIT`.
6. No journal transaction spans PTY or lock-file I/O (052 §8.11).
7. No lock conversion (shared↔exclusive). Release and re-acquire instead.

Why it is acyclic:

- Effect side holds at most I-sh and F-sh, in that order, and takes no L0 or L2
  wait. Its L0 use is `Gate::probe` (`LOCK_SH|LOCK_NB`, momentary). A held L0
  exclusive makes the probe return busy, which is `Unknown`, so the effect holds
  without writing. It does not wait.
- Its journal read is a WAL read. It can wait only on `READ_BUSY_WAIT` (250 ms,
  `store.rs:50`). That wait happens while F-sh is held, so the bound on an
  exclusive waiter is (read budget + writer call + slack), not unbounded.
- Invalidators take I-ex before F-ex, so no effect holds F-sh while an
  invalidator holds L2. Once I-ex is held, no new effect can take I-sh, so no new
  F-sh holder can appear (see §5, writer preference).
- L2 holders never wait on I, F or L0. Creation and closure wait only on L2,
  which is always released by the commit or the rollback.

**Residual waits, all bounded:**

| Wait | Bound | Failure result |
|---|---|---|
| Effect I-sh / F-sh (`LOCK_NB` poll) | 50 ms total | hold, no effect, retry next tick |
| Effect journal read | `READ_BUSY_WAIT` = 250 ms | `Unknown`, hold |
| Effect writer call | writer contract: nonblocking (§6) | — |
| Invalidator I-ex / F-ex | `FENCE_WAIT` = 2 s | refused as `Unknown`, nothing committed |
| Invalidator L2 | existing `BusyWait` budget | `Unknown`, rolled back |
| Creation L2 | existing `BusyWait` budget | refused, file left orphaned |

`FENCE_WAIT` must exceed effect hold time (read budget + writer call). A writer
that blocks forces its invalidators to time out. That is fail-closed: the
transition is not committed and the owner stays.

## 4. Pump restructure (outbox must not span I/O)

Current: outbox lock held across the writer call. Target:

- Under M: compute the work (`wants_write`, `wants_resize`), copy at most one
  bounded chunk of `pending` (≤ `MAX_CHUNK`) into a local buffer, or copy the
  `latest` size. Release M.
- Acquire I-sh, F-sh, probe L0, read the owner journal row, then do the one
  writer call. Release F-sh, then I-sh.
- Under M again: drain `written` bytes from `pending`, add to `delivered`, or set
  `applied = size` if the writer accepted it.

Consequences:

- Latest-wins: `applied` is set to the size actually applied. If a newer size
  arrived while the effect ran, the next tick applies it. Unchanged.
- Backpressure: `Ok(0)` and `WouldBlock` keep bytes queued. Unchanged.
- Retire during an effect: the effect already happened and is counted exactly.
  Retire then clears what remains. A retired pump never starts another effect
  (checked under M and again under F-sh).
- The M-held sections are memory operations only, with no I/O and no lock waits.
  Invalidator discards (§7) also take M, so they never wait on F while holding M.

`Outbox` drops `#[derive(Debug)]` for a manual impl that prints counts only.

## 5. Fences and intent (per attachment)

Two files per attachment, both under the pinned state root:

- `workspace-fence/<run-id>/<attachment-id>.fence` (F)
- `workspace-fence/<run-id>/<attachment-id>.intent` (I)

Names are fixed-format, length-bounded hex or UUID strings derived from ids.
Nothing else is written there.

**Writer preference.** Effects need I-sh then F-sh. Invalidators take I-ex, then
F-ex. While I-ex is held, every new effect fails at I-sh (bounded, no effect).
Existing F-sh holders finish their one bounded effect and release. This makes
revocation immediate without a journal publication before the lock, and avoids
relying on flock fairness, which does not exist on either platform.

**Effect (shared) protocol, per effect:**

1. `I-sh` (LOCK_NB, bounded poll).
2. `openat(root_fd, name, O_RDWR|O_NOFOLLOW|O_CLOEXEC)`, no `O_CREAT`.
3. `fstat(fd)`: regular, `st_nlink == 1`, owner == euid, mode `0600`, size 0
   (fence files never hold data).
4. `fstatat(root_fd, name, AT_SYMLINK_NOFOLLOW)` equals the fd's (dev, ino).
5. `F-sh` (LOCK_NB, bounded poll). Release `I-sh`.
6. Re-read the owning row (exact root, session, run, attachment, client, fence
   generation, lifetime) under F-sh. Refuse if any mismatch or missing row.
7. One nonblocking writer effect.
8. Close the fd. `close` releases the flock. No explicit `LOCK_UN` needed, and
   no `dup` of the fd exists.

**Invalidator (exclusive) protocol:**

1. `I-ex` (bounded wait, `FENCE_WAIT`).
2. Open and verify F as above (no create).
3. `F-ex` (bounded wait). If the attachment is already gone (missing row), the
   fence is still required, and `Unknown` is returned if the file is missing.
4. Under M: take the discard count (§7). Under L2: commit the fenced transition.
5. Close F, close I. Only then may an effect proceed, and it re-reads the row.

**Creation** is the only path that creates a fence file. It runs under
owner-authorized effect context and uses `O_CREAT|O_EXCL|O_NOFOLLOW`, mode
`0600`, `O_CLOEXEC`, then `fsync` on the directory. It happens before the L2
insert. The file is keyed by a fresh attachment id and is not yet visible to any
closure, so creating it under no lock is safe. A failed insert leaves an orphan
file, which is benign.

**Retirement.** Fence files are not unlinked during normal operation. They are
removed only by a sweep that takes I-ex and F-ex, verifies the name still maps
to the held inode, unlinks, and releases. Because openers take I-sh before
`open`, no opener can sit between `open` and lock acquisition during a sweep.
Retirement requires the run row to be gone or closed.

**Filesystem check.** Before the first create or open in a state root, the root
must be a local filesystem: APFS or HFS+ on macOS, and ext2/3/4, btrfs, xfs or
tmpfs on Linux, checked with `fstatfs`. Anything else (NFS, SMB, FUSE, unknown)
is refused. `flock` over network filesystems is not reliable.

## 6. Authority and the lease

- `Observe` contexts stay read-only. The pump keeps its `ObserveContext`.
- `WorkspaceTerminal::open` already takes an `EffectContext`. It mints a
  `FenceLease` from it. The lease holds: a bounded child effect context
  (`HostAuthority` required), the attachment identity tuple, and nothing else.
  No public constructor takes an `ObserveContext`.
- Lease acquisition for open must succeed, or `open` returns
  `TerminalRefusal::Unavailable`. A failed or unknown acquisition never
  authorizes an effect, a release, a forced unlock or a settlement.
- The writer contract: `write` and `resize` return `WouldBlock` or `Ok(0)` when
  they cannot proceed immediately, and never block. This is enforced by the
  production writer (#519). The fake writer in tests must assert the contract by
  returning an error when it would block. A blocking writer, if one exists, holds
  F-sh; invalidators then time out and fail closed. It is not unbounded.
- Panic in a writer or the pump: unwinding drops the lease and the fds, and the
  kernel releases the locks. The outbox mutex is poison-tolerant (existing).

**Fork and exec.** `O_CLOEXEC` closes the fence fds at `exec`. The window between
`fork` and `exec` in a child holds an inherited copy of the fd, and so the flock.
That window is bounded by the child's exec. A child that never execs keeps the
lock until it exits. Mitigation: spawn through the existing `Command` path, which
execs immediately. Test: a spawned `sleep` child, the holder drops its fence,
and a probe process must acquire `LOCK_EX|LOCK_NB` (§9, T-CLOEXEC).

**Independent descriptions.** Each `open` creates a fresh open file description,
so a second process is independent. Within one process, two `open`s of the same
file conflict, which is the intended cross-thread exclusion and is why reentry
is refused (rule 4). `dup`, `try_clone` and fd passing are not used on fence
files; the type exposes no clone.

**Platform semantics.** `flock(2)` locks belong to the open file description on
both macOS and Linux. Both support `LOCK_NB`. Neither gives fairness, and neither
converts atomically, so the protocol never converts. `fcntl` record locks are
per process and are not used.

## 7. Discard accounting

The queued bytes live in the pump process's outbox. A fence in another process
can stop new effects, but it cannot read or discard those bytes. Revision 2
resolves this without IPC and without a schema change:

- **Same process (exact).** The terminal registry maps `(run, attachment)` to a
  weak reference to the terminal's `Shared`. An invalidator in the same process
  as the live owner takes M under F-ex and drains `pending`. It records
  `input_bytes = pending.len()` and `geometry_pending = latest != applied`.
  Those counts are exact.
- **Other live process.** Count is `Unknown(OwnerInAnotherProcess)`. Nothing is
  fabricated, and the transition still commits. The discard is performed by the
  owner's own retire.
- **Owner process dead.** Count is `Unknown(OwnerProcessDead)`, established by
  the failed lock or kernel evidence, never by a guess.

The count is returned to the caller in a typed `Discard` value. It is not
persisted, which avoids a journal change. The discard record is not logged, and
no bytes appear in any output, log or journal. Debug output shows counts only.

**Limitation (decision needed).** A cross-process invalidator reports `Unknown`
for pending input whenever the owner is alive in another process. That is the
ruled option (c). Options (a) IPC and (b) a durable pending record are not
pursued without a separate ruling.

## 8. Invalidation paths in detail

**Explicit release / EOF / detach / transport / retry** (`Attachment::release`,
`release_attachment`, `release_row`). Acquire I-ex and F-ex, then L2. The
`mutations_only` check runs before any lock. A row missing or already released
returns the existing refusal or `Ok` (idempotent), and still holds the fence
while checking so no effect is in flight. Ambiguous commit: if the SQLite commit
returns an error after the commit may have happened, the result is `Unknown`. The
caller re-reads. Nothing is treated as released, and no effect is authorized.

**Secondary release.** Only the secondary's own I and F are taken. The owner's
fence and row are untouched. A secondary release never stops the run.

**Liveness-watch release** (`supervise.rs` → `release_workspace_attachment`).
Already runs under L0. Acquire I-ex and F-ex, then L2 `release_row`. A timeout
returns an error, the supervisor retains the watch, and it retries.

**Run closure / settlement** (`settle.rs` line ~357 → `workspace::closed`):

1. Under L0 (the caller holds it), read S1 = the set of `(attachment id,
   lifetime, session, generation)` rows for the run, outside any transaction.
2. Acquire I-ex then F-ex for every member of S1, in attachment-id order, each
   with `FENCE_WAIT`.
3. Open L2. Read S2 with the same query. If S2 ≠ S1 (a row added or removed by a
   concurrent creator), roll back, release all locks, and retry from step 1.
   Maximum 3 attempts, then `Unknown` (incomplete). The run stays open for the
   next settlement attempt.
4. Commit: `closed` phase, `DELETE`, outcome. Commit happens inside the existing
   close transaction, with no I/O in it.
5. Release all F and I.

Creators cannot be blocked by the closure (they take no L0). They are serialized
against the close transaction by SQLite. The creation insert checks
`phase != 'closed'` in its own transaction, so a creator that commits after the
close is refused. One that commits before the close is seen by S2 and forces
a retry.

**Owner already closing.** A second settlement arrives while one is in progress:
it waits on L0. A release arriving during a closure waits on I/F, which the
closure holds, and then sees the row gone (idempotent `Ok`). No duplicate stop.

**Interrupted / dead-owner settlement** (`settle_interrupted_wait`). Same as
closure, under the recovered gate. The dead owner's pump is gone with its
process, so its fence has no holder and the locks are free. The discard count is
`Unknown(OwnerProcessDead)` unless the owner is alive. Settlement never
fabricates a closed state; it commits only the journal transition it performs.

**Process death during a protocol step.**

| Death point | Kernel releases | Journal | Result |
|---|---|---|---|
| Effect holding F-sh, before write | F-sh, I-sh | unchanged | owner live, next effect re-reads |
| Effect holding F-sh, after partial write | F-sh | unchanged | written bytes counted by the kernel; pending bytes `Unknown` |
| Invalidator holding I-ex/F-ex, before L2 | both | unchanged | row still attached; retry |
| Invalidator after L2 commit, before release | both | committed | transition committed; no authority left behind |
| Closure during L2 | both | rolled back by SQLite | no change; retry |

Kernel release of a lock never grants authority. Every effect re-reads the row
under F-sh, and every invalidator commits through L2.

## 9. Test plan (targeted, named selectors)

No validation runs until implementation. The plan:

- **T-REPRO (pre-fix, failing-first).** Reproduce the input and resize races on
  base `6b72af5f6` with a deterministic barrier (pump paused after the owner read,
  EOF committed, pump resumed). Record the nextest IDs.
- **T-ORDER-A.** Release or close commits before the pump's F-sh. Zero old-owner
  effects, typed refusal.
- **T-ORDER-B.** Pump holds F-sh for one effect while release waits. The effect
  completes before the invalidating commit. No later effect. Typed result.
- **T-CONCURRENT-CREATE.** Pre-read / insert barrier: a creator inserts between
  S1 and L2. Closure retries and seals the new row. Second creator after close is
  refused.
- **T-OWNER-GATE.** Closure holds L0; probe returns busy; effect holds without
  writing.
- **T-WAL-BOUND.** A held `BEGIN IMMEDIATE` in another connection. The effect's
  read times out at 250 ms with `Unknown`. Invalidator time out at `FENCE_WAIT`.
  Both bounded.
- **T-PATHS.** Explicit EOF, acknowledged detach, liveness loss, normal closure,
  interrupted/dead-owner recovery, retry after ambiguous commit. Input and resize
  each. A foreign terminal's bytes and geometry untouched.
- **T-ISOLATION.** Owner vs secondary. Wrong root, session, run, generation.
- **T-LOCKFILE.** Replaced inode (rename race), hard link (`nlink`), symlink
  (`O_NOFOLLOW`), wrong owner, wrong mode, missing file, non-local filesystem.
  Each refuses without effect.
- **T-CLOEXEC.** Spawn a child while holding F-ex; drop the holder; a probe
  process acquires F-ex nonblocking.
- **T-WRITER-PREF.** While I-ex is held, a new effect fails at I-sh. Existing
  F-sh holder completes first.
- **T-SEPARATE-PROCESS.** A real child holds F-sh; the parent invalidates. Then
  SIGKILL the holder and check the lock is released, no stale authority is
  granted, and the row is unchanged. Repeat with F-ex. Clean up by recorded PID.
- **T-DISCARD.** Queued input plus coalesced geometry. Same-process count exact.
  Cross-process count `Unknown`. No payload bytes in output, logs or journal.
- **T-PANIC.** Writer panic under F-sh. Lock released by unwind.

Each invalidator path needs its own test. No single path stands in for another.

## 10. Integration and dependencies

- PR #659 (#512) changes `gate.rs`, `store.rs`, `supervise.rs`,
  `workspace_channel.rs`, `workspace_session.rs`, `run/settle.rs`,
  `run/workspace.rs` and `host_dispatch.rs`. This design touches
  `run/workspace.rs`, `run/settle.rs`, `supervise.rs` (call sites only),
  `terminal.rs`, and new fence code. It does not edit `workspace_channel.rs` or
  `host_dispatch.rs`.
- Integration base: after #659 merges. Stacking before then needs #659's owner
  and review. No source merge while #659's holds are active.
- #519 owns production PTY binding and the nonblocking writer. This design
  assumes that contract and does not prove it.

## 11. Limitations and open decisions

1. **Cross-process discard count.** `Unknown` when the owner is alive elsewhere
   (option c). Needs a ruling if an exact count is wanted (options a or b).
2. **Replacement of the fence inode by a same-user process** that can rename
   files under the state root is outside this design's guarantee. Detection holds
   for the protocol window and for inodes held in-process. Cross-process pinning
   of inodes across time would need a journal field (schema). **Ruling needed.**
3. **Writer contract** is assumed of #519. This design does not prove it.
4. **Fork window** between `fork` and `exec` is bounded but not zero.
5. **Discard counts** are not persisted, so a restart cannot recover them.

No claim of live PTY, TLS, native Linux or reconnect proof is made from fake
tests.

## 12. Contract and changelog (at implementation)

- Update `docs/knowledge/contracts/052-runtime-kernel-contract.md` §8.11 with the
  fence guarantee, lock ownership, lock order, and the limitations above.
- `CHANGELOG.md` `[Unreleased]`: a single entry, after review.

## 13. File ownership and sequence

Supervisor assignment (via Chatterbox): #512 A owns the custody spawn and
output files. #525 owns terminal, release and flock. This section records the
exact #525 file list so shared files are flagged before any edit.

**#525 owns (implementation phase):**

| File | Change |
|---|---|
| `src/runner/workspace_client/terminal.rs` | pump restructure, fence lease, manual `Debug` for `Outbox` |
| `src/runner/workspace_client/tests.rs` | FakePty barriers, `T-*` pump tests |
| `crates/effigy-kernel/src/run/workspace.rs` | release, liveness release, `closed()` fence, creation lock files |
| `crates/effigy-kernel/src/run/workspace/tests.rs` | `T-PATHS`, `T-ISOLATION`, `T-LOCKFILE` |
| `crates/effigy-kernel/src/run/workspace_fence.rs` (new) | fence files, `flock`, `fstatfs` checks |
| `crates/effigy-kernel/tests/workspace_fence_holder.rs` (new) | separate-process holder for `T-SEPARATE-PROCESS` and `T-CLOEXEC` |
| `docs/knowledge/contracts/052-runtime-kernel-contract.md` | §8.11 guarantee, lock ownership, limitations |
| `CHANGELOG.md` | `[Unreleased]` entry |

**Shared or flagged (confirm before editing):**

| File | Why | Rule |
|---|---|---|
| `crates/effigy-kernel/src/run/settle.rs` | #525 edits the close transaction (~357) and `settle_interrupted_wait`. Custody settlement (~178–243) uses `custody::settle_effect`. | #525 touches only the close and interrupted-settle regions. Any `settle_effect` change is A's, and must not share a hunk. |
| `crates/effigy-kernel/src/supervise.rs` | #525 edits the liveness-watch release call site (~1460). A's spawn and handoff callers sit at ~8, ~226, ~1035+. | Shared. #525 edits only the liveness call site. A does not edit that control path. Rebase in sequence if both touch the file. |
| `crates/effigy-kernel/src/run/mod.rs` | One-line module declaration for `workspace_fence`. | Shared only if A adds a module. Otherwise #525 alone. |
| `crates/effigy-kernel/src/faults/scenarios.rs` | Fault points for the new protocol steps, if needed. | Not in A's list. Avoid unless a new fault point is required. |

**Not touched by #525:** `custody.rs`, `custody/` (all files), `custody/tests.rs`,
`crates/effigy-containers/**` (environment, `workspace_activation`, creation,
reconcile, exec, process), the lifecycle caller, `crates/effigy-kernel/tests/support/custody-fixture.rs`,
`host_dispatch.rs`, `workspace_channel.rs`. PR #659 touches none of the custody
or containers paths.

**Sequence (proposed):**

1. PR #659 (#512) merges first. It touches `gate.rs`, `store.rs`, `supervise.rs`,
   `workspace_channel.rs`, `workspace_session.rs`, `run/settle.rs`,
   `run/workspace.rs` and `host_dispatch.rs`.
2. A's custody spawn and output change lands or is reviewed next.
3. #525 implementation starts on the post-#659 head, rebases onto A, and resolves
   any `supervise.rs` or `settle.rs` hunk overlap. Each rebase needs the same
   independent review.

**Open items:** the exact file list for A's process-output tests is not in this
doc. Confirm it does not overlap `workspace_client/tests.rs` or
`run/workspace/tests.rs`. The lifecycle caller is not identified. Neither is
expected to overlap, but both need confirmation before implementation.
