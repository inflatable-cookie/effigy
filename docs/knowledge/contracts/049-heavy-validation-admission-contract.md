# 049 - Heavy Task Admission and Run Control Contract

Owner: Platform
Last Updated: 2026-11-19

Q-010 follows the storage and custody model approved by planner and Tom
(2026-10-22: "Approve") in [052 §3.1](052-runtime-kernel-contract.md#31-q-010-run-classes-and-the-durability-boundary).
Foreground evidence stays in memory; durable runs use checkout journals. The
foreground-aware task status surface uses v3 envelopes (contract 018).

Effigy offers generic heavy-task controls that any orchestrator may use, with
no commitment to one workflow (Q-006, answered by Tom on 2026-10-18). A heavy
task carries metadata only. An optional environment hook gates heavy runs, and
every run, heavy or not, gets kernel identity and custody. Foreground runs
keep state in memory; durable run control uses the owning checkout journal
(contract [052](052-runtime-kernel-contract.md) §3.1 and §5.6).

The Queue/Nucleus host-run client this contract used to specify is retired
(see [Retired](#retired)). Effigy implements no scheduler, capacity model or
admission store of its own. A provider that needs persisted machine-wide
grants uses only the bounded host-admission family in contract 052 §7.5,
never task history or a central run index.

## Classification

- A task, public or internal, may declare `admission = "heavy"`. It changes no behaviour
  by itself: a heavy task with no hook set runs exactly as a light one.
- `heavy` is the only class. The key stays a closed set of named classes, so
  a later class is added by name; an unknown class is a manifest error.
- `tasks --json` rows carry `"admission": "heavy"` for a classified task, and
  `--plan --json` carries it at the top level. Unclassified rows and plans
  keep their shape; the field is absent.
- No name implies a class. `qa`, `ci` and `ci:fresh` are ordinary tasks.
- A QA group with a heavy member is admitted once, for the whole group run.
  Group execution remains checkout-durable regardless of admission class,
  preserving contract [051](051-bounded-qa-groups-contract.md)'s retained ledger
  and post-completion status/logs (052 §3.1); no hook is required for durability.

## Admission hook

`EFFIGY_ADMISSION_HOOK` is the only mechanism. It is set in the environment
by whoever launched the agent or shell, so it reaches agent-typed and nested
invocations, which a flag cannot. Configuration never names an orchestrator
command.

When the variable is set and non-empty, a heavy run calls it after the run's
own admission (its task gate and `Admitted` record, in memory for foreground
runs or committed in the checkout journal for durable runs) and before its
first workload effect:

1. Effigy runs `/bin/sh -c "$EFFIGY_ADMISSION_HOOK"` in the invocation's
   working directory, as a process of the run under kernel custody
   (`Custody::spawn_auxiliary`): intent first in the selected run class, in its
   own process group, guarded and settled with the run. Foreground hook
   custody uses the memory guard; a durable hook uses the journal sentinel.
2. The request is one JSON object in `EFFIGY_ADMISSION_REQUEST`:

   ```json
   {
     "schema": "effigy.admission-request.v1",
     "run_id": "<kernel run id>",
     "task": "<selector>",
     "class": "heavy",
     "cwd": "<invocation directory>",
     "root": "<Effigy root>"
   }
   ```

3. The hook answers with one line on stdout: `admit`, or `deny <reason>`.
   Its stderr reaches the operator while Effigy waits, so it can report
   progress ("waiting for capacity, position 3").
4. Effigy waits for the answer within `EFFIGY_ADMISSION_HOOK_TIMEOUT_MS`
   (default one hour). A signal while waiting cancels the run.
5. After `admit`, the hook keeps running for the whole run. The run's settle
   stops it (TERM, grace, KILL by group), including when Effigy itself is
   killed: the memory guard or durable sentinel closes its hook custody. Loss
   of all foreground custodians has contract 052 §3.1's residue limit; the
   provider must verify its lease, not assume all workload descendants died.
   An orchestrator holds the slot for exactly as long as the hook process lives. A hook that exits after
   `admit` releases nothing Effigy controls. A run that holds its own hook checks it
   before remote effects, while awaiting remote registration, and while the
   remote workload runs. On loss it fails the task and settles the journaled
   remote session; an unverified closure remains unknown.

The hook's stdin stays at EOF: Effigy sends it no reports and waits for no
acknowledgements. The launcher-facing contract (the three environment names,
the request schema and fields, the decision grammar and the timeout) is pinned
by `the_hook_contract_is_pinned` in `src/runner/task_engine/admission.rs`, and
the request bytes by `admission_protocol_v1_stays_byte_identical_without_the_opt_in`
in `tests/run_control_cli.rs`.

Every failure fails closed: `deny`, a timeout, an exit before answering, a
malformed answer, a spawn failure or a signal. The run closes `NotStarted`,
no task command runs, and the error carries `effigy.admission.v1` details
with `decision` (`deny`, `timeout`, `failed` or `cancelled`), `reason`,
`task` and `run_id`.

Nested calls never queue twice. Every child an admitted run spawns gets
`EFFIGY_ADMITTED=<admitting run id>`; a heavy invocation that sees the marker
does not call the hook again, and an in-process nested run (a sequence step or
a QA member) is already inside the admission. The marker is advisory: there is
no liveness check and no descriptor behind it, so it carries nothing a child
could renew or prolong, and nothing a closed descriptor, a sleeping parent or a
dead hook can invalidate part-way through a run. A forged marker only lets a
run skip the local orchestrator's queue, which is the orchestrator's risk on a
single-user machine (Q-011). Effigy keeps no record of it: there is no run id
looked up in SQLite and no journal read.
The in-process marker is held per thread, the same way run nesting is
(sequence steps and QA members run on the thread that admitted their parent):
two runs that share one Effigy process but not a thread never see each other's
marker, so a heavy run on one thread is always gated by its own hook. A nested
run that moved to another thread is not inside this thread's admission and
queues again.


## Absolute run deadline (effigy#992, operator decision 6ca7e791)

`EFFIGY_RUN_DEADLINE_MS` opts one invocation into **one absolute deadline**
covering admission, the workload and Effigy's own settlement. It is off by
default: unset or blank means every budget is exactly what it was before.

### Clock, origin and interface

- The clock is `std::time::Instant` (monotonic). The origin is sampled **once**,
  at process start in `src/bin/effigy.rs:1-10` (and idempotently in
  `src/cli/entrypoint.rs:18-23`), before any capture, staging or admission.
  It is never restarted at a workload spawn or per step; a nested Phase
  inherits the remaining time, not a new span.
- `crates/effigy-core/src/run_deadline.rs:30` fixes the **settlement allowance**
  at 10 s. The workload boundary is `ceiling - allowance`
  (`run_deadline.rs:75`), and no wait may pass the ceiling.
- Readback: a foreground run writes one evidence line at admission naming the
  clock, its observable origin, the elapsed time since that origin, the
  ceiling, the allowance, the workload budget and the remaining time
  (`run_deadline.rs:161-190`; written at
  `src/runner/task_engine/run/foreground.rs`). The task run and plan JSON
  carry the same facts in a `deadline` object
  (`src/runner/execute/json_payload/payload.rs:40-104`), absent when the
  opt-in is off. It records `origin: "process_start"`,
  `origin_readback: "elapsed_since_origin_ms"`, `since_origin_ms`, the
  configured phase budgets (`phase_budgets_ms`) and a `phases` object with,
  for each of admission, workload, settlement and terminal, the
  `since_origin_ms` (the observable origin) and the `remaining_ms` observed
  when that phase ended (`run_deadline.rs:120-280`). Because the task JSON is
  rendered before settlement, its `deadline` object carries the phases
  observed up to that point; the completed run's full readback (all four
  phases, including settlement and terminal) is written as one post-settlement
  stderr line by `src/runner/execute/pipeline/standard.rs`, and the
  `effigy.run-deadline.v1` error carries the same complete `deadline` object.
  Phase observations are genuinely run-scoped: `run_deadline::begin_run`
  pushes a per-run frame, so a nested run's Admission, Workload or Settlement
  observation never overwrites its parent's same-named one, and concurrent
  threads never share a frame. `standard.rs` keeps the run's `PhaseHandle`
  across `close`, so a nested run's terminal observation and post-settlement
  line come from its own completed frame, never its parent's. Observations are
  taken at phase end (admission refusal, workload completion, settlement
  completion, terminal evidence), not only at start. An admission refusal's
  details are serialized after its admission phase is recorded, so they carry
  the elapsed wait. Numbers are monotonic milliseconds; nothing is converted
  to a wall clock.
- Owned tests drive phase progression through the same seam: production reads
  `Instant::now()`; a test advances a monotonic offset, never a wall clock
  (`run_deadline.rs:20-28,247-262`).

### One ceiling for every phase

| Phase | Bound | Location |
| --- | --- | --- |
| Task-gate wait | `min(--lock-wait, ceiling - allowance)`; `FailFast` unchanged | `src/runner/task_engine/run/foreground.rs:89-103` |
| Admission (including a hung hook or a late grant) | capped at `ceiling - allowance`; expiry refuses before any workload spawn, with the typed `deadline` decision | `src/runner/task_engine/admission.rs` |
| Workload lifetime | `min(timeout_ms, ceiling - allowance)`; an in-process step or nested scope may shorten it, never extend it | `src/runner/task_engine/spawn_env.rs:24-31,60-69,104-115`; `src/runner/task_engine/process.rs:145-152` |
| Kernel operation waits | an open shell operation is capped at the ceiling; a finite one is shortened to it | `crates/effigy-kernel/src/ctx.rs:192-200` |
| Settlement (escalation, guard closure, cleanup) | one cleanup context and one signal window, both capped at the ceiling | `crates/effigy-kernel/src/ctx.rs:251-276,418-425` |

A deadline that cannot fit the allowance is refused **at start**, before any
capture, gate or effect: `EFFIGY_RUN_DEADLINE_MS <= 10000` exits 2 with a
message naming the allowance (`run_deadline.rs:154-170`, `src/bin/effigy.rs`).

### Failure shapes

- **Admission expiry.** The refusal is typed: `effigy.admission.v1` with
  `decision: "deadline"` (not `"timeout"`, which stays the hook's own
  timeout) at `src/runner/task_engine/admission.rs`. The workload never
  spawns. A decision that arrives after the boundary is a **late grant** and
  refuses the same way before the workload spawns. The decision wait is
  bounded by `ceiling - allowance`, and its poll sleep never starts with less
  time remaining than its interval.
- **Settlement not positively complete.** Under the opt-in, *any* settlement
  that is not positively complete -- a cleanup-context timeout, a settle
  error, a residue effect, a child run that never reported positive
  completion, or a settled outcome whose cleanup is `Incomplete` -- records
  the run as incomplete and is never presented as success
  (`crates/effigy-kernel/src/run/foreground.rs`). **The owner keeps custody**:
  it holds the direct-child handles and runs positive settlement (effect
  stops, group closure, leader reap, guard close) bounded by the cleanup
  context, which the run deadline already caps (`ctx.rs`). An unresolved
  child obligation is added as residue only under the opt-in, so the
  no-option behaviour is unchanged. If the owner cannot finish by the
  ceiling, it records `incomplete`/`unknown` with the retained guard and every
  unresolved effect named, abandons only its own drop wait (pins released, no
  second wait), and exits by the deadline; it never claims closure. **The
  guard is bounded by the same ceiling**: the owner's original ceiling travels
  in the guard's configuration as its **absolute system-monotonic nanosecond
  value** (`Config::deadline_nanos`, because `helper()` clears the
  environment). The helper compares that absolute value and never adds a fresh
  remaining duration after its own startup, so spawn, scheduling and
  config-decode time cannot extend the deadline. Every guard settlement path
  -- the retained/prelaunch interpolation retry, effect settlement, the
  closure poll and the final terminal record -- is bounded by
  `min(60s, remaining)`, and the closure poll sleeps no longer than the
  remaining time (`supervise/memory.rs`; `settlement_budget`, `deadline_left`,
  `wait_for_closure`). A guard that reaches the ceiling with an unproved
  closure or residue records `unknown`/`incomplete` (never complete) and
  **remains alive as the verified holder of the exclusion**: it re-inventories
  the run's recorded process groups and the cleanup pin every 250 ms. A
  proved-empty group with the pin closed settles as `interrupted` and drops the
  gates. Otherwise a new invocation cannot acquire the gate until `effigy run
  release <run>` re-verifies emptiness and asks that parked sentinel to drop
  its gates (it never signals task processes). Releasing the holder is that
  explicit later action, never an implication of the deadline.
  Owner cleanup stops 1 s before the ceiling, leaving the final second for
  terminal accounting and exit; an unproved closure remains residue held by
  the guard. Scheduling and blocking I/O are not bounded by this reserve.
  The guard has receipts, not the owner's direct-child handles, and a handoff
  is never reported as complete. The task returns
  `effigy.run-deadline.v1` with `outcome: "incomplete"`, `residue: true`,
  `settlement_notes` naming the retained resources and the deadline evidence,
  and exits 124 (`src/runner/execute/pipeline/standard.rs`;
  `src/runner/error.rs`). An already-failed result keeps its own failure; the
  marker never upgrades a failure to success.
- **Custody shape is unchanged.** Escalation is still TERM, grace, KILL of the
owned group, leader reaped after group closure, residue reported. Only the
  time budgets come from the absolute deadline.

### Nested Effigy children

A child starts its own monotonic clock, so it receives only the parent's
remaining **workload** budget in `EFFIGY_RUN_DEADLINE_MS`
(`src/runner/task_engine/process.rs:520-533`). It can never inherit the
parent's configured span (that would restart the clock and extend the
ceiling). The value is applied after every inherited value and task override,
so nothing can prolong it. Each nesting level therefore reserves its own
allowance; if the remaining budget cannot fit one, the child refuses at start
rather than overrunning the parent.

### Composition

- The completeness footer still requires positive settlement: any settle that
is not positively complete calls `foreground_settled(false)`, so no footer is
printed, and `output_cap::foreground_complete()` is false
(`src/runner/task_engine/run/foreground.rs:340-380`;
`crates/effigy-core/src/output_cap.rs`).
- Off by default means byte-identical behaviour: `current()` is `None`, no
evidence line and no `deadline` field are written, and every budget keeps its
  previous value.

### Files and physical model

This feature adds **no file and no helper write**. It changes time budgets
only; the persistence inventory (state and gate files, `kernel.db` and its sidecars,
the private unlinked helper log, and the foreground terminal row) is unchanged.

## Run control

Every task run is a kernel `Run`. Foreground settlement writes one terminal
row. Durable runs journal their lifecycle (contract 052 §3.1).

- **`--run-id <key>`.** A caller-chosen key (1–128 of `A–Z a–z 0–9 . _ : -`)
  selects checkout durability before effects. The key is scoped by CheckoutId,
  so equal keys in different checkouts are independent. This deliberately
  replaces the old host-global key semantics: no central uniqueness index is
  kept. Within a checkout the first invocation starts, a repeat/race attaches
  and returns its result, and a key bound to a different task refuses. Keys
  remain unique within retention (newest 20 settled runs per task). Records
  found after a concurrent admission failure are checked against the recorded
  task before attachment; an unreadable record never authorizes attachment.
  Managed tasks keep their existing `--run-id` refusal.
- **`effigy runs [--limit N] [--all]`.** This checkout's durable task and
  QA-group runs, foreground terminal rows and verified live diagnostics, newest first.
  `--all` uses bounded checkout discovery in 052 §7.6. JSON remains
  `effigy.runs.v1`; additive provenance/coverage fields distinguish durable
  rows, live diagnostics, residue and partial discovery. Unavailable fields
  are explicit, never fabricated historical outcomes.
- **`effigy run status <run>`.** Resolves the current checkout first, then
  discovers candidates if needed; ambiguous keys refuse and name the roots.
  Durable state/exit/cleanup comes from that checkout's journal and gate.
  Live foreground state requires verified holder identity; a held gate whose
  recorded owner is verifiably gone reads `interrupted` and names
  `effigy run stop <run>` as its settlement. After exit the answer is
  the terminal row when available, or `not recorded` when absent or expired.
  It never infers success from a free gate. JSON remains `effigy.run.v1` with evidence provenance.
- **`effigy run logs <run> [--follow] [--stdout-offset N] [--stderr-offset N]`.**
  Durable runs only: redacted output per stream at absolute byte offsets.
  `--follow` drains to the final outcome and exits with the run's result.
  JSON keeps `effigy.run.logs.v1`, base64 data and each `next_offset`.
  Foreground runs have no retained log; the command reports that explicitly.
- **`effigy run stop <run> [--wait-ms MS]`.** Durable runs keep the journal's
  generation-fenced Stop, verified holder signal and interrupted-settle path.
  Foreground runs permit only a freshly verified live-owner stop; stale gate
  bytes or discovered residue grant no signalling authority. A foreground run
  whose owner is verifiably gone signals nothing: the command waits for that
  run's own guard to settle and reports the row it wrote (else that no outcome
  was recorded). An owner that cannot be verified is refused. Stop cannot
  resurrect a vanished foreground record or signal its old group by number.

