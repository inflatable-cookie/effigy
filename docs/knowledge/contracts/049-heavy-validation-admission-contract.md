# 049 - Heavy Task Admission and Run Control Contract

Owner: Platform
Last Updated: 2026-10-22

Q-010 touch-points below are proposed with contract [052](052-runtime-kernel-contract.md)
§3.1 and §7, approved by planner and Tom (2026-10-22: "Approve") for implementation. The existing
binary still journals every task in the central store. The amended contract
distinguishes foreground memory from durable checkout runs; it does not claim
the cutover has shipped.

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

## Run control

Every task run is a kernel `Run`. Only durable runs have a journal. The
following is the proposed Q-010 control boundary (contract 052 §3.1).

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
