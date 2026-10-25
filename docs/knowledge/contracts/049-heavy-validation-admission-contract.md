# 049 - Heavy Task Admission and Run Control Contract

Owner: Platform
Last Updated: 2026-10-25

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
   `admit` releases nothing Effigy controls.

Every failure fails closed: `deny`, a timeout, an exit before answering, a
malformed answer, a spawn failure or a signal. The run closes `NotStarted`,
no task command runs, and the error carries `effigy.admission.v1` details
with `decision` (`deny`, `timeout`, `failed` or `cancelled`), `reason`,
`task` and `run_id`.

Nested calls never queue twice. Every child an admitted run spawns gets
`EFFIGY_ADMITTED`, an opaque marker naming the admitting run; an in-process
nested run (a sequence step or a QA member) is already inside the admission.
A nested heavy invocation with a live marker skips the hook. A marker whose
run has ended does not bypass the hook. Under Q-010, a foreground marker
is an opaque capability bound to an inherited lease descriptor and the live
hook's verified identity, not a run id looked up in SQLite. A child receives
only the lease-observation end, never the lifetime writer; it cannot prolong
admission after the holder exits. A forged token, missing/closed descriptor,
unknown hook identity or dead lease cannot bypass admission. Durable runs use
the same live-capability test, not a global journal lookup. The in-process
marker is held per thread, the same way run nesting is (sequence steps and QA members run on
the thread that admitted their parent): two runs that share one Effigy
process but not a thread never see each other's marker, so a heavy run on
one thread is always gated by its own hook. A nested run that moved to
another thread is not inside this thread's admission and queues again,
which fails closed.


## Reported custody protocol (v2)

