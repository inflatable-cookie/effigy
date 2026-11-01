# 037 - Workspace Terminal Attachment Release Authority (design v4, draft)

Status: **docs-only design, pre-implementation, for independent review.** Owner
task: effigy#525. Design base: `6b72af5f6`. Implementation base: the independently
reviewed integrated #512 head (`512-int`, both-parent review), not an arbitrary
PR #659 head. No source change is authorized until this design passes review.

Scope: every transition that ends an admitted workspace terminal's ability to
deliver input or geometry: explicit release, EOF, acknowledged detach, transport
loss, liveness-watch loss, run closure (normal, stop/down, escalation), interrupted
or dead-owner settlement, and same-handle retry. Input and resize both.

## 0. What changed from v3, and why

Supervisor ruling (2026-11-01): replace the flock fence with **single release
authority**. v3's per-attachment and intent flock protocol is retired as the
primary mechanism. The reasons, verified at base:

- The PTY master is created and kept by the custody owner (`custody/spawn.rs`
  ~326-341: `pty()` returns `(master, slave)`, the parent keeps `out_read = master`).
  The custody owner is the per-run supervisor (`supervise.rs`, owner loop
  ~1259-1310). The pump in `terminal.rs` lives in the workspace client serving
  process. So the write path and the release path already sit in different
  processes. That is the root of the cross-process problem.
- If exactly one process can write to the PTY and commit releases, no cross-process
  effect fence is needed. Serialization becomes in-process: a stop flag and a
  bounded wait under the outbox mutex, with no lock file, no inode checks, no
  global intent lock and no lock order across processes.
- Cross-process callers already have a durable request channel: the `requests`
  table (`store.rs` ~1894), consumed by the owner loop (`supervise.rs` ~1295-1310,
  `pending_requests` in `run/mod.rs` ~1454). So a cross-process release can be a
  request, not a lock.

Consequences: v3's intent inode, fence files, descriptor identity, fenced enqueue,
and multi-process lock order are removed. New costs: a request action, a
death-proof path, and a mixed-version gate (§7, §8, §12). Those are real costs and
are not claimed to disappear.

## 1. Problem (verified at base)

- The pump (`terminal.rs` `pump` ~442-522) reads the owner row, then calls
  `write_once` or `resize_once`. The read ends before the effect. A release that
  commits in between is not seen by the effect. This is the #485 race.
- The outbox mutex is held across `writer.write` / `writer.resize`
  (`write_once` ~573-596, `resize_once` ~600-616).
- `Outbox` derives `Debug` and holds `pending: VecDeque<u8>`. A `{:?}` prints input
  bytes.
- Production has no `WorkspaceTerminal::open` caller: `terminal.rs:712` is in tests
  only. Production binding is #519. The race is real in the design, and the FakePty
  reproduction is the failing-first proof. Production reachability is not claimed.

## 2. Terms

- **Owner incarnation**: the exact per-run owner process, identified by
  `runs.owner_identity` (`ProcessIdentity`: pid, start, boot), plus the custody
  tree recorded under that run. Identity is compared with
  `ident::process_identity` and `ident::process_state`, never with a flock.
- **Authority**: the owner incarnation's in-memory terminal state (outbox, stop
  flag, writer handle) together with its committed `run_attachments` row. Only the
  owner incarnation may perform an effect or commit a release for a live run.
- **Owner-local path**: the transition runs inside the owner incarnation.
- **Requested path**: a non-owner process writes a durable request. The owner
  performs the transition and reports the outcome.
- **Death-proven path**: a non-owner process performs the transition only after
  proving the owner incarnation and its custody tree are dead (§6).
- **Unknown**: any path that cannot establish one of the above. It returns a named
  Unknown, commits nothing it cannot prove, and never forces a release.

## 3. Release and closure inventory

Every path that ends a terminal's authority, and where it runs at base.

