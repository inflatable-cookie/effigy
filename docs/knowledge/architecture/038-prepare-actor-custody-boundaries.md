# Prepare and direct-parent custody scope for Oxide #309

Status: planned. Source-bound design scope only; no implementation, activation,
build, probe or native work is authorized by this document. It records what
Effigy owns, what it deliberately does not, and the exact interfaces a bounded
implementation tranche would have to supply. Envelope, holds and evidence stay
as Oxide stated them.

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

Claims here were re-checked at `522c939e` against the pinned head
`dc6f5abf` that the effigy#1017 report used. Two of that report's cited files
changed between the two heads: `src/cli/entrypoint.rs` gained the nested
lock-wait export at `:144-145`, after the deadline configuration at `:23-27`;
and `docs/knowledge/contracts/052-runtime-kernel-contract.md` changed in two
hunks, one line re-expanded to four at `:3730` and 90 new lines at `:4480-4569`.
Both 052 hunks sit after every 052 line this document cites, so the cited text
and its line numbers are unchanged. Every custody, clock, admission, gate, ring
and store seam below therefore keeps both its shape and its line numbers,
including the corrections the independent review recorded:

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
and the existing sentinel as the only independent holder. Opt-in is a flag on
`run prepare` plus a required caller-supplied attempt label, not an environment
variable: environment variables are inherited by nested invocations
(`src/cli/entrypoint.rs:144-145` shows that inheritance is deliberate for the
lock wait), and a silently inherited custody mode would change nested runs.

Shape:

1. **Refusal before the first effect.** `run prepare --custody --attempt <label>`
   refuses at `src/runner/run_control.rs:95` when the label is absent, already
   holds a live attempt, or the sentinel below cannot start. Refusal precedes
   the effects-root open at `:101`, so a refused attempt allocates nothing.
2. **Owner E** is the invoking `effigy` process.
3. **Worker W** is one `Command`-spawned helper in the same style as the sentinel
   (`crates/effigy-kernel/src/supervise/memory.rs:239-252`), and performs the
   whole first-effect sequence of the step list above. E holds a
   `std::process::Child` for W. Moving the effects into a child is what makes a
   total bound possible at all: E's clock loop is no longer blocked by W's
   `fsync`, SQLite busy wait or rename.
4. **Publisher S** is the existing memory sentinel, started before W, holding the
   effect and task gates plus one absolute deadline instant. It is the same
   helper, not a new controller: it clears its environment, runs `setsid`
   (`crates/effigy-kernel/src/supervise.rs:435-465`), reads
   `deadline_nanos` from its config (`crates/effigy-kernel/src/supervise/memory.rs:140,224`)
   and evaluates it locally (`:145-159`).
5. **Disposition.** E writes the attempt disposition only after it has reaped W.
   If E dies first, S takes settlement over exactly as it already does for a
   foreground run: `wait_for_closure` ends on a verifiably gone owner
   (`supervise/memory.rs:1064-1108`), `record_owner_loss` writes
   `interrupted`/`unknown` (`:1140-1181`), and `drop(gates)` is the positive
   delegated closure (`:1043`). An unproved closure parks holding the gates and
   records nothing as complete (`:1019-1043`).

### The real creator and wait graph, named

`E` = invoking effigy owner, `W` = prepare worker, `S` = memory sentinel,
`H` = admission hook, `A` = selector workload leader, `K` = anchor.

| Creator | Child it holds | Positive obligation | Source |
| --- | --- | --- | --- |
| E | W (prepare worker) | deadline-bounded TERM/CONT→KILL, then `wait()` reap before any disposition | proposal; the sibling pattern is `supervise/memory.rs:239-252` |
| E | S (sentinel) | bounded `try_wait` in `Drop`, capped by the remaining run deadline | `supervise/memory.rs:596-620` |
| E | H (admission hook, heavy runs only) | run settle stops it through its `OwnedProcess` | `src/runner/task_engine/admission.rs:744-771`, `:295-300` |
| E | A (selector leader) | peek with `waitid(WNOWAIT)`, reap with `waitpid` only after group closure, capture drain and anchor retirement | `crates/effigy-kernel/src/custody/spawn.rs:400-414,741-757,770`; `crates/effigy-kernel/src/custody.rs:1724-1822` |
| A | middle, then K (anchor) | waits the middle before workload exec; K deliberately leaves A's child set | `custody/spawn.rs:467-480,525-532` |
| S | its own workloads, in the managed route only | `Custody::spawn_all` under `__kernel-supervise` | `supervise.rs:2133` |