An orchestrator that needs its own evidence can opt in to a second protocol
(effigy#325). The two protocols coexist: **v1 is unchanged byte for byte**
unless the launcher opts in, because today's `effigy.admission-request.v1`
consumers parse strictly.

### Opt-in and rollout

`EFFIGY_ADMISSION_PROTOCOL=2`, set next to the hook, selects v2. Unset or
empty keeps v1 exactly: the v1 request, no stdin events, no terminal record.
Any other value fails closed with the existing `effigy.admission.v1` `failed`
error naming the variable, so a typo cannot silently run unreported.

The answer grammar does not change: the hook still prints `admit` or
`deny <reason>`. A v2-capable hook that answers in v1 form is fine.

Rollout: the provider ships a v2-capable hook first, then sets the variable.
Downgrade means unsetting it; nothing else changes.

Under v2 the request is `effigy.admission-request.v2`, carrying every v1 field
plus `"protocol": 2`.

**v2 is the foreground class's protocol.** Its `custodians` report names the
memory guard, and only a foreground run has one (Q-010); a durable run keeps a
journal sentinel instead and already records its own run row. A durable run —
`--run-id`, or a run inside a durable parent tree — therefore refuses v2
**before the hook starts**, with the existing `effigy.admission.v1` `failed`
error naming `EFFIGY_ADMISSION_PROTOCOL` and its next action ("unset it for
this run, or run it without `--run-id`"). Reporting an incomplete custodian
set would be worse than no report, and silently downgrading to v1 would let a
provider believe it had coverage it does not have. With the variable unset, a
durable run is admitted exactly as today. The terminal record below is
foreground-only for the same reason.

### Reports and acknowledgements

Under v2 Effigy writes newline-delimited `effigy.admission-event.v1` JSON
reports to the hook's **stdin**, each with a per-run `seq`. The hook
acknowledges each one with `ack <seq>` on its stdout.

| Report | Shape | When |
| --- | --- | --- |
| `custodians` | `{schema, event:"custodians", seq:1, run_id, owner:{boot,pid,start}, guard:{boot,pid,start}}` | after `admit`, before any workload spawns |
| `workload` | `{schema, event:"workload", seq, run_id, pgid, leader:{boot,pid,start}}` | inside the spawn receipt/ack barrier, before that group's `go` |
| `terminal` | `{schema, event:"terminal", seq, run_id, outcome, exit_code, signal, cleanup, workload_groups}` | at settle, before the hook is stopped |

`owner`, `guard` and `leader` are the kernel's `ProcessIdentity`; `pgid` is
the group the leader owns.

Delivery is acknowledged, never assumed. Effigy waits `ack <seq>` within a
bounded timeout (10 s per report). A write failure, EOF, a malformed or
out-of-order ack, or a timeout **stops the workload before `go`**: the child is
still behind the spawn barrier, so it exits without exec and its group is left
empty, and the run settles with the existing admission failure
(`effigy.admission.v1`, decision `failed`). A workload therefore never runs
unreported under v2. Writes are bounded too: a hook that stops reading causes
ack timeouts, so a broken hook fails the run closed instead of stalling it.

A report that is not delivered also **ends the sequence for that run**: Effigy
writes no further report, so the hook is never handed a later `seq` and the
acknowledged prefix keeps no gap. A failed write may have left part of a line,
which no framing can repair, so a later report would corrupt the channel; the
hook must not acknowledge an incomplete last line. The narrow terminal record
below is still written, because it is the run's own settled record.

A missing acknowledgement for the `terminal` report does not change the run's
result: the durable terminal record is the source of truth.

### Coverage witness

The acknowledged sequence itself is the coverage witness, and it works without
any terminal report:

- **Custodians first.** Seq 1 is always `custodians`, acknowledged before any
  workload is spawned. In-process work — built-ins, in-process sequence steps —
  runs inside the owner process and dies with it, so the owner's identity
  covers it.
- **Strictly sequential.** Event n+1 is never written until n is
  acknowledged, and a report that is not delivered ends the sequence instead of
  skipping ahead, so the acknowledged seqs are always a gap-free prefix `1..n`.
- **No unacknowledged `go`.** A workload group receives `go` only after its
  own `workload` report is acknowledged. A spawned child held at the barrier
  whose report was never acknowledged never execs its workload: on barrier
  failure or owner death it exits without running.
- **An ack means recorded.** `ack <seq>` is the hook's statement that it has
  durably recorded that event. A provider that acknowledges before recording
  breaks its own coverage.
- **Zero vs unknown.** An acknowledged `custodians` with zero `workload`
  events means no workload group ever started: a legitimate zero. No
  acknowledged `custodians` event means nothing is known, and the release rule
  below does not apply.
- **Pending events.** An event the provider received but did not acknowledge
  belongs to a group that never got `go`. A provider may still include its
  identity in liveness checks; doing so is harmless.
- **The terminal count is only a cross-check.** `workload_groups` adds nothing
  to the witness. Missing, partial or unacknowledged events still mean
  unknown, never "empty".

An out-of-process nested invocation (a child Effigy process) is one workload
group of its parent's run and reports through its parent's hook only as that
process. Its own workload groups are its own admission's concern.

### The terminal record

Only v2 hook-admitted heavy foreground runs write it: **one row** to the
owning checkout journal at settle, in the additive schema-1
`foreground_terminals` table (Q-010 storage; additive, so an older reader
selects the columns it knows and needs no version bump). The row holds the run
ID, the `CheckoutId`, the boot identity, the task, the outcome, the exit code
or signal, `cleanup`, the acknowledged `workload_groups` count and the closed
time. It is retained for seven days.

It is **evidence, never authority**, and carries no PID, PGID or start
identity: nothing in it can be signalled, reclaimed or recovered from.

`effigy run status <run_id> --json` (`effigy.run.v1`) returns it with
`evidence_source: "foreground-terminal-record.v1"`, plus `checkout_id` and
`boot`, so a consumer can bind it to the run, the checkout and the runtime.
`effigy runs` lists it beside durable runs. `run logs` reports that a
foreground run kept no log, and `run stop` refuses: there is no live owner and
the reported identity grants no signal.

The only custody closure Effigy asserts from this is a terminal result with
`cleanup: "complete"` (052 R5/I-R7: Retained and Irreversible count as
resolved; Residue and Unknown do not). There is no replay or recovery for
foreground runs: if every custodian dies before settle, nothing is recorded and
`run status` keeps saying `not-recorded`. A checkout that has never recorded
runtime state gets its journal binding at settle, which is the same additive
write the row needs; no run identity is minted for a foreground run.

Runs without v2 keep Q-010 exactly, including no journal write at all.

### Release policy (Tom, decision d3bad004 v2)

Tom on 2026-10-24: "Yes: accept the escaped-process risk, and Queue releases
automatically on the evidence." A provider **may release its own capacity** on
v2 evidence once all four of these hold:

- full coverage, meaning an acknowledged `custodians` event plus the gap-free
  acknowledged prefix, with no terminal count needed;
- every reported identity (owner, guard and each workload leader) no longer
  matches a live process;
- every reported group is empty (`killpg(pgid, 0)` → ESRCH);
- there is no terminal result.

The accepted risk is named: a process that escaped its group may still be
running. The limits are named too:

- **Coverage must be complete.** Missing, partial or unacknowledged events mean
  unknown, and the rule does not apply.
- **Effigy asserts no closure.** It never claims closure from this evidence and
  gains no signal or reclaim authority (052 §3.1).
- **`run status` never reports a provider-side release.** A release on this
  evidence is never shown as `cleanup: "complete"`.

## Run control

Every task run is a kernel `Run`. Only durable runs have a journal. The
following is the Q-010 control boundary (contract 052 §3.1).

- **`--run-id <key>`.** A caller-chosen key (1–128 of `A–Z a–z 0–9 . _ : -`)
  selects checkout durability before effects. The key is scoped by CheckoutId,
  so equal keys in different checkouts are independent. This deliberately
  replaces the old host-global key semantics: no central uniqueness index is
  kept. Within a checkout the first invocation starts, a repeat/race attaches
  and returns its result, and a key bound to a different task refuses. Keys
  remain unique within retention (newest 20 settled runs per task). Managed
  tasks keep their existing `--run-id` refusal.
- **`effigy runs [--limit N] [--all]`.** This checkout's durable task and
  QA-group runs plus verified live foreground diagnostics, newest first.
  `--all` uses bounded checkout discovery in 052 §7.6. JSON remains
  `effigy.runs.v1`; additive provenance/coverage fields distinguish durable
  rows, live diagnostics, residue and partial discovery. Unavailable fields
  are explicit, never fabricated historical outcomes.
- **`effigy run status <run>`.** Resolves the current checkout first, then
  discovers candidates if needed; ambiguous keys refuse and name the roots.
  Durable state/exit/cleanup comes from that checkout's journal and gate.
  Live foreground state requires verified holder identity; after exit the
  answer is `not recorded (foreground or expired)`. It never infers success
  from a free gate. JSON remains `effigy.run.v1` with evidence provenance.
- **`effigy run logs <run> [--follow] [--stdout-offset N] [--stderr-offset N]`.**
  Durable runs only: redacted output per stream at absolute byte offsets.
  `--follow` drains to the final outcome and exits with the run's result.
  JSON keeps `effigy.run.logs.v1`, base64 data and each `next_offset`.
  Foreground runs have no retained log; the command reports that explicitly.
- **`effigy run stop <run> [--wait-ms MS]`.** Durable runs keep the journal's
  generation-fenced Stop, verified holder signal and interrupted-settle path.
  Foreground runs permit only a freshly verified live-owner stop; stale gate
  bytes or discovered residue grant no signalling authority. Stop cannot
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
   with the run's real result. `unknown` is reported only when the journal
   itself cannot be observed (R3), with a message naming the run for
   `effigy run status`.
3. **Exact binding.** A follower binds to one run id, never another run of the
   same task or key. Expired log bytes (rotation keeps the newest segments of
   each stream) and failed log writes are reported, never silently skipped.

Logs are kernel family 6: `<checkout>/.effigy/state/runs/<run-id>/<seq>.<stream>.<offset>.log`,
1 MiB segments, the newest four kept per stream, removed with the run by
retention.

## Retired

The host-run client and everything Q-006 lists went on 2026-10-19
(`retired.toml` `effigy-host-run-client`): `src/runner/host_scheduler`, the
`effigy-host-run` crate, parent tokens (`HOST_RUN_TOKEN`, `HOST_RUN_ID`), the
scheduler override and setting variables, exit codes 75 (`scheduler_unreachable`)
and 77 (`invalid_parent_token`) as scheduler results, `--host-run-request-id`,
`effigy tasks request status|follow`, the `qa`/`ci`/`ci:fresh` name rule, the
private-parent fixture harness, and every Effigy-side client obligation this
contract held. Exit 75 now means only a held task gate. The retired flag and
command refuse with the replacement. Historical host-run state under
`~/.local/state/host-run/` and the older admission store under
`~/.cache/effigy/admission/` are left untouched.

Queue's migration to the hook is described in
[guide 080](../../guides/080-host-wide-validation-admission.md).

## Evidence and validation

| Guarantee | Proof |
| --- | --- |
| Hook admit, hold, release; deny; timeout; crash; nested marker; no-hook default; `qa`/`ci` | `test:run-control:cli` |
| v2 refuses a durable run before the hook starts; the same run without the opt-in is admitted as v1 | `test:run-control:cli` (`admission_protocol_v2_refuses_a_durable_run_before_the_hook`) |
| v1 stays byte for byte: unchanged request, stdin at EOF, no journal | `test:run-control:cli` (`admission_protocol_v1_stays_byte_identical_without_the_opt_in`) |
| v2 reports every workload group with its real PGID and leader, the group's first effect only after its ack, one terminal report, and the `run status`/`runs` terminal record | `test:run-control:cli` (`admission_protocol_v2_reports_every_workload_group_and_the_terminal_result`) |
| v2 fails closed for a missing ack, hook EOF, a malformed ack and a write failure | `test:run-control:cli` (`admission_protocol_v2_fails_closed_when_a_report_is_not_acknowledged`) |
| The coverage witness without a terminal report: two acknowledged groups, a third held at the barrier; and custodians alone | `test:run-control:cli` (`admission_protocol_v2_coverage_witness_survives_losing_both_custodians`, `..._of_zero_workload_groups`) |
| A reported identity grants no signal or reclaim | `test:run-control:cli` (`admission_protocol_v2_reported_identity_grants_no_signal_or_reclaim`); `test:kernel:run` (`run_terminal_record_round_trips_and_an_older_journal_reads_absent`) |
| The in-process marker is per thread: a heavy run on another thread of the same process still takes the hook | `test:run-control` (`admission::tests::a_held_admission_marker_belongs_to_the_thread_that_holds_it`) |
| `--run-id` racing invocations and repeats | `test:run-control:cli`; `test:run-control` |
| `run stop` settles a running task and its children | `test:run-control`; `test:run-control:cli` |
| `logs --follow` ends on settle; killed follower reattaches by offset | `test:run-control:cli` |
| Stream loss before, at and after settle (engine fault points `FollowRead`, `FollowSettled`, `FollowDone`) | `test:run-control` (fault-injection build) |
| Terminal run without logs; expired bytes; unreadable journal | `test:run-control`; `test:run-control:cli` |
| Log segments, offsets, expiry, truncation, read fault | `test:kernel:run` |

Q-010 implementation adds the classification, zero-write, multi-checkout
discovery and admission-capability proofs in contract 052 §12.6 to these same
run-control selectors. These are future acceptance requirements, not Phase 1
proof results.
