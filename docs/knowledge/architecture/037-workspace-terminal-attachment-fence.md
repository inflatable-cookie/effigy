# 037 - Workspace Terminal Attachment Fence (design, draft)

Status: **draft, pre-implementation.** Revision 3, for independent security
re-review at the exact head that adds it. No source change is authorized until
this revision passes review. Owner task: effigy#525. Design base: `6b72af5f6`.
Implementation base: the independently reviewed integrated #512 head (see §14),
not an arbitrary PR #659 head.

Scope: admitted workspace terminals (`src/runner/workspace_client/terminal.rs`)
and every transition that invalidates an attachment's owner: explicit release,
EOF, detach, transport loss, liveness-watch release, run closure, and
interrupted or dead-owner settlement. Input and resize both.

## 0. Revision 3 changes (from the review at `941ceab`)

| Review item | Revision 3 answer |
|---|---|
| F released before outbox reconciliation | Reconcile happens under M while F-sh is held (§6). |
| Pre-acquisition inode checks insufficient; intent inode retired | One global intent inode, never unlinked. Post-lock `fstat` vs `fstatat` check, bounded reopen, named refusal (§5). |
| Gate probe Held by matching owner treated as Unknown | Matching Held is Live. Non-matching or Free holds the effect (§8). |
| L1 inside L2 in closure | Closure reads S1 after taking the intent lock. Creation is serialized through the same intent lock, so S1 cannot grow. S2 is a revalidation inside L2, never a lock acquisition (§8). |
| Enqueue vs release race | Enqueue is fenced: intent-shared, F-sh, row check, append under M (§6). |
| Newer vs snapshotted resize | Reservation snapshots the size. Reconcile sets `applied` to the snapshot, so a newer size is always applied next (§6, T-RESIZE). |
| Fork before exec may hang | Documented as a lock held until exec or exit; timeout is `Unknown`; no automatic release (§10). |
| WAL BUSY must be induced | T-WAL-BUSY asserts the BUSY error, not only a timeout (§9). |
| Crash after write, before reconcile | `Unknown` for the reserved bytes. No exactly-once or count claim (§10). |
| O_NOFOLLOW checks only the last component | Every parent is traversed by a verified `O_DIRECTORY|O_NOFOLLOW` fd (§5). |
| Supervisor: same-user replacement out of scope | Stated as a residual (§11). |
| Supervisor: writer must be nonblocking | `O_NONBLOCK` on the PTY fd and `EAGAIN` as `WouldBlock`, plus a stalled-child test (§7). |

## 1. Problem (verified at base)

- The pump (`terminal.rs` `pump`, ~442-522) reads `owner_liveness` through the
  journal, then calls `write_once` or `resize_once`. The journal read ends before
  the effect. A release that commits between them is not seen by the effect.
- The outbox mutex **is** held across the writer call. `write_once` (~573-596)
  and `resize_once` (~600-616) take `shared.lock()` and call `writer.write` /
  `writer.resize` while holding it.
- `Outbox` derives `Debug` and holds `pending: VecDeque<u8>`. Any `{:?}` of the
  terminal prints input bytes. §4 replaces that derive.
- The existing `require_live_owner` (`run/workspace.rs` ~1203-1215, ~1243-1290)
  already treats a `Gate::probe` `Held` result from the matching holder as live.
  A live run's supervisor holds the owner gate for its lifetime. A `Held` result
  is the normal case, not a failure.

## 2. Process and path inventory

| Path | Process | Locks, in order | Journal |
|---|---|---|---|
| Effect (`write_once`, `resize_once`) | terminal owner host | I-sh → F-sh (I released) → journal read (WAL) → writer call → reconcile under M → F-sh release | read; no write |
| Enqueue (`WorkspaceTerminal` input or resize acceptance) | any host holding the terminal | I-sh → F-sh → row check → append under M → release | read |
| Explicit release / EOF / detach / transport / retry (`Attachment::release`) | any host holding the handle | I-ex → F-ex → M (discard count) → L2 `release_row` → release | write |
| Liveness-watch release (`supervise.rs` → `Run::release_workspace_attachment`) | supervisor | L0 (Run guard) → I-ex → F-ex → M → L2 → release | write |
| Run closure (`settle.rs` → `workspace::closed`) | settling host | L0 → I-ex → S1 read → F-ex (sorted) → M → L2 (revalidate S2 == S1, then close) → unlink F → release | write |
| Interrupted / dead-owner settlement (`settle_interrupted_wait`) | recovering host | L0 (recovered gate) → same as closure | write |
| Attachment creation (`open` Owner or CreateSecondary; owner admission ~449) | any host | I-sh → `O_CREAT|O_EXCL` of F (pre-txn) → L2 insert (phase, session, count re-checked) → release | write |