S is A's **sibling**, not its parent: Effigy hands S a receipt, its own gate
descriptors and the closure pin, never a child handle. Nothing in the foreground
or prepare route gives S a handle to E's children, and no receipt, EOF or census
converts one into the other.

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
  the refusal table — missing label, duplicate live attempt, sentinel
  unavailable; each refuses before any filesystem effect.
- `src/runner/run_control/tests.rs`: the production route — E starts S, then W;
  W is reaped before the disposition is written; a W that blocks in a syscall is
  killed at the attempt deadline and reaped; killing E leaves S holding the gates
  and writing `unknown`.
- Selectors: `effigy test:focused -- prepare_custody` and
  `effigy test:focused -- run_control::prepare`, plus `effigy qa:docs` for this
  document. Budget: per-test wall clock ≤ 60 s, no fixture longer than the
  workspace test-runner timeout, no native, image, VM or guest probe.

### Refused here

Claimed handle transfer from W or S to the consumer or to the later selector; a
subreaper or a new controller; a hard blocked-I/O completion bound; any physical
allocation guarantee; maintenance prepare treated as admission; a prepare
receipt treated as a resource ledger.

## Scope 2: the common elapsed clock

### What exists

One opt-in absolute deadline per invocation (`EFFIGY_RUN_DEADLINE_MS`,
`crates/effigy-core/src/run_deadline.rs:29`), configured once at CLI start
(`src/cli/entrypoint.rs:23-27`). Its origin is sampled once at process start,
before any capture, staging or admission, on the monotonic clock, and is never
restarted (`run_deadline.rs:1-8`). The ceiling is stored as absolute
`CLOCK_MONOTONIC` nanoseconds (`:82,110,239-249`), a settlement allowance of 10 s
is reserved inside it (`:34`), and the evidence reserves are 1 s each (`:39,43`).

The clock already crosses one process boundary: the owner passes
`ceiling_nanos()` as `deadline_nanos` when it starts the guard
(`crates/effigy-kernel/src/run/foreground.rs:167-169`;
`crates/effigy-kernel/src/run/checkout_custody.rs:63`), and the guard evaluates
`deadline_left`/`deadline_passed` against the same monotonic source in its own
process (`supervise/memory.rs:145-159`). That is the shape `common_elapsed_clock`
needs; what it lacks is a target.

### Remaining target

1. **An attempt origin, not only an invocation origin.** Today the only origin is
   process start. A prepare attempt that begins late inherits a partly spent
   budget and cannot state its own bound. The attempt must take its origin at its
   first mandatory effect — before the effects-root open at
   `src/runner/run_control.rs:101` — and its bound must be
   `min(attempt_origin + attempt_ceiling, invocation_ceiling)`.
2. **Exactly one absolute instant across the boundary.** The attempt publishes
   the absolute instant, never a duration and never an elapsed reading, so no
   holder reconstructs time. One conversion point exists and stays unique:
   `Instant` in the owner, `ceiling_nanos` at the send
   (`run/foreground.rs:167-169`).
3. **Phase remainders inside the attempt.** Store open, ring publication, gate
   preparation, disposition write and delivery each read their remainder from the
   same clock through the existing phase names
   (`run_deadline.rs:52-58`). No second timer is introduced for any of them.

### Semantics the interface must state

- **Blocked I/O.** The clock is cooperative. `ctx.operation().check(..)` is only
  consulted between operations (`crates/effigy-kernel/src/ctx.rs:292-304`), and
  SQLite busy waits, `fsync` and `rename` are ordinary blocking calls. The
  attempt therefore gets its bound from its shape, not from the clock: the work
  runs in W so that E's loop can escalate and reap. Whether a kill lands inside a
  blocked syscall and whether the wait returns promptly are **not** guaranteed;
  the interface must say so rather than imply a total bound.
- **Owner loss.** S is the only clock holder that survives E. It evaluates the
  same absolute instant, and on expiry or observed owner loss it finishes
  settlement, records `unknown`/`interrupted` (`supervise/memory.rs:1140-1181`)
  and either closes (`:1043`) or parks holding the gates (`:1019-1043`). Deadline
  expiry never releases retained state: release is an explicit later action
  (`:1035-1042`).