| # | Path (base location) | Calling process | v4 route | Proof or condition |
|---|---|---|---|---|
| 1 | Explicit release / EOF (`Attachment::release` → `release_attachment`, `run/workspace.rs` ~714, ~908) from `workspace_channel.rs` calls at ~2680, ~2748, ~3011 | client-serving host | owner-local if the caller is the owner incarnation; otherwise requested (`ReleaseAttachment`, §5) | owner alive: request; owner dead: §6 |
| 2 | Acknowledged detach (`workspace_channel.rs` ~2412, ~3054 → `Release::Detach`; existing `RequestAction::Detach` ~1302) | client-serving host | requested, reusing the existing `Detach` request kind for the owner's ack | same as #1 |
| 3 | Transport failure / replacement / retry (`workspace_channel.rs` ~2748 and ~3011 call sites) | client-serving host | same as #1 | same as #1 |
| 4 | Liveness-watch loss (`supervise.rs` `Control::control` ~1448-1461 → `Run::release_workspace_attachment`, `run/workspace.rs` ~1440) | owner incarnation | owner-local, no request | none beyond owner identity |
| 5 | Normal child or run closure (`run/settle.rs` ~357 → `workspace::closed`, `run/workspace.rs` ~482) | owner incarnation | owner-local | none beyond owner identity |
| 6 | Stop / escalate / down (`RequestAction::Stop`/`Escalate`, `supervise.rs` ~940, ~944; `kernel_run.rs` ~1669; `gateway/operator.rs` ~843) | any | request (existing path). The owner then settles through #5 | owner alive: request; owner dead: #8 |
| 7 | Settle request (`RequestAction::Settle`, `supervise.rs` ~1307) | any | owner-local when consumed | as #6 |
| 8 | Interrupted / dead-owner settlement (`run/settle.rs` ~569 `settle_interrupted_wait`; callers `run/children.rs` ~120, `supervise.rs` ~747, `faults/driver.rs` ~428) | recovering host | death-proven path only (§6). Otherwise named Unknown | §6 proof |
| 9 | Same-handle recovery (`release_recoverable`) | — | **not present at base** (`rg` finds no definition). Flagged: the brief names it from another source line. Not designed here | — |
| 10 | Owner handoff / new incarnation (`resume_owner`, `run/workspace.rs` ~767; admission ~449) | new incarnation | a new incarnation may not become authority while the predecessor is alive or unproven (§6, §7) | predecessor death proof |
| 11 | Creation / secondary attach (`attach_secondary`, `open`, `workspace.rs` ~779-900) | any host | not a release. Creation is requested or owner-local. Admission is unchanged | — |

Paths #4, #5 and #7 already run in the owner incarnation. Paths #1–3 and #6 cross
processes and need requests. Path #8 is the only death-proven one.

## 4. Owner-local protocol (the only effect and commit site)

Owner-local state per terminal (in the owner incarnation): `pending`, `latest`,
`applied`, `delivered`, `stop`, `retired`, `in_flight: Option<Reservation>`,
`quarantine: Option<Quarantine>`. All of it is under the outbox mutex M, which holds
memory only. The writer call is never under M.

**Effect.** Under M: refuse if `stop` or `retired`; check the owner row (§8 liveness
check, which is a read in the owner's own process and needs no lock); record
`in_flight` (input prefix or resize snapshot); copy the bytes or the size; release M.
Call the writer (nonblocking, §10). Under M: reconcile (drain `written` from the
front of `pending`, or set `applied` to the snapshot); clear `in_flight`; notify.

**Release or closure (owner-local, serialized in the owner):**

1. Under M: set `stop = true`. Enqueue and new effects are refused from now on.
2. Wait, bounded by `OWNER_STOP_WAIT`, until `in_flight` is clear. The only effect
   that can be in flight is one nonblocking writer call (§10). A timeout returns
   Unknown and the release is not committed (§9).
3. Under M: move `pending` and `latest` into `quarantine` (not discarded, not
   delivered). Record `quarantined_input` and `quarantined_geometry` counts.
4. Commit the journal transition (`release_row` or the close transaction) in L2.
5. Resolve the quarantine (§9). Close the writer fd, which retires the child's
   terminal, only after the commit is verified.
6. Reply to the requester (if requested) with the outcome and counts (§5).

Bytes in `quarantine` are never delivered. They are either discarded after a
verified commit or restored in exact order after a verified precommit failure.

**Enqueue (input and resize acceptance from the channel).** Under M: refuse if
`stop` or `retired`; refuse if the owner row is not live (owner-local read); else
append. Because release sets `stop` under M first, no byte is accepted after the
stop point. A byte accepted before the stop point is in `pending` and is counted
exactly.

**Resize semantics (latest-wins, unchanged).** The reservation snapshots `latest`.
Reconcile sets `applied` to the snapshot. A newer `latest` enqueued during the
effect stays and is applied on the next tick. Newer-during-snapshot and
newer-after-reconcile are both tested (§11).

**Drop, retire and stop paths.** Only the owner incarnation's pump or the release
path clears `pending`, `latest` or `quarantine`. `WorkspaceTerminal::drop` sets
`stop` under M and joins the pump with a bounded wait. If the join times out, drop
records Unknown for the pending counts and detaches the handle. It never clears
state directly. Pump retirement on `Gone`, on writer `Closed`, on stop, and on panic
all go through the same owner-local path (§4 steps 3–5).

## 5. Requested protocol (cross-process release)

**Request.** A new `RequestAction::ReleaseAttachment { attachment }` variant,
written through the existing `request_bounded` path (`run/mod.rs` ~1389) with the
run's `expected_generation`. It is stored in `requests.action` (TEXT, serialized
JSON), so **no DDL is needed**. The `Request` JSON carries the attachment id.