Notation: L0 = Run owner gate (existing `Gate`). I = the single state-root
intent inode. F = per-attachment fence inode. M = terminal outbox mutex
(in-process). L2 = SQLite writer transaction (`BEGIN IMMEDIATE`).

Only two statements write `run_attachments` (`workspace.rs` `closed`'s DELETE
and `release_row`'s UPDATE); creation inserts at `workspace.rs` ~879 and the owner
admission at ~449. Every invalidator therefore reaches the fence through
`run/workspace.rs`. `workspace_channel.rs` call sites keep calling
`Attachment::release` unchanged.

## 3. Lock order (single total order)

**L0 → I → F (sorted by attachment id) → L2.** M is taken only after I or F, never
while holding L2, and never while waiting on another lock.

Rules:

1. Locks are taken only in the order above. No thread waits on an earlier lock
   while holding a later one.
2. No thread waits on I, F or L0 while holding L2. No thread waits on I or F while
   holding M.
3. No thread holds F on attachment A and requests F on A again. Per-thread
   registry refuses reentry with `RunError::broken`.
4. Every wait is bounded. Shared acquisitions are `LOCK_NB` polls with a deadline.
   Exclusive waits are bounded by `FENCE_WAIT`.
5. No journal transaction spans PTY or lock-file I/O (052 §8.11).
6. No lock conversion (shared ↔ exclusive). Release and re-acquire instead.

**Why it is acyclic.**

- I is held exclusively only by invalidators and closure, and each takes it
  before any F. Effects, enqueues and creators take I shared, briefly, and never
  wait for an F or L0 while holding it in a way that can cycle: an effect holds
  I-sh only while it opens and locks its F (§5), and F-ex can only be held by a
  thread that already holds I-ex. So I-sh and F-ex never coexist.
- Therefore an effect's F-sh acquisition does not wait on an invalidator. It can
  wait only on a holder that does not exist while I-sh is held.
- Effects hold F-sh during the journal read and the writer call. Their waits
  inside that window are bounded: `READ_BUSY_WAIT` (250 ms, `store.rs:50`) and the
  nonblocking writer (§7).
- Invalidators hold I-ex and then wait for F-ex. Existing F-sh holders finish their
  one bounded effect and release, so F-ex waits are bounded by effect duration.
- L2 holders wait on nothing except SQLite (`BusyWait`). Creators hold I-sh while
  they wait on L2, which is allowed (I before L2). Closure never takes a lock inside
  L2.
- `Gate::probe` is `LOCK_SH|LOCK_NB`, momentary, and never waits.

**Residual waits, all bounded.**

| Wait | Bound | Failure result |
|---|---|---|
| Effect or enqueue I-sh (`LOCK_NB` poll) | 50 ms total | hold, no effect, retry next tick |
| Effect or enqueue F-sh | 50 ms total | hold, no effect |
| Effect journal read | `READ_BUSY_WAIT` = 250 ms | `Unknown`, hold |
| Effect writer call | nonblocking, §7 | `WouldBlock` / `Ok(0)` |
| Invalidator I-ex / F-ex | `FENCE_WAIT` = 2 s | refused as `Unknown`, nothing committed |
| Invalidator L2 | existing `BusyWait` budget | `Unknown`, rolled back |
| Creation L2 | existing `BusyWait` budget | refused; orphan F remains (§10) |

`FENCE_WAIT` must exceed one effect's hold time (read budget plus writer call).

## 4. Pump and outbox (no lock-free writes, no shared I/O under M)

The pump and the connection share the outbox. M protects memory only. The writer
call runs outside M, under F-sh.

Typed reservation, taken under M under F-sh:

- `in_flight: Option<Reservation>` with `{ input_len: usize, resize: Option<TerminalSize> }`.
  `input_len` is the length of the front of `pending` the effect may write, at
  most `MAX_CHUNK`. `resize` is the snapshot of `latest` the effect may apply.
- At most one reservation exists per terminal. The reservation is cleared under M
  when it is reconciled.

Effect sequence (the protocol in §6 wraps it):

1. Under M, under F-sh: if `stop` or `retired`, refuse. Compute `wants_write` and
   `wants_resize`. For the chosen kind, record the reservation and copy the bytes
   or size. Release M.
2. Call the writer (no M held, F-sh held).
3. Under M, still under F-sh: reconcile.
   - Input: drain exactly `written` bytes from the **front** of `pending`. The
     front is the reserved prefix. Only the pump drains, and only under F-sh or as
     the owning pump's retire, so nothing else can change the prefix. Bytes
     appended by enqueue sit behind the prefix and are untouched. `delivered +=
     written`.
   - Resize: on `Ok`, set `applied = snapshot`. A newer `latest` set by enqueue
     during the effect stays, and the next tick applies it. `applied` never
     becomes a size newer than the one actually applied.
   - Clear `in_flight`.
4. Release M, then F-sh.

Consequences:

- Partial write (`Ok(k)`, `k < n`): `k` bytes drained, the rest stays queued.
- `WouldBlock` / `Ok(0)`: nothing drained, reservation cleared, pump idles.
- Retire during an effect: retire runs only on the pump thread or under F-ex, so
  it cannot run during an effect. Retire clears what remains after reconcile.
- A panic between step 2 and step 3 leaves `in_flight` set. The pump guard's
  `Drop` marks the terminal retired, clears `pending`, and reports the reserved
  bytes as `Unknown` (§7). Lock release is by unwind.
- The `Outbox` manual `Debug` prints counts only.

## 5. Fence files and descriptor identity

**Layout (under the pinned state root).**

- `workspace-fence/intent.lock`: **one** intent inode per state root. Created once
  by owner-authorized setup. **Never unlinked, never replaced.**
- `workspace-fence/<run-id>/<attachment-id>.fence`: F, one per attachment.
  Directory names and file names are fixed-format, length-bounded ids. Nothing
  else is written there.

**Traversal.** The state root is opened once as a directory fd from the pinned
`StateRoot`. Each component is opened with `openat(parent_fd, name,
O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC)`, and each directory is checked with `fstat`:
directory, owner = euid, mode with no group or other write. The final file is
opened with `O_NOFOLLOW|O_CLOEXEC`. Parents are never followed by path. If the
existing `StateRoot` exposes only a path, the implementation first adds a verified
directory handle. That is a stated dependency, not a silent change.

**Open-lock-verify (every acquisition of I or F, shared or exclusive).**

1. `fd = openat(parent_fd, name, O_RDWR|O_NOFOLLOW|O_CLOEXEC)`, no `O_CREAT`.
2. `flock(fd, LOCK_SH|LOCK_NB)` or `LOCK_EX|LOCK_NB`, polled to the deadline.
3. `fstat(fd)`: regular file, `st_nlink == 1`, owner = euid, mode `0600`, size 0.
4. `fstatat(parent_fd, name, AT_SYMLINK_NOFOLLOW)`: `(dev, ino)` equals the fd's.
5. On a mismatch at step 3 or 4: close the fd, which releases the lock, and reopen.
   At most 3 attempts. After the third, refuse with `FenceReplaced` and no effect.
6. Only after step 4 holds is the lock treated as held. The check is repeated
   after the lock is taken, not only before.

The fd is never duplicated (`dup`, `try_clone` and fd passing are not used). Each
`open` creates a fresh open file description. Within one process, two `open`s of
the same file conflict, which is why rule 3 forbids reentry.

**Effect protocol (per effect and per enqueue).**

1. Take I-sh (open-lock-verify on `intent.lock`).
2. Open-lock-verify F-sh on `<attachment-id>.fence`. A missing F is refused,
   never created.
3. Release I-sh.
4. Under F-sh: the owner read (§8). Refuse or hold on any mismatch.
5. Reservation and writer call, reconcile (§4).
6. Close F. The kernel releases the lock.

**Creation.** Take I-sh. Create the run directory if absent (`mkdirat`, no unlink
ever). `openat(..., O_CREAT|O_EXCL|O_NOFOLLOW|O_CLOEXEC)` with mode `0600`, then
`fsync` on the directory. Then the L2 insert, which re-checks phase, session and
count inside its own transaction. Release I-sh. Attachment ids are fresh (owner
ids are derived from the run and are created once at admission, and a closed run
cannot be admitted again). `O_EXCL` refuses any existing file, so an id is never
recreated. A failed insert leaves an orphan F, which is benign and never used.

**Retirement.** F is unlinked only by closure, after the L2 commit that deletes its
row, under I-ex and F-ex, and before both are released (§8). Explicit release never
unlinks, because the row still exists in `released` state. No path ever calls
`O_CREAT` for an existing id. A waiter that opened F before the unlink must hold
I-sh, which is impossible while closure holds I-ex. A waiter that opens afterwards
gets `ENOENT` and refuses, and never recreates F.

**Filesystem check.** The state root must be on a local filesystem: APFS or HFS+
on macOS, and ext2/3/4, btrfs, xfs or tmpfs on Linux, checked with `fstatfs`.
Anything else is refused.

## 6. Authority, enqueue and the lease

- `Observe` contexts stay read-only. The pump keeps its `ObserveContext` for
  reads.
- `WorkspaceTerminal::open` takes an `EffectContext` and mints a `FenceLease`
  from it. The lease holds a bounded child effect context (`HostAuthority`
  required) and the attachment identity tuple. No constructor takes an
  `ObserveContext`.
- Lease acquisition must succeed for `open`. Otherwise `open` returns
  `TerminalRefusal::Unavailable`. A failed or unknown acquisition never authorizes
  an effect, release, forced unlock or settlement.
- Enqueue is fenced (§5 protocol steps 1-4, then append under M). Enqueue waits
  for no invalidator, since an invalidator cannot hold F-ex while I-sh is held.
  An enqueue that finds the row gone or the owner not live is refused, so no
  byte is accepted after release commits.
- Panic in the writer or the pump: unwinding drops the lease and the fds, and the
  kernel releases the locks. The outbox mutex is poison-tolerant (existing).

## 7. Writer contract and nonblocking I/O

- The PTY fd is set `O_NONBLOCK` by the production writer (#519). `write` and
  `resize` map `EAGAIN` / `EWOULDBLOCK` to `WouldBlock`. Any other error ends the
  writer.
- The `TerminalWriter` contract: `write` and `resize` never block. A writer that
  cannot honour this is not admitted to the pump.
- **Stalled-child test (T-STALLED).** A real child (or a pipe with a full buffer)
  that does not read. The writer returns `WouldBlock` promptly. The pump holds
  F-sh only for the bounded effect. An invalidator's `FENCE_WAIT` expires with
  `Unknown`, and nothing commits.
- The fake writer in tests asserts the contract. A writer that would block returns
  an error, not a wait.
- This design does not prove the production PTY binding. That is #519.

**Fork and exec.** `O_CLOEXEC` closes the fence fds at `exec`. In the window
between `fork` and `exec`, the child holds a copy of the fd, and so the lock. A
child that never execs keeps the lock until it exits. Spawn goes through the
existing `Command` path, which execs immediately. Test T-CLOEXEC covers the exec
path. The pre-exec hang is handled in §10.

**Platform semantics.** `flock(2)` locks belong to the open file description on
both macOS and Linux. Both support `LOCK_NB`. Neither gives fairness and neither
converts atomically, so the protocol never converts. `fcntl` record locks are per
process and are not used.

## 8. Liveness, closure and invalidation

**Liveness (effect and enqueue, under F-sh).**

- `attached == false` (exact row, released or absent): `Gone`. The pump retires.
- `require_live_owner` `Ok` (matching `Gate::probe` `Held` plus live process):
  `Live`. The effect may proceed.
- Any other `require_live_owner` result, including `Free`, a non-matching holder,
  or an observation error: `Unknown`. The effect holds without writing.
- A journal `SQLITE_BUSY` after `READ_BUSY_WAIT`: `Unknown`, hold.

**Explicit release** (`Attachment::release`). The `mutations_only` check runs
first. Then I-ex (bounded), then F-ex (bounded). Then the M discard count (§9),
then L2 `release_row`, then release F, then I. A missing or already released row
returns the existing refusal or idempotent `Ok`, and still holds F while it checks,
so no effect is in flight. An ambiguous commit (SQLite returns an error after the
commit may have landed) returns `Unknown`. The caller re-reads. Nothing is treated
as released, and no effect is authorized.

**Secondary release.** Only the secondary's own F is taken. The owner's F and row
are untouched. A secondary release never stops the run.

**Liveness-watch release** (`supervise.rs` → `release_workspace_attachment`). Runs
under L0. Takes I-ex, F-ex, the discard count, L2 `release_row`, then releases.
A timeout returns an error, the supervisor keeps the watch, and it retries.

**Run closure and settlement** (`settle.rs` ~357 → `workspace::closed`).

1. Under L0, take I-ex (bounded). No creator can hold I-sh now, so the set cannot
   grow while I-ex is held.
2. Read S1 = the rows for the run: `(attachment id, lifetime, session,
   generation)`.
3. Take F-ex for every member of S1, in attachment-id order, each bounded.
4. Take the discard counts (§9) for every member, under M.
5. Open L2. Read S2. If S2 ≠ S1, the state is inconsistent. Roll back and return
   `Unknown` (incomplete). Do not retry inside L2, and never acquire a lock inside
   L2. The run stays open for the next settlement.
6. Commit the existing close transaction (`phase closed`, `DELETE`, outcome).
7. Unlink each F for S1 (I-ex and F-ex still held). A failed unlink is an orphan
   and does not change the commit.
8. Release all F-ex, then I-ex.

The close transaction at `settle.rs` ~357 is the transaction that closes the run.
The locks are taken immediately before it and released after the commit, with no
early return in between. Its existing `workspace_session_custody` deletion is in
the same commit.

**Owner already closing.** A second settlement waits on L0. A release waiting on
I-ex sees the row gone after the closure commits, and returns idempotent `Ok`. No
duplicate stop.

**Interrupted / dead-owner settlement** (`settle_interrupted_wait`). Same as
closure, under the recovered gate. The dead owner's process is gone, so its F has
no holder. The discard count is `Unknown(OwnerProcessDead)` unless the owner is
alive. Settlement never fabricates a closed state.

**Process death.**

| Death point | Kernel releases | Journal | Result |
|---|---|---|---|
| Effect holding F-sh, before write | F-sh | unchanged | owner live; next effect re-reads |
| Effect after write, before reconcile | F-sh | unchanged | reserved bytes `Unknown`; no exactly-once claim |
| Invalidator holding I-ex/F-ex, before L2 | both | unchanged | row still attached; retry |
| Invalidator after L2 commit, before unlink | both | committed | transition committed; orphan F, benign |
| Closure during L2 | both | rolled back by SQLite | no change; retry |

Kernel release of a lock never grants authority. Every effect re-reads the row
under F-sh, and every invalidator commits through L2.

## 9. Discard accounting

The queued bytes live in the pump's outbox. A fence in another process can stop
new effects, but it cannot read or discard them.

- **Same process (exact).** The terminal registry maps `(run, attachment)` to a
  weak reference to the terminal's shared state. An invalidator in the owner's
  process takes M under F-ex and records:
  - `input_bytes = pending.len()` and `geometry_pending = latest != applied`, exact,
    when no reservation is set;
  - if a reservation is set (only after a panic, since F-ex excludes a live
    effect), the reserved bytes are `Unknown` and the rest is exact.
- **Other live process.** `Unknown(OwnerInAnotherProcess)`. The transition still
  commits. The owner's own retire discards the bytes.
- **Owner process dead.** `Unknown(OwnerProcessDead)`, from failed lock or kernel
  evidence, never from a guess.

The count is returned as a typed `Discard` value. It is not persisted, which avoids
a journal change. No bytes appear in output, logs or the journal. Debug output
shows counts only.

**Cross-process discard count is `Unknown` (ruled).** Options (a) IPC and (b) a
durable pending record are not pursued without a separate ruling.

## 10. Failure, crash and limitation details

- **Fork before exec hangs.** A child stuck between `fork` and `exec` keeps a copy
  of the fd, so the lock persists. Invalidators hit `FENCE_WAIT` and return
  `Unknown(LockHeld)`. Nothing is released automatically, and no forced unlock
  exists. Recovery is an operator action on the named process, with exact-PID
  controls only. Holders never fork outside the exec path.
- **Crash after a PTY write, before reconcile.** The reserved bytes are `Unknown`.
  Nothing in this design claims exactly-once delivery or an exact count for them.
- **Orphan F after crash or failed insert.** Benign. The id is never reused, and
  closure does not require the file's absence. Orphans are reclaimed by no path in
  this revision. Sweeping them would need its own ruling.
- **Stale identity.** An effect whose F is missing refuses. An effect whose F's
  identity changed (`FenceReplaced`) refuses after three bounded attempts.
- **Accidental replacement during an active holder.** The post-lock check narrows
  the window, but a rename of F while a holder keeps its fd is undetected until the
  next acquisition. Residual, stated in §11.
- **WAL contention.** A WAL reader does not normally wait on a writer. The bound is
  the 250 ms busy timeout. T-WAL-BUSY must observe the `SQLITE_BUSY` error on the
  effect's read. If no mechanism can induce BUSY in the test database, the claim
  is limited to the timeout path, and the test says so.

## 11. Residual and out of scope

1. **Same-user hostile replacement is out of scope (Supervisor ruling).** A
   same-user process that renames or replaces files under the state root can
   replace F or the intent inode. Detection covers accidental replacement at
   acquisition time and at the post-lock check. It is not a security boundary.
2. **Accidental replacement during an active holder.** Detected at the next
   acquisition, not while the holder keeps its old fd. Stated, not hidden.
3. **Cross-process discard count is `Unknown`** (ruled).
4. **Writer contract** is assumed of #519. This design does not prove the
   production binding.
5. **Fork-before-exec hang** (§10). A lock can outlive its owner until exec or exit.
6. **Discard counts are not persisted.** A restart cannot recover them.
7. **Orphan fence files** are not swept by this revision.

No claim of live PTY, TLS, native Linux or reconnect proof is made from fake tests.

## 12. Test plan (targeted, named)

No validation runs until implementation. Each invalidator path has its own test.
No single path stands in for another.

- **T-REPRO (pre-fix, failing-first).** Reproduce the input and resize races on
  base `6b72af5f6` with a deterministic barrier (pump paused after the owner read,
  EOF committed, pump resumed). Record the nextest IDs.
- **T-ORDER-A.** Release or close commits before the pump's F-sh. Zero old-owner
  effects; typed refusal.
- **T-ORDER-B.** Pump holds F-sh for one effect while release waits. The effect
  completes before the invalidating commit. No later effect. Typed result.
- **T-RECONCILE.** An invalidator tries F-ex while an effect is between writer call
  and reconcile. It blocks until reconcile. Count is exact. Covers partial write,
  `WouldBlock`, `Ok(0)`, and resize.
- **T-ENQUEUE.** Enqueue races release. Enqueue after release commits is refused.
  Enqueue during an effect appends behind the reserved prefix. Bytes untouched.
- **T-RESIZE.** Newer resize enqueued during a snapshotted old resize: next tick
  applies the newer size, never the old one after it. Newer enqueued after reconcile:
  applied on the next tick.
- **T-CONCURRENT-CREATE.** Creator waits on I-sh while closure holds I-ex. Creator
  after close is refused. Creator before close is in S1.
- **T-OWNER-GATE.** Matching `Held` is live and effects proceed. Free or non-matching
  holds without writing.
- **T-WAL-BUSY.** Asserts `SQLITE_BUSY` on the effect read, and that the effect holds
  within 250 ms. Invalidator times out at `FENCE_WAIT`.
- **T-PATHS.** Explicit EOF, acknowledged detach, liveness loss, normal closure,
  interrupted/dead-owner recovery, retry after ambiguous commit. Input and resize
  each. A foreign terminal's bytes and geometry untouched.
- **T-ISOLATION.** Owner vs secondary. Wrong root, session, run or generation.
- **T-LOCKFILE.** Replaced inode, hard link (`nlink`), symlink (`O_NOFOLLOW`), wrong
  owner, wrong mode, missing file, non-local filesystem. Each refuses without effect.
- **T-PARENT.** Intermediate replacement: rename the run directory and substitute a
  symlink or a different directory. Root substitution. Each refuses.
- **T-REOPEN.** Replacement between open and `flock`, and between `flock` and the
  post-lock check. Bounded reopen, then `FenceReplaced`.
- **T-CLOEXEC.** Spawn a child while F-ex is held. Drop the holder. A probe acquires
  F-ex nonblocking.
- **T-STALLED.** Stalled child; writer returns `WouldBlock`; invalidator times out
  with `Unknown`; nothing commits.
- **T-WRITER-PREF.** While I-ex is held, a new effect fails at I-sh. Existing F-sh
  holder completes first.
- **T-SEPARATE-PROCESS.** A real child holds F-sh; the parent invalidates. SIGKILL
  the holder. The lock is released, no stale authority is granted, and the row is
  unchanged. Repeat with F-ex. Clean up by recorded PID.
- **T-DISCARD.** Queued input plus coalesced geometry. Same-process count exact.
  Cross-process count `Unknown`. No payload bytes in output, logs or journal.
- **T-PANIC.** Writer panic under F-sh. Lock released by unwind. Reserved bytes
  `Unknown`.

## 13. File ownership and sequence

**#525 owns (implementation phase):**

| File | Change |
|---|---|
| `src/runner/workspace_client/terminal.rs` | pump restructure, reservation, fence lease, enqueue fence, manual `Debug` for `Outbox` |
| `src/runner/workspace_client/tests.rs` | FakePty barriers, `T-*` pump tests |
| `crates/effigy-kernel/src/run/workspace.rs` | release, liveness release, creation, owner admission (~449) |
| `crates/effigy-kernel/src/run/workspace/tests.rs` | `T-PATHS`, `T-ISOLATION`, `T-LOCKFILE`, `T-PARENT`, `T-REOPEN` |
| `crates/effigy-kernel/src/run/workspace_fence.rs` (new) | intent inode, fence inodes, traversal, `fstatfs` checks |
| `crates/effigy-kernel/tests/workspace_fence_holder.rs` (new) | separate-process holder for `T-SEPARATE-PROCESS`, `T-CLOEXEC`, `T-STALLED` |
| `docs/knowledge/contracts/052-runtime-kernel-contract.md` | §8.11 guarantee, lock ownership, limitations |
| `CHANGELOG.md` | `[Unreleased]` entry |

**Shared or flagged (confirm before editing):**

| File | Why | Rule |
|---|---|---|
| `crates/effigy-kernel/src/run/settle.rs` | #525 takes the close locks around the close transaction (~357) and `settle_interrupted_wait`. Custody settlement (~178–243) uses `custody::settle_effect`. | #525 touches only the close and interrupted-settle regions. No shared hunk with `settle_effect`. |
| `crates/effigy-kernel/src/supervise.rs` | #525 edits the liveness-watch release call site (~1460). A's spawn and handoff callers sit at ~8, ~226, ~1035+. | Shared. #525 edits only the liveness call site. Rebase in sequence if both touch the file. |
| `crates/effigy-kernel/src/run/mod.rs` | One-line module declaration for `workspace_fence`. | Shared only if A adds a module. |
| `crates/effigy-kernel/src/faults/scenarios.rs` | Fault points, if a protocol step needs one. | Not in A's list. Avoid unless required. |

**Not touched by #525:** `custody.rs`, `custody/` (all files), `custody/tests.rs`,
`crates/effigy-containers/**`, the lifecycle caller,
`crates/effigy-kernel/tests/support/custody-fixture.rs`, `host_dispatch.rs`,
`workspace_channel.rs`. PR #659 touches none of the custody or containers paths.

**Open items:** the exact file list for A's process-output tests is not in this
doc. Confirm it does not overlap `workspace_client/tests.rs` or
`run/workspace/tests.rs`. The lifecycle caller is not identified.

## 14. Integration and sequence

- Implementation base: the **independently reviewed integrated #512 head** (the
  "512-int" line), with both-parent review of the integration merge. #525 is
  integrated additively on that head, not on an arbitrary PR #659 head.
- The sole integrator coordinates. No source merge while #659 holds are active.
- Overlapping files (`run/workspace.rs`, `run/settle.rs`, `supervise.rs`) are edited
  only after that head is fixed. Each rebase needs the same independent review.
- #519 owns the production PTY binding and the nonblocking writer. This design
  assumes that contract and does not prove it.

## 15. Contract and changelog (at implementation)

- Update `docs/knowledge/contracts/052-runtime-kernel-contract.md` §8.11 with the
  fence guarantee, lock ownership and order, and the limitations in §11.
- `CHANGELOG.md` `[Unreleased]`: one entry, after review.