- **One clock, one domain.** Both readings used today — `Instant::now` and
  `system_monotonic_nanos` — are the same host monotonic domain, and the design
  permits exactly one conversion between them, at owner→guard. No new inner
  clock, no wall-clock reading, no cross-clock or RAW conversion.

### Focused controls and budgets

- `crates/effigy-core/src/run_deadline.rs`'s owned injectable clock
  (`:251-254`) drives attempt-origin and phase-remainder tests with no sleeping.
- `src/runner/run_control/tests.rs`: the attempt refuses a ceiling that cannot fit
  the 10 s settlement allowance; the attempt bound is the minimum of the two
  ceilings; an expired attempt parks retained state instead of releasing it.
- Selectors: `effigy test:focused -- run_deadline`, `effigy test:focused --
  prepare`, `effigy qa:docs`. Budget: the owned-clock tests are sub-second; the
  route tests need the workspace runner timeout only for their hostile-fixture
  cases.

### Refused here

A hard blocked-I/O guarantee; a clock supplied to and read by the consumer as
authority over its own elapsed accounting; deriving elapsed time from a duration
or from the journal; a second clock for prepare; any RAW conversion; using the
deadline as authority to release, evict or reuse a retained attempt.

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
(`src/runner/task_engine/admission.rs:442-467`;
`crates/effigy-kernel/src/custody.rs:1137-1175`). A refused or blocked pre-exec
leader is killed and waited by its actual parent
(`custody/spawn.rs:710-738`). Completion evidence is the terminal report
(`admission.rs:1044-1090`), which stops at the acknowledged prefix
(`:481-500`), plus the narrow Q-010 row bound to run, checkout and boot but
carrying no process identity (`:1099-1163,1164-1190`). Report failure is a warning and a
`settlement_incomplete` flag, not a retry (`:1074-1090`).

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
  leader identity, the acknowledged prefix is gap-free
  (`admission.rs:296-320`) and the terminal disposition came from a settled
  outcome. Otherwise the disposition is `unknown`.

The prepare attempt of scope 1 never produces an admission receipt. Its
resolution and the later selector's resolution both call
`build_execution_preflight` (`run_control.rs:224`), so the digest is the only
addition needed to show both describe the same selector — the second resolver is
not duplicated.

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
to make completion look attributable.

## The rest of the Spec064 list

| Interface | Effigy's position at `522c939e` |
| --- | --- |
| `creator_waits` | Effigy-owned **witness** only: the obligation table above, plus a receipt field naming which waits completed. No API for a third party to register waits. |
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
| `crates/effigy-kernel/src/prepare_custody/worker.rs` (new) | W: the helper that performs the first effect, and E's reap obligation. |
| `src/runner/run_control.rs` | Route `run prepare --custody` through the attempt; keep `--check` as a mode of it. |
| `crates/effigy-core/src/run_deadline.rs` | Attempt origin and the minimum-of-ceilings rule; one conversion point. |
| `src/runner/task_engine/admission.rs` | The receipt fields and the `unreported` disposition. |
| `crates/effigy-kernel/tests/prepare_custody.rs` (new), `src/runner/run_control/tests.rs` | The focused controls named above. |
| `changelog.d/<task>-<slug>.<type>.md` | One paragraph describing the actual landed level. |

## Unsupported interfaces, consolidated

These stay unsupported in any reading of this design, and a future acceptance
must refuse rather than exempt:

- claimed handle transfer between owner, worker, sentinel and consumer;
- a subreaper or a new controller;
- maintenance treated as admission;
- assumed cross-clock or RAW conversion;
- a new inner clock;
- group or escapee signals treated as custody;
- any hard blocked-I/O or physical-allocation guarantee;
- a decide/fit verdict issued by Effigy as authority;
- production image-attachment evidence;
- a final census treated as positive reap.

## Proof of this document

Source inspection at `522c939e` with the changed-seam diff against `dc6f5abf`,
`effigy qa:docs`, and independent exact-head review that every file:line claim
holds at the PR head. No build, probe, fixture, native or resource run was part
of this design task, and no runtime behaviour is claimed.