`--run-id` still captures output unless both stdout and stderr are terminals;
relay/redaction, missing-log diagnostics and workload outcome rules stay as
before. `--json` alone does not force persistence: foreground JSON captures
in bounded memory and streams to its caller. Other foreground text invocations
keep the operator's descriptors untouched. A durable capture failure never
changes the workload's outcome (K10).

### Queue admission without a run index

The v1 request already supplies `run_id`, `cwd` and `root`. Queue holds capacity
through the live hook process/pipe lease and verifies that lease's identity;
it must not infer liveness by searching `runs --all` or opening a machine
journal. A diagnostic request for a durable run opens the request's checkout
root after verifying its identity. A foreground run may have no stored status.
The hook's own grant/waiter store is provider-owned; if it uses Effigy's machine
store it obeys 052 §7.5's allowlist and one-write-per-transition budget. Waiting,
position reports, polling and inherited markers cause no Effigy database writes.
Admission metadata alone never changes a foreground run into a durable one.

### Follow guarantees

For durable runs these close the lost-attach class (papercut 910c578a):

1. **Lost follow, run continues.** A follower that loses its stream before,
   during or after settle loses nothing. A new follower binds to the exact
   run id and resumes at the delivered offsets: no gap, no duplicate. The run
   is never touched or resubmitted.