**Compatibility gap (verified).** `pending_requests` deserializes every pending
row with serde (`run/mod.rs` ~1454-1460). An old owner that does not know the
variant fails the whole pending read. A new variant must therefore be written only
when the owner incarnation is known to understand it (§7).

**Owner handling.** The owner loop (`supervise.rs` ~1295-1310) handles
`ReleaseAttachment` by running §4 steps 1–6 for that attachment only. It marks the
request resolved in the same commit as the attachment's release, or leaves it
pending on Unknown (retry).

**Requester result.**

- If the requester's channel is connected, the owner's reply carries the outcome
  and the exact counts from §4 step 3.
- If not, the requester re-reads the row state, which is the durable outcome of
  the commit. The counts are not persisted and are reported as **Unknown**. Making
  them durable would need a result column (schema decision, §12).
- The **cross-process discard count stays Unknown** (Supervisor ruling). The owner
  computes exact counts for its own terminal. A non-owner process never does.

**Timeouts.** A requester waits up to `REQUEST_WAIT` for the outcome, then returns
Unknown. The request stays pending. It is not re-sent by the requester. The owner
processes it when it can (§9, live-but-stuck).

## 6. Death-proven path (dead owner only)

A non-owner process may perform a release or settlement without the owner only
when all of these hold, checked in order:

1. **Exact incarnation dead.** `runs.owner_identity` is checked with
   `ident::process_identity(pid).verified()` against the recorded identity. The
   process must be verified absent, or verified to have a different start or boot
   identity (pid reuse). `Active` or `Stopped` with a matching identity means
   alive. Anything else (observation failed, identity unverified) means **Unknown**.
   Death is never inferred from a missing row, a failed flock, or a missing pid
   alone.
2. **Custody tree dead.** Every custody member recorded for the run (the owner and
   the fork chain in `custody/spawn.rs` ~436–470, and any `workspace_session_custody`
   rows, `settle.rs` ~371) is verified dead by identity, or recorded absent. A member
   whose identity cannot be verified means **Unknown**.
