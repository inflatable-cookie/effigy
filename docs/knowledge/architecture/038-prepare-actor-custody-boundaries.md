# Prepare and direct-parent custody scope for Oxide #309

Status: planned. Source-bound design scope only; no implementation, activation,
build, probe or native work is authorized by this document. It records what
Effigy owns, what it deliberately does not, and the exact interfaces a bounded
implementation tranche would have to supply. Envelope, holds and evidence stay
as Oxide stated them.

Revision: effigy#1023 corrects this document so it composes with Oxide's merged
design half (Oxide #313, `b069e923`). It changes the publisher name, the clock
payload and origin rules, the wait and owner-loss graph, the receipt shape, and
the first-effect claim. Where this revision and the earlier text disagree, this
revision governs.

Owner: Effigy runtime kernel and runner maintainers for the interfaces and
lifecycle; Oxide's owner for the consumer graph, oracle set and accounting
decision.

## Why this exists

Oxide's Spec064 names nine interfaces it needs from Effigy, all currently
missing: `effigy_prepare_custody`, `effigy_admission`, `common_elapsed_clock`,
`creator_waits`, `deciding_census`, `final_census`,
`image_attachment_evidence`, `capture_delivery` and
`accounting_fit_readiness`. This document maps the Effigy-owned half of that
list to current source, names the direct-child handles and positive wait or reap
obligations that really exist, and states the remaining target of each proposed
interface. The verdict is unchanged from the effigy#1017 report and from
[contract 049](../contracts/049-heavy-validation-admission-contract.md): Effigy
does not today supply the conjunction of an all-owned physical prepare peak, an
enforced total time bound with positive reap, and requested actor custody.

Three things this document is not:

- it is not a change to the 600 first-effect, 512 MiB monitored (384+128), 22 GiB
  or 445-pin envelope, and it introduces no interpretation of RAW;
- it is not maintenance treated as admission, and it is not a claim that a
  report transfers custody;
- it is not permission. Each scope below ends with the interfaces it refuses.

## Evidence base

Claims here were first checked at `522c939e` against the pinned head `dc6f5abf`
that the effigy#1017 report used. Two of that report's cited files changed
between the two heads: `src/cli/entrypoint.rs` gained the nested lock-wait
export at `:144-145`, after the deadline configuration at `:23-27`; and
`docs/knowledge/contracts/052-runtime-kernel-contract.md` changed in two hunks,
one line re-expanded to four at `:3730` and 90 new lines at `:4480-4569`. Both
052 hunks sit after every 052 line this document cites.

Effigy#1023 re-read the seams it changes at `50a9462d` (the main head that
carries #1022 at `c4d33736`). Those reads are cited inline. Other citations
keep the `522c939e` evidence base. Oxide's design at `b069e923` is not reachable
from this checkout; its interface names below come from the Oxide planner's
provisional proposal as quoted in the effigy#1023 brief, and this document does
not treat them as agreed.

Review history: the first review of this document (5935018539) failed and is
kept as history. Review 5935123400 is the review of the `50a9462d` text.

Verified corrections the independent review recorded earlier still hold:

- the deadline is wired, not dormant: `src/cli/entrypoint.rs:23-27` configures
  it and `src/runner/task_engine/kernel.rs:113-116` reaches `capped()`
  (`crates/effigy-kernel/src/ctx.rs:192-215`);
- the detached supervisor `__kernel-supervise`
  (`crates/effigy-kernel/src/supervise.rs:1213-1253`, `:2133`) is a real direct
  parent of its own workloads;
- the cleanup-pin writer is `MemoryGuard.closure`
  (`crates/effigy-kernel/src/supervise/memory.rs:28`) and
  `ForegroundRun::closure_pin`
  (`crates/effigy-kernel/src/run/foreground.rs:198`).

## Naming

The memory sentinel in this document is **memory sentinel MS**. Earlier text
called it "S". In Oxide's composition S is the Nucleus retained
supervisor/publisher, so the letter is retired here. Nothing below uses "S" for
the sentinel.

## Scope 1: opt-in prepare custody and the direct-parent graph

### What the prepare route does today

`effigy run prepare` is dispatched at `src/runner/run_control.rs:77-79` into
`prepare` (`:95`). In order, that function: opens the checkout effects root,
which can create `.effigy/state` (`:101`; `crates/effigy-kernel/src/root.rs:65`);
optionally resolves a declared foreground selector into gate identities
(`:206-263`); opens the store in preparing mode, which ignores the strict
switch by construction (`crates/effigy-kernel/src/store.rs:499-506`); prepares or
verifies the fixed terminal ring (`src/runner/run_control.rs:122-126`;
`crates/effigy-kernel/src/run/terminal_ring.rs:44,60,164`); prepares or verifies
the task gates (`:128-135`; `crates/effigy-kernel/src/run/mod.rs:57-72`;
`crates/effigy-kernel/src/gate.rs:410,475`).

The clock is present but cooperative: `effect_context(OpenReason::Shell)` at
`:105` is capped by the invocation ceiling
(`crates/effigy-kernel/src/ctx.rs:192-195`), and store patience is capped too
(`:251-261`). `PREPARE_WAIT` is a 30-second contention window
(`src/runner/run_control.rs:36`), not a total execution deadline.

The decisive fact for this scope: **prepare creates no child process at all.**
SQLite, the ring write and the gate preparation all run in the invoking process.
There is no receipt, no attempt identity, no independent holder of the effect
gates and no reaper. Every allocation those calls make survives a failure, and
nothing outlives the process if it is killed mid-write.

### Proposal: `effigy_prepare_custody`

One explicit, opt-in maintenance attempt with a named owner, a bounded worker
and memory sentinel MS as the only independent holder. Opt-in is a flag on
`run prepare` plus a required caller-supplied attempt label, not an environment
variable: environment variables are inherited by nested invocations
(`src/cli/entrypoint.rs:144-145` shows that inheritance is deliberate for the
lock wait), and a silently inherited custody mode would change nested runs.

Shape:

1. **Refusal, and what it does and does not claim.** `run prepare --custody
   --attempt <label>` refuses at `src/runner/run_control.rs:95` when the label is
   absent, already holds a live attempt, carries no inherited ceiling (Scope 2),
   or MS cannot start. Every refusal precedes the effects-root open at `:101`,
   which is the first prepare-root write (`root.rs:65` creates `.effigy/state`).
   So a refusal makes **no prepare-root writes**. That is the whole claim;
   source does not support "allocates nothing". Process spawn for MS, gate
   descriptor duplication (`crates/effigy-kernel/src/supervise/memory.rs:170-215`)
   and any attempt record are effects. They are charged from the original origin
   (Scope 2), and a sentinel-unavailable refusal cannot retroactively declare
   them absent.
2. **Owner E** is the invoking `effigy` process.
3. **Worker W** is one `Command`-spawned helper in the same style as MS
   (`crates/effigy-kernel/src/supervise/memory.rs:239-252`), and performs the
   whole first-effect sequence of the step list above. E holds a
   `std::process::Child` for W. Moving the effects into a child is what makes a
   total bound possible at all: E's clock loop is no longer blocked by W's
   `fsync`, SQLite busy wait or rename.
4. **Memory sentinel MS** is the existing memory sentinel, started before W,
   holding the effect and task gates plus one absolute deadline instant. It is
   the same helper, not a new controller: it clears its environment, runs
   `setsid` (`crates/effigy-kernel/src/supervise.rs:435-465`), reads
   `deadline_nanos` from its config (`crates/effigy-kernel/src/supervise/memory.rs:93,140`)
   and evaluates it locally (`:145-159`). MS holds no handle to W. W is E's
   child, not MS's.
5. **Disposition and loss.** The rules below are the whole of it.
   - **E→W wait receipt first.** E writes an attempt disposition only after it
     holds a positive wait receipt for W: the `ExitStatus` returned by
     `Child::wait()`, or a `waitid` peek followed by reap, recorded with W's
     `ProcessIdentity`. A kill that returns no status is not a receipt. If E's
     bounded wait on W expires, or E leaves before reaping, W's state is
     `unknown`.
   - **E's wait on MS is not positive.** E's `Drop` for `MemoryGuard` polls
     `try_wait` on MS only up to a bound, and returns without reaping when the
     bound expires or the guard was abandoned
     (`crates/effigy-kernel/src/supervise/memory.rs:540,582,595-620`). When that
     happens E records `unknown` for MS and does not claim MS was reaped.
   - **E loss.** If E dies first, MS takes settlement over as it already does for
     a foreground run. `wait_for_closure` ends on a verifiably gone owner
     (`owner_is_gone`, `supervise/memory.rs:1116`; `wait_for_closure` at
     `:1064-1108`), `record_owner_loss` writes `interrupted`/`unknown`
     (`:1140-1181`), and `drop(gates)` at `:1043` runs only when the cleanup pin
     is closed (`ClosureWait.closed`). That closure is MS's settlement of its own
     guard. It is **not** E's direct wait or reap of W or A, and it is **not**
     proof that W or A completed. MS holds no handle to either, so after E loss
     their state is `unknown` unless the pin closure shows them gone, and the pin
     is not a census.
   - **Retained states stay retained.** An unproved closure parks MS holding the
     gates and records nothing as complete (`:1039-1041`). `unknown`, retained
     and any Oxide-defined terminal state (SPENT is Oxide's term; Effigy source
     has no SPENT state and this document gives it no meaning) are preserved even
     if exclusion settles later. Settlement does not transfer custody, and it
     does not create a new custodian.

### The real creator and wait graph, named

`E` = invoking effigy owner, `W` = prepare worker, `MS` = memory sentinel,
`H` = admission hook, `A` = selector workload leader, `K` = anchor.

| Creator | Child it holds | Positive obligation | Source |
| --- | --- | --- | --- |
| E | W (prepare worker) | deadline-bounded TERM/CONT→KILL, then `wait()` reap, with the `ExitStatus` as the receipt, before any disposition | proposal; the sibling pattern is `supervise/memory.rs:239-252` |
| E | MS (memory sentinel) | bounded `try_wait` in `Drop`; not positive: expiry or abandonment records `unknown` | `supervise/memory.rs:540,582,595-620` |
| E | H (admission hook, heavy runs only) | run settle stops it through its `OwnedProcess` | `src/runner/task_engine/admission.rs:299` (`hook`), `:744-771`, `:295-300` |
| E | A (selector leader) | peek with `waitid(WNOWAIT)`, reap with `waitpid` only after group closure, capture drain and anchor retirement | `crates/effigy-kernel/src/custody/spawn.rs:400-414,741-757,770`; `crates/effigy-kernel/src/custody.rs:1724-1822` |
| A | middle, then K (anchor) | waits the middle before workload exec; K deliberately leaves A's child set | `custody/spawn.rs:467-480,525-532` |
| MS | its own workloads, in the managed route only | `Custody::spawn_all` under `__kernel-supervise` | `supervise.rs:2133` |

MS is A's **sibling**, not its parent: Effigy hands MS a receipt, its own gate
descriptors and the closure pin, never a child handle. Nothing in the foreground
or prepare route gives MS a handle to E's children, and no receipt, EOF or census
converts one into the other.

Every creator's wait is a separate record (Scope 3). A terminal report or EOF
from a different process never satisfies a creator's own wait.

### Reuse and removable duplication

Reused unchanged: the sentinel helper, its own deadline evaluation, its
owner-loss record and its gate hold; `Child`/`OwnedProcess` handle ownership;
`Custody` stop escalation and reap ordering; the gate and ring publishers.

Removable duplication once the attempt exists:

- the parallel `--check` and write branches over store, ring and gates
  (`src/runner/run_control.rs:104-138`) collapse into one mode on a single
  attempt entry, because both are now steps of one recorded attempt;
- `prepare_foreground_gates` and `verify_prepared_foreground_gates`
  (`crates/effigy-kernel/src/run/mod.rs:57-72`) are load-bearing and stay: they
  first re-spell the ids through `aliases::gates`
  (`crates/effigy-kernel/src/run/aliases.rs:7-35`) so a path-derived identity and
  a journal-recorded token take the same gate, then call
  `Gate::prepare_foreground` (`crates/effigy-kernel/src/gate.rs:410`) or its
  verifier (`:475`). Calling the gate layer directly would skip the checkout
  re-spelling. What the attempt removes is the repeated re-spelling: one
  resolution recorded on the attempt, reused by the later selector's admission,
  which takes the same spelling;
- `PREPARE_WAIT` (`src/runner/run_control.rs:36`) and the store's own busy wait
  become two applications of one attempt deadline instead of two independent
  bounds.

### Focused controls and budgets

- `crates/effigy-kernel/tests/prepare_custody.rs` (new): attempt identity and
  the refusal table — missing label, duplicate live attempt, missing inherited
  ceiling, sentinel unavailable; each refuses before the first prepare-root
  write.
- `src/runner/run_control/tests.rs`: the production route — E starts MS, then W;
  W is reaped before the disposition is written; a W that blocks in a syscall is
  killed at the attempt deadline and reaped; killing E leaves MS holding the
  gates and writing `unknown`.
- Selectors: `effigy test:focused -- prepare_custody` and
  `effigy test:focused -- run_control::prepare`, plus `effigy qa:docs` for this
  document. Budget: per-test wall clock ≤ 60 s, no fixture longer than the
  workspace test-runner timeout, no native, image, VM or guest probe.

### Refused here

Claimed handle transfer from W or MS to the consumer or to the later selector; a
subreaper or a new controller; a hard blocked-I/O completion bound; any physical
allocation guarantee; maintenance prepare treated as admission; a prepare
receipt treated as a resource ledger; a "refusal allocates nothing" claim beyond
the no-prepare-root-write claim above.

## Scope 2: the common elapsed clock

### What exists

One opt-in absolute deadline per invocation (`EFFIGY_RUN_DEADLINE_MS`,
`crates/effigy-core/src/run_deadline.rs:29`), configured once at CLI start
(`src/cli/entrypoint.rs:23-27`). Its origin is sampled once at process start,
before any capture, staging or admission, on the monotonic clock, and is never
restarted (`run_deadline.rs:1-8`). The ceiling is stored as absolute
`CLOCK_MONOTONIC` nanoseconds (`:80-89,110`), a settlement allowance of 10 s
is reserved inside it (`:34`), and the evidence reserves are 1 s each (`:39,43`).
`parse` (`:398-406`) and `configure_env_once` (`:427-438`) build the deadline
from the process-start sample.

The environment variable is a **duration from this process's own start**. It
carries no absolute reference, so no predecessor can pass an earlier origin
through it.

### Supported host-clock payload (today)

This is what source supports for a value that crosses a process boundary:

- **One field:** `deadline_nanos: u64`, an absolute instant on the host-wide
  monotonic clock. Produced as `RunDeadline::ceiling_nanos()`
  (`crates/effigy-core/src/run_deadline.rs:110`) at
  `crates/effigy-kernel/src/run/foreground.rs:167-169` and
  `crates/effigy-kernel/src/run/checkout_custody.rs:63`. Carried as
  `MemoryStart.deadline_nanos` (`crates/effigy-kernel/src/supervise/memory.rs:140`)
  and `Config.deadline_nanos` (`:93`). Evaluated by `deadline_left` and
  `deadline_passed` (`:145-159`) against `system_monotonic_nanos()`
  (`crates/effigy-core/src/run_deadline.rs:239-246`).
- **Units:** nanoseconds. **Domain:** `libc::CLOCK_MONOTONIC` through
  `system_monotonic_nanos`. This is the same host clock for every same-host
  process, and it does not restart while the host runs.
- **Origin identity:** none. The value carries no origin id, no boot id and no
  sample time. The owner's identity travels separately in `Config.owner`
  (`memory.rs:64`), not in the clock value.
- **Producer caveat.** `from_origin` (`run_deadline.rs:80-89`) computes
  `ceiling_nanos` by sampling `system_monotonic_nanos()` at **its own call**, plus
  `ms`. Production calls it from `configure_env_once` immediately after the
  `Instant::now()` origin sample, so the two anchors agree to the gap between two
  adjacent reads. Test call sites inject an origin instead
  (`src/runner/task_engine/run/foreground.rs:433`,
  `src/runner/task_engine/spawn_env.rs:126,155`,
  `src/runner/task_engine/admission.rs:1477`, all in `#[cfg(test)]` code). For those,
  the nanos ceiling is anchored to the call, not to the injected origin.
- **Diagnostic only.** `PhaseReadback.since_origin_ms`
  (`crates/effigy-core/src/run_deadline.rs:229-231`, `:306`) is an evidence
  readback. It is not a payload and not authority.

Neither the monotonic-domain equivalence stated above nor `from_origin` proves a
cross-process binding. Both sample the clock in the process that holds them.

### Qualification origin and predecessor effects

The qualification origin starts before Nucleus reservation persistence, source
capture and the other predecessor effects. Sentinel startup, worker spawn,
prepare and the later real selector must all fall under **the same original
allowance**. Source does not support that today:

- Effigy's only inherited input is the duration above, measured from its own
  process start. Predecessor effects that run before that start sit outside any
  allowance Effigy can see.
- A consumer that needs one allowance across the sequence needs a same-timebase
  control that Oxide has not resolved. Until one exists, the **consumer binding
  is UNSUPPORTED, refuse**. Effigy does not establish one from the monotonic
  equivalence or from `from_origin`.
- If the finite allowance cannot fit the sequence, that is a **blocker** for the
  consumer to decide. Effigy does not reset, shorten or extend the allowance.

### Attempt rule: narrow only

- An attempt never establishes a full allowance. Its bound is
  `min(inherited ceiling_nanos, attempt_origin + attempt_ceiling)`. The local
  sub-origin can only lower the inherited ceiling. It cannot raise it.
- An attempt with no inherited ceiling (`run_deadline::current()` is `None`, that
  is `EFFIGY_RUN_DEADLINE_MS` unset) refuses. It does not mint a ceiling.
- The attempt's local origin is the first effect the attempt owns: the sample
  taken before MS spawn, before the effects-root open at
  `src/runner/run_control.rs:101`.

### Remaining target

1. **One absolute instant across the boundary.** The attempt publishes the
   absolute instant, never a duration and never an elapsed reading, so no holder
   reconstructs time. One conversion point exists and stays unique: `Instant` in
   the owner, `ceiling_nanos` at the send (`run/foreground.rs:167-169`). No
   conversion service and no new inner clock.
2. **Phase remainders inside the attempt.** Store open, ring publication, gate
   preparation, disposition write and delivery each read their remainder from the
   same clock through the existing phase names (`run_deadline.rs:52-58`). No
   second timer is introduced for any of them.

### Semantics the interface must state

- **Blocked I/O.** The clock is cooperative. `ctx.operation().check(..)` is only
  consulted between operations (`crates/effigy-kernel/src/ctx.rs:292-304`), and
  SQLite busy waits, `fsync` and `rename` are ordinary blocking calls. The
  attempt therefore gets its bound from its shape, not from the clock: the work
  runs in W so that E's loop can escalate and reap. Whether a kill lands inside a
  blocked syscall and whether the wait returns promptly are **not** guaranteed;
  the interface must say so rather than imply a total bound.
- **Owner loss.** MS is the only clock holder that survives E. It evaluates the
  same absolute instant, and on expiry or observed owner loss it finishes
  settlement, records `unknown`/`interrupted` (`supervise/memory.rs:1140-1181`)
  and either closes (`:1043`) or parks holding the gates (`:1039-1041`).
  Deadline expiry never releases retained state: release is an explicit later
  action.
- **One clock, one domain.** Both readings used today — `Instant::now` and
  `system_monotonic_nanos` — are the same host monotonic domain. That is a
  statement about one process's two readings. It is not a cross-process binding.

### Focused controls and budgets

- `crates/effigy-core/src/run_deadline.rs`'s owned injectable clock
  (`:251-254`) drives attempt-origin and phase-remainder tests with no sleeping.
- `src/runner/run_control/tests.rs`: the attempt refuses with no inherited
  ceiling; the attempt bound is the minimum of the two ceilings and never exceeds
  the inherited one; an expired attempt parks retained state instead of releasing
  it; a ceiling that cannot fit the 10 s settlement allowance is refused before
  anything starts.
- Selectors: `effigy test:focused -- run_deadline`, `effigy test:focused --
  prepare`, `effigy qa:docs`. Budget: the owned-clock tests are sub-second; the
  route tests need the workspace runner timeout only for their hostile-fixture
  cases.

### Refused here

A hard blocked-I/O guarantee; a clock supplied to and read by the consumer as
authority over its own elapsed accounting; deriving elapsed time from a duration
or from the journal; a duration, `sinceStartMs` or `elapsedAtSpawnMs` payload as
the binding; a second clock for prepare; any RAW conversion; a conversion
service; a consumer binding without a same-timebase control; a local sub-origin
used as a new full allowance; resetting or reusing an inherited ceiling; using
the deadline as authority to release, evict or reuse a retained attempt.

## Scope 3: exact-selector admission and attributable completion

### What exists

`run prepare --task` resolves a declared foreground selector through the real
resolver — `build_execution_preflight`, then `resolve_route`, then
`TaskTarget::for_selection` and the owner/shared gate identities
(`src/runner/run_control.rs:206-263`) — and prepares its gates without running
it. The real invocation builds its own `ProcessSpec` from the resolved command's
argv, cwd and environment (`src/runner/task_engine/process.rs:137-168`), with
lifetime = the smaller of the local timeout and the remaining workload ceiling
(`:148-155`), and spawns through custody with a `WorkloadGate`
(`src/runner/task_engine/run/foreground.rs:181-227`).

Under admission protocol v2 the workload event carries the leader identity and
pgid and is acknowledged before `go`
(`src/runner/task_engine/admission.rs:442-467`, `before_go` at `:443`;
`crates/effigy-kernel/src/custody.rs:1137-1175`). A refused or blocked pre-exec
leader is killed and waited by its actual parent (`custody/spawn.rs:710-738`).
Completion evidence is the terminal report (`admission.rs:1044-1090`), which
stops at the acknowledged prefix (`:481-500`), plus the narrow Q-010 row bound
to run, checkout and boot but carrying no process identity
(`:1099-1163,1164-1190`). Report failure is a warning and a
`settlement_incomplete` flag, not a retry (`:1074-1090`).

Without a hook, `WorkloadGate::for_spawn` returns `None` when the admission does
not report (`admission.rs:414-422`), so today's non-v2 spawn has no gate and
produces no receipt.

### Proposal: `effigy_admission`

An interface with three fields and no new authority:

- **Input:** the resolved selector identity (rendered selector plus arguments),
  the resolved root, the owner and shared gate identities, and a digest of the
  effective argv, cwd and environment the spawn actually used.
- **Output:** an admission receipt carrying that digest, the owner identities,
  the workload-group count, the acknowledged-prefix length and the terminal
  disposition. `unreported` when no hook is configured — never `admitted`.
- **Rule:** admission exists only as the pre-`go` gate of a real spawn.
  Completion is attributable only when the receipt is bound to that spawn's
  leader identity, the acknowledged prefix is gap-free (`admission.rs:296-320`,
  `seq` at `:309`) and the terminal disposition came from a settled outcome.
  Otherwise the disposition is `unknown`.

`unreported` is design vocabulary. Source emits no such token; the no-hook state
is `reporting() == false` (`admission.rs:385-387`). That state must **not** pass
genuine-admission readiness. A no-hook `unreported` correctly fails that
readiness, and this document keeps that rule.

The prepare attempt of scope 1 never produces an admission receipt. Its
resolution and the later selector's resolution both call
`build_execution_preflight` (`run_control.rs:224`), so the digest is the only
addition needed to show both describe the same selector — the second resolver is
not duplicated.

### Receipt shape: what each Effigy-created edge must carry

For each Effigy-created required edge the receipt names the fields below. Where
source has no field, the row says **MISSING** and names the seam. Nothing here
is claimed that source lacks.

| Field | Source today | Seam |
| --- | --- | --- |
| Parent PID and start token | Present where the parent is verified: `ProcessIdentity { boot, pid, start }` with `start: ProcessStartIdentity` (`crates/effigy-kernel/src/ident.rs:12-16,36-40`). For MS, the owner identity is `Config.owner` (`supervise/memory.rs:64`). | none |
| Child PID and start token | Present where verified: `ident::process_identity(child.id()).verified()` (`supervise/memory.rs:259`), and `ProcessReceipt.identity` (`crates/effigy-kernel/src/custody.rs:188-193`). The field is `None` when verification fails, so that edge reads as unknown. | none |
| Observed or unknown | Partial. `verified()` returns `Option`, so absence is representable. No edge records an explicit observed/unknown field. | `supervise/memory.rs:259,330`; `custody.rs:188` |
| Exit code or signal | Workload: present in the terminal report (`exit_code`, `signal` from `outcome_code` at `admission.rs:1238`; event fields at `:1048-1058`). E→MS: **MISSING**. The bounded `try_wait` in `Drop` discards its status (`supervise/memory.rs:595-620`). E→W: MISSING (proposal). | `supervise/memory.rs:595-620` for E→MS; `admission.rs:1044-1056` for workload |
| Time and source binding | Start time is in `ProcessStartIdentity` (`ident.rs:12-16`). No event or receipt records when it was observed or which reporter wrote it. **MISSING**. | `admission.rs:1048-1058` (terminal event fields) |
| Subject, run and attempt binding | `run_id` lives in `Delivery` (`admission.rs:306`), but the workload event body carries only `pgid` and `leader` (`admission.rs:443-453`). Attempt binding: **MISSING** everywhere. | `admission.rs:443-453`; attempt has no source |
| Resolved-selector digest | **MISSING**. `admission.rs` has no digest field. | `admit` (`admission.rs:671`) request body; `run_control.rs:224` resolver |
| Gap-free pre-`go` admission terminal | Partial. The acknowledged prefix is gap-free (`admission.rs:296-320`). The terminal report is sent at settle (`:1044-1090`) and does not distinguish a pre-`go` refusal. **MISSING** as a named pre-`go` record. | `admission.rs:1044-1058`; `before_go` at `:443` |
| Hook identity | **MISSING** from the terminal report. The hook is `AdmissionCore.hook` (`admission.rs:299`). | `admission.rs:1044-1058` |

Creator-wait records follow the same rule per creator. The publisher's own
creator wait stays a separate required record. A sibling's terminal or EOF
cannot fill it. The E→MS wait stays **not positive** (Scope 1) until its status
is recorded.

### Focused controls and budgets

- `src/runner/task_engine/admission.rs`'s in-file `#[cfg(test)]` module
  (`:1369`) and `src/runner/task_engine/tests.rs`: receipt-bound completion,
  `unreported` without a hook, prefix-gap refusal, and a prepare attempt that
  produces no receipt.
- `src/runner/run_control/tests.rs`: the same selector resolved by prepare and by
  the real run yields the same digest, and the two remain separate attempts.
- Selectors: `effigy test:focused -- admission`, `effigy test:focused --
  run_control`, `effigy qa:docs`. Budget: the hook-driven cases need the existing
  hook fixtures only; no external heavy admission is exercised.

### Refused here

Maintenance treated as admission; a report, receipt or exit code treated as
custody; completion inferred from a group census or a leader identity; a third
party deriving signal or reclaim authority from a receipt; a protocol downgrade
to make completion look attributable; a `MISSING` field described as present.

## Oxide and Nucleus boundary

Census, image attachment, complete RAW and fit stay with their named Oxide and
Nucleus providers. Effigy supplies no census, image or fit verdict in this
document, and this document designs none of those parts. The rows for them in the
table below record Effigy's position only. Oxide and Nucleus own the consumer
graph, the oracle set and the accounting decision.

## The rest of the Spec064 list

| Interface | Effigy's position at `522c939e`, updated at `50a9462d` |
| --- | --- |
| `creator_waits` | Effigy-owned **witness** only: the obligation table above, plus the per-edge receipt fields in Scope 3. No API for a third party to register waits. |
| `deciding_census` | Not supplied. Membership is best-effort observation (`custody/platform.rs:529-555,595-618,629-681`) and contract 052:1282-1287 says a scan can miss a fork/orphan race. A deciding census is a consumer decision over an `Unknown`-capable observation. |
| `final_census` | Not supplied, same source. Group closure accepts accounted identities and zombies rather than an empty inventory (`custody/platform.rs:37-51,108-168,595-618`), so it is not a final census. |
| `image_attachment_evidence` | Not supplied in production. Image identity observations exist only under the `workspace-private-qualification` compile-time route (`custody.rs:1163-1176`; `supervise/image_identity.rs:1-8,12-38`), and the birth-hash scaffolding grants no capability (`supervise/birth_hash.rs:1-9`). |
| `capture_delivery` | Partly supplied: bounded, redacted rotating capture per stream (`custody/capture.rs:1-6`), delivery that stops at the acknowledged prefix (`admission.rs:481-500`) and an output cap (`crates/effigy-core/src/output_cap.rs`). Not supplied: proof that an external sink received everything, or any retry after a failed terminal report. |
| `accounting_fit_readiness` | Not supplied and not claimable. `MAX_DB_BYTES` is a 1 GiB logical bound (`store.rs:33`), the ring is 16,448 fixed bytes (`terminal_ring.rs:60`) and contract 049:517-522 expressly leaves APFS metadata copy-on-write unbounded. Fit is the consumer's accounting decision over witnesses. |

## Implementation seams for one bounded tranche

No implementation is authorized here. If one is briefed, these are the exact
files it would touch, in this order:

| File | Responsibility |
| --- | --- |
| `docs/knowledge/architecture/038-prepare-actor-custody-boundaries.md` | This scope; status moves only when code lands. |
| `crates/effigy-kernel/src/prepare_custody.rs` (new), `lib.rs` | Attempt identity, opt-in gate, refusal table, disposition writer. |
| `crates/effigy-kernel/src/prepare_custody/worker.rs` (new) | W: the helper that performs the first effect, and E's positive wait-receipt obligation. |
| `src/runner/run_control.rs` | Route `run prepare --custody` through the attempt; keep `--check` as a mode of it. |
| `crates/effigy-core/src/run_deadline.rs` | Attempt origin and the narrow-only minimum-of-ceilings rule; one conversion point. |
| `src/runner/task_engine/admission.rs` | The receipt fields in the table above, the selector digest and the `unreported` disposition. |
| `crates/effigy-kernel/tests/prepare_custody.rs` (new), `src/runner/run_control/tests.rs` | The focused controls named above. |
| `changelog.d/<task>-<slug>.<type>.md` | One paragraph describing the actual landed level. |

## Unsupported interfaces, consolidated

These stay unsupported in any reading of this design, and a future acceptance
must refuse rather than exempt:

- claimed handle transfer between owner, worker, sentinel and consumer;
- a subreaper or a new controller;
- maintenance treated as admission;
- assumed cross-clock or RAW conversion;
- a new inner clock, and a clock conversion service;
- **consumer binding of the host clock across processes: UNSUPPORTED, refuse**,
  until a same-timebase control exists;
- predecessor-effect coverage by an inherited allowance: UNSUPPORTED, refuse;
- a duration, elapsed or `sinceStartMs` payload as the binding;
- a local prepare sub-origin used as a new full allowance, or a reset of an
  inherited ceiling;
- group or escapee signals treated as custody;
- any hard blocked-I/O or physical-allocation guarantee;
- a decide/fit verdict issued by Effigy as authority;
- production image-attachment evidence;
- a final census treated as positive reap;
- a field the receipt marks MISSING, reported as present.

## Proof of this document

Source inspection at `522c939e` for the original evidence base, and at
`50a9462d` for the seams this revision changes, with the changed-seam diff
against `c4d33736`. Verification: `effigy qa:docs`, and independent exact-head
review that every file:line claim holds at the PR head. No build, probe,
fixture, native or resource run was part of this design task, and no runtime
behaviour is claimed.