2. **Terminal, stream unavailable.** When the run is terminal and its log is
   gone, the follower reads the settled outcome from the journal and exits
   with the run's real result. `unknown` is reported when the journal, the
   run's workload evidence, or one stream's log read cannot be verified:
   each observation retries within its own 30-second budget — a different
   stage's success never renews it — and a spent budget ends the follow with
   the reason it saw, never as a settled or drained success (R3), with a
   message naming the run for `effigy run status`.
3. **Exact binding.** A follower binds to one run id, never another run of the
   same task or key. Expired log bytes (rotation keeps the newest segments of
   each stream) and failed log writes are reported, never silently skipped.
   An unverifiable read holds the delivered offset: if it recovers, the read
   continues exactly there — no gap, no duplicate.

Logs are kernel family 6: `<checkout>/.effigy/state/runs/<run-id>/<seq>.<stream>.<offset>.log`,
1 MiB segments, the newest four kept per stream, removed with the run by
retention.

## Retired

The host-run client and everything Q-006 lists went on 2026-10-19:
`src/runner/host_scheduler`, the `effigy-host-run` crate, parent tokens
(`HOST_RUN_TOKEN`, `HOST_RUN_ID`), the scheduler override and setting
variables, exit codes 75 (`scheduler_unreachable`) and 77
(`invalid_parent_token`) as scheduler results, `--host-run-request-id`,
`effigy tasks request status|follow`, the `qa`/`ci`/`ci:fresh` name rule, the
private-parent fixture harness, and every Effigy-side client obligation this
contract held. Exit 75 now means only a held task gate. The retired flag and
command refuse with the replacement. Historical host-run state under
`~/.local/state/host-run/` and the older admission store under
`~/.cache/effigy/admission/` are left untouched.