3. **No retained master, by construction.** The PTY master is created with
   `CLOEXEC` in the owner, never passed to the workload, and the custody fork path
   closes every older fd before workload exec (`spawn.rs` comment "already closed
   every older fd", ~470). The conclusion is a construction argument and needs
   independent review and a test (T-FD-INHERIT). It is not a runtime proof that no
   process holds the master. If review does not accept the argument, the death-proven
   path is disabled, and every case returns Unknown.

When all three hold, the death-proven path runs §4 steps 2–5 with no live owner:
there are no in-flight effects, and the quarantine is the dead owner's memory, which
is gone. Counts are Unknown (`OwnerProcessDead`). Settlement never fabricates a
closed state.

**Not a proof of no retained fd.** Steps 1–2 prove the processes are dead. Step 3
argues that none of them retained the master. Nothing observes a stray holder
directly. That residual is stated in §13.

## 7. Authority handoff and mixed versions

- **One authority at a time.** A new owner incarnation (recovery after death,
  §6) is authority only after the predecessor is proven dead. A live predecessor
  keeps authority, and the new incarnation's `resume_owner` must refuse.
- **Mixed-version gate.** A pre-v4 host binary calls `Attachment::release` directly
  from `workspace_channel.rs`, bypassing the owner. A v4 owner cannot stop that call
  in-process. Without a gate, a v4 owner and a pre-v4 host can both commit releases
  for the same run. The gate must be durable and checked before a v4 owner starts
  effects and before a pre-v4 host can attach. The mechanism needs a schema field
  (for example, `runs.owner_protocol`) or a workspace channel protocol version
  check. **This is a decision (§12), not solved here.**
- **Pre-existing rows.** Rows and runs created before v4 have no protocol marker.
  They are treated as pre-v4: a pre-v4 owner or host may still release them
  directly. Recovery for these rows goes through §6 only, and only after the gate
  in §12 says they cannot have a live pre-v4 writer.
- **Serde gap.** An old owner that cannot parse `ReleaseAttachment` must be
  unreachable for requests. The same gate covers this.

## 8. Liveness and the observation boundary

- `Observe` contexts stay read-only. The owner reads its own row with its existing
  context. A non-owner reads the row only to decide between §5 and §6.
- Owner-local liveness under the outbox mutex or a single owner thread is:
  attached row exact (`attached_observed`), `require_live_owner` `Ok`. A matching
  `Gate::probe` `Held` is live. Any other result, including a `SQLITE_BUSY` read
  after `READ_BUSY_WAIT`, is Unknown, and the effect holds. `Gone` (row not
  attached) is the only retirement trigger.
- No `FenceLease`, no flock and no descriptor identity exist in v4. The owner's
  authority is its in-memory state plus its committed row.

## 9. Failure, ordering and crash

| Point | Owner-local or requested | Result | Counts |
|---|---|---|---|
| `stop` set, in-flight effect running | either | release waits up to `OWNER_STOP_WAIT` for one nonblocking call | exact after reconcile |
| `OWNER_STOP_WAIT` expires | either | release not committed; Unknown; request stays pending | not reported |
| Precommit failure (error before COMMIT is issued) | either | quarantine restored in exact order; row still attached; `stop` cleared | none discarded |
| Ambiguous commit (COMMIT error or unknown) | either | re-read the row under the same owner. Released or deleted: committed, discard exact. Still attached: restore. Re-read fails: **Unknown, writer retired (fail closed), quarantine kept unresolved, not delivered** | exact only when verified |
| Owner crash after COMMIT, before ack | requested | row state is the outcome; requester re-reads; counts Unknown | Unknown |
| Owner crash after writer call, before reconcile | either | reserved bytes Unknown; no exactly-once claim | Unknown for reserved |
| Owner crash during quarantine, before commit | either | row still attached; next owner (after §6) or same owner resumes; quarantine is gone with the process | Unknown |
| Requester crash during request | requested | request remains; owner processes it; no requester state is needed | — |
| Owner live but stuck (not servicing) | requested | requester times out Unknown; no commit; no forced release; §5 timeout | not reported |
| Owner stopped (SIGSTOP) then resumed | requested | owner processes the pending request once; no duplicate stop; idempotent on a released row | exact |
| Duplicate request | requested | idempotent: second sees released row, `Ok`, no second stop | — |
| Close and release both requested | either | serialized in the owner; second is idempotent | exact for first |

Idempotence: a request or release for a row already released or deleted returns the
existing refusal or `Ok`, never a second stop and never a false Closed.

## 10. Writer contract

- The writer is owned by the owner incarnation. The PTY master fd is `CLOEXEC`.
- `write` and `resize` must return without waiting. `EAGAIN` / `EWOULDBLOCK` and
  `Ok(0)` map to `WouldBlock`.
- **Not proven.** `O_NONBLOCK` on the data fd does not prove that the resize ioctl
  and every `TerminalWriter` path are bounded. The production claim needs #519's
  actual proof: the ioctl path, plus a stalled-child test on the real master.
- The stalled-child test (T-STALLED) checks that a writer returns `WouldBlock`
  promptly and that the next owner-local operation still runs. It checks that the
  owner does not block on the writer. A stalled child with an accepted-but-unread
  buffer does not hold the owner, because `WouldBlock` releases the reservation
  immediately. The invalidator is not blocked by a stalled child in v4, because it
  is the owner itself.

## 11. Tests (deterministic, named)

- **T-REPRO-INPUT, T-REPRO-RESIZE (failing-first, pre-fix).** The two existing
  FakePty controls on `e28838a36` (nextest IDs `5a08e98f-64f4-4422-b8f9-cd4acc508a83`
  and `4bb4f9af-2cf4-43f8-bba3-90406e0cfba3`, both exit 100). Re-run on base
  `6b72af5f6` and on the fixed head, both must be preserved.
- **T-STOP-BEFORE-EFFECT.** Release commits after `stop` and before the next
  reservation: zero old-owner effects, typed refusal.
- **T-STOP-DURING-EFFECT.** Release waits on an in-flight reservation; the effect
  finishes before the commit; no later effect.
- **T-QUARANTINE-PRECOMMIT.** Fault before COMMIT: restore in exact order, the row
  stays attached.
- **T-QUARANTINE-AMBIGUOUS.** Fault after COMMIT before return: re-read shows
  released, discard exact. Fault in re-read: Unknown, writer retired, no delivery.
- **T-RESIZE-NEWER.** Newer resize during a snapshotted old resize, and newer after
  reconcile: newest applied, never older after newer.
- **T-ENQUEUE-RACE.** Enqueue after stop refused. Enqueue before stop counted.
- **T-DROP-PATHS.** Idle drop, drop during in-flight, drop with a stalled writer
  (join times out, Unknown), drop racing release. Each path clears state only in the
  owner.
- **T-REQUEST.** Request written, consumed once, idempotent duplicate, stuck owner
  (SIGSTOP) → requester Unknown, resume (SIGCONT) → processed once.
- **T-DEATH-EXACT.** Owner process killed (SIGKILL) at each death point in §9.
  Death-proven path runs only with identity proof. Counts Unknown.
- **T-PID-REUSE.** Recorded pid reused with a different start identity: dead
  verified, path allowed. Same identity with `Stopped`: alive, path refused.
- **T-IDENTITY-UNKNOWN.** Observation fails: Unknown, no release.
- **T-CUSTODY-TREE.** Dead owner with a live recorded custody member: refuse. Member
  identity unverifiable: refuse.
- **T-FD-INHERIT.** Owner spawns a workload with the master present: the workload
  does not see the master fd (`CLOEXEC`, close-before-exec).
- **T-MIXED-VERSION.** Fixture: pre-v4 row and pre-v4 host. The gate refuses or
  routes them through §6 only. The v4 owner does not write an unknown request
  variant to a pre-v4 owner.
- **T-PARTIAL-FAILURE.** Each invalidator path (#1–#8) has its own test. No path
  covers another.
- All process-level tests use recorded PIDs and clean up by recorded PID. No pattern
  kills.

## 12. Decisions needed (ruling required before code)

1. **Mixed-version gate.** Which mechanism: `runs.owner_protocol` schema field, or a
   workspace channel protocol version check, or both. Schema consequence and
   migration for existing rows.
2. **Requester count.** Keep Unknown for durable-only requests (current proposal),
   or add a result column (schema).
3. **Owner placement.** #519 must place the outbox and pump in the custody owner
   incarnation, the process that holds the master. Otherwise input and release
   cross processes without a single authority. Confirm this, because it moves the
   pump out of the workspace client serving process.
4. **Death-proof construction (§6 step 3).** Accept the construction argument plus
   T-FD-INHERIT, or require another mechanism. If not accepted, the death-proven
   path is disabled and dead-owner cases stay Unknown.
5. **`release_recoverable`.** Not present at base. Confirm which source line the
   brief means before designing it.

## 13. Residual and uncovered

1. **No runtime proof that no process holds the master.** §6 step 3 is a
   construction argument.
2. **Same-user hostile processes** are out of scope (Supervisor ruling).
3. **Pre-v4 rows and hosts** are not covered until the gate in §12 lands.
4. **#519 writer proof** is not supplied by this design.
5. **Live-but-stuck owner** blocks release until it recovers or dies. Unknown is
   the outcome, not a forced release.
6. **Counts are not persisted** (§5), so a restart cannot recover them.
7. **Admission and queue races** in `open` and admission (~449) are not re-audited
   here. The owner handoff in §7 must be checked against the admission
   transaction before code.

## 14. Effort: v3 flock fence against v4 single authority

| Dimension | v3 (flock fence) | v4 (single authority) |
|---|---|---|
| Cross-process locks | per-attachment F, global I, order L0→I→F→L2 | none on the normal path |
| File state | fence and intent inodes, traversal, reopen | none |
| Descriptor and inode identity | required, residual for accidental replacement | not needed |
| Death detection | flock-based, rejected by Supervisor | identity-based (§6) |
| Request channel | not needed | new `ReleaseAttachment` action, owner loop change |
| Death-proof path | not designed | required (§6), depends on custody records |
| Mixed-version | not designed | gate required (§7, §12) |
| Schema | none (v3) | none for the action; gate may need a field |
| Overlap with #512 A | `run/workspace.rs`, `settle.rs`, `supervise.rs` | same, plus `custody/spawn.rs` (fd closure) and `custody/anchor.rs` (records), and `workspace_channel.rs` if requests come from the channel |
| Pump placement | serving process | owner incarnation (§12, decision 3) |
| Uncovered paths | several, flagged in v3 | §3 table; #9 not present at base |

Net: v4 removes the fence machinery, but it moves the pump and adds a request path,
a death proof and a compatibility gate. The total is not clearly smaller, and the
largest new risk is the owner-placement change (decision 3).

## 15. File ownership and base (v4)

**#525 owns:**

| File | Change |
|---|---|
| `src/runner/workspace_client/terminal.rs` | owner-side pump, stop/quarantine/reconcile, drop path, enqueue check |
| `src/runner/workspace_client/tests.rs` | FakePty barriers, T-REPRO, T-STOP, T-QUARANTINE, T-RESIZE, T-ENQUEUE, T-DROP |
| `crates/effigy-kernel/src/run/workspace.rs` | owner-local release path; request routing in `Attachment::release`; §6 precondition |
| `crates/effigy-kernel/src/run/workspace/tests.rs` | T-REQUEST, T-DEATH-EXACT (non-process parts), T-PID-REUSE, T-IDENTITY-UNKNOWN |
| `crates/effigy-kernel/src/run/mod.rs` | `RequestAction::ReleaseAttachment`, serde compatibility test |
| `crates/effigy-kernel/src/run/settle.rs` | close transaction call site only, owner-local ordering |
| `crates/effigy-kernel/tests/` new fixture | process-level T-DEATH-EXACT, T-STUCK, T-FD-INHERIT, T-MIXED-VERSION |
| `docs/knowledge/contracts/052-runtime-kernel-contract.md` | §8.11 authority model |
| `CHANGELOG.md` | `[Unreleased]` entry |

**Shared, coordinate before editing:**

| File | Why | Rule |
|---|---|---|
| `crates/effigy-kernel/src/supervise.rs` | Owner loop consumes `ReleaseAttachment`; liveness-watch call site (~1460) | Shared with A's spawn and handoff callers. Serialize. |
| `crates/effigy-kernel/src/custody/spawn.rs` | Owner fd closure and master `CLOEXEC`, the basis of §6 step 3 | **A's file.** #525 needs no edit, only an agreed assertion or test. Any change is A's, reviewed. |
| `crates/effigy-kernel/src/custody/anchor.rs` | Custody member records used by §6 step 2 | Read-only for #525 unless A agrees. |
| `crates/effigy-kernel/src/workspace_channel.rs` | Calls to `Attachment::release` (~2412, ~2680, ~2748, ~3011, ~3054). Requests may be sent from here. | **#512-owned.** No edit unless #512 agrees. Routing lives in `Attachment::release`. |
| `crates/effigy-kernel/src/run/children.rs` | `settle_interrupted_wait` caller (~120) | Death-proof precondition. Coordinate with the owner of `children.rs`. |
| `src/runner/workspace_client/` other files, `custody/tests.rs`, containers | not in the path | Not touched. |

**Base and sequence.** Implementation starts on the independently reviewed integrated
`512-int` head, after A's custody spawn change lands or is reviewed (its fd closure
is §6 step 3). #525 rebases onto that head, with both-parent review of the merge.
The sole integrator coordinates, and the #512 merge hold stays in place.

## 16. Contract and changelog (at implementation)

- `docs/knowledge/contracts/052-runtime-kernel-contract.md` §8.11: single release
  authority, the request action, the death-proven path, the mixed-version gate, and
  the limitations in §13.
- `CHANGELOG.md` `[Unreleased]`: one entry after review.