Retired in v0.16 (never released):

- **Strict persistence and the fixed terminal ring.** `EFFIGY_STRICT_PERSISTENCE`,
  `effigy run prepare` (and `--check`), gate pre-creation, strict SQLite write
  mode, the ring file and the APFS allocation rules. Runs need no preparation
  step. The one terminal row lives in the checkout journal.
- **The reported custody protocol.** `EFFIGY_ADMISSION_PROTOCOL=2`,
  `effigy.admission-request.v2`, the `custodians`, `workload` and `terminal`
  reports and their `ack` lines, the coverage witness and the release policy
  built on them. The hook keeps the v1 request and answer.

A launcher that still sets `EFFIGY_STRICT_PERSISTENCE` or
`EFFIGY_ADMISSION_PROTOCOL` is refused at startup with one message that names
the variable. Queue's migration to the hook is described in
[guide 080](../../guides/080-host-wide-validation-admission.md).

## Evidence and validation

| Guarantee | Proof |
| --- | --- |
| Hook admit, hold, release; deny; timeout; crash; nested marker; no-hook default; `qa`/`ci` | `test:run-control:cli` |
| v1 stays byte for byte: unchanged request and stdin at EOF | `test:run-control:cli` (`admission_protocol_v1_stays_byte_identical_without_the_opt_in`) |
| The in-process marker is per thread: a heavy run on another thread of the same process still takes the hook | `test:run-control` (`admission::tests::a_held_admission_marker_belongs_to_the_thread_that_holds_it`) |
| `--run-id` racing invocations and repeats | `test:run-control:cli`; `test:run-control` |
| `run stop` settles a running task and its children | `test:run-control`; `test:run-control:cli` |
| `logs --follow` ends on settle; killed follower reattaches by offset | `test:run-control:cli` |
| Stream loss before, at and after settle (engine fault points `FollowRead`, `FollowSettled`, `FollowDone`) | `test:run-control` (fault-injection build) |
| Terminal run without logs; expired bytes; unreadable journal | `test:run-control`; `test:run-control:cli` |
| An unverifiable workload observation or unread stream ends unknown within its own budget, never settled or drained success; a recovering observation resumes at the exact offset with no gap or duplicate (hook `RunEffects`; hidden stdout segments) | `test:run-control` |
| Log segments, offsets, expiry, truncation, read fault | `test:kernel:run` |
| One foreground admission writes a fixed, bounded gate set, and a gate body stays inside its bound whatever the holder's `argv0` is | `test:kernel:gate` (`gate_foreground_admission_writes_a_fixed_bounded_set`); `test:kernel:gate` (unit: `a_gate_body_is_bounded_whatever_the_holder_looks_like`) |


Q-010 implementation adds the classification, zero-write and multi-checkout
discovery proofs in contract 052 §12.6 to these same run-control selectors. These are future acceptance requirements, not Phase 1
proof results.
