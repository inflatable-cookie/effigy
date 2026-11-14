# 049 - Heavy Task Admission and Run Control Contract

Owner: Platform
Last Updated: 2026-11-13

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

### Per-invocation persistence (effigy#977)

A consumer that accounts for storage must be able to name what one invocation
persists and what it does not, so this route's Effigy-owned persistence is
stated as fixed file counts, enforced row and gate maxima, and one measured
write-ahead-log contribution. The route is a plain foreground host task with one
heavy admission, text output, no declared shared lock and no `--run-id`.
Concurrent other runs, files that already existed, rows that already existed,
anything the child writes and anything the admission hook itself writes are
outside this contribution.

`s` below is the number of checkout-scope spellings the run's task identity
resolves to: 1 when the checkout's recorded token and its filesystem identity
agree, 2 when they differ. It is fixed before any file is written.

| Path (under `<checkout>/.effigy/state/`) | v1 | v2 | This invocation's maximum |
| --- | --- | --- | --- |
| `gates/<scope>.scope` | `s` | `s` | 0 bytes: created empty and never written |
| `gates/<scope>/<gate>.gate` | `s` | `s + 1` | 4096 bytes each |
| `gates/<scope>/` directory | `s` | `s` | 0700, one entry per held gate |
| `kernel.db` `foreground_terminals` row | none | 1 | 960 stored bytes |
| `kernel.db` pages, when the store is created | none | fixed schema only | `store::SCHEMA_MAX_PAGES` = 64 pages, once per store lifetime |
| `kernel.db-wal` | none | at most 1 | 96 frames = 384 KiB appended by this write |
| `kernel.db-shm` | none | at most 1 | SQLite's wal-index, sized by the log the shared store reaches |
| `kernel.db-journal` | none | none in the steady state | a single transaction's rollback journal, removed at commit |
| run journal, capture log, run history, per-run scratch | none | none | no file |

Gates. `Gate::acquire` writes one diagnostic body per held gate; both writers
of that body (`gate.rs::write_body`, and `gate/foreground.rs::set_foreground`
through it) bound it at `gate::DIAG_MAX` = 4096 bytes, so the file is a fixed
maximum whatever the holder's `argv0` or identifiers look like. The body is
diagnostics only and never authority. When even the identity part would not fit
— which the fixed-width fields cannot produce — nothing is written and the
previous body reads as a different holder, never as this one. The count is
enforced too: `run/foreground.rs::begin_under` refuses more than
`gate::FOREGROUND_GATES_MAX` = 4 aliased gates, which is the task's gate and one
declared shared lock with each one's checkout-alias spelling. The v2 store
writer gate (`Store(kernel)`) adds one gate file inside a scope one of those
spellings already names, so it adds no further scope. `FLOCK` is the exclusion;
the files themselves are never unlinked by this route, and a failed diagnostic
rewrite leaves the previous bytes.

Terminal row. `run/terminal.rs::record` validates and bounds the row at the one
place that writes it. **Identities are refused, never cut**: `run_id` (64
bytes), `checkout` (128), `boot` (64) and the closed-set words `outcome` and
`cleanup` (32 each) that exceed their maximum make
`TerminalRecord::checked` return `StoreError::Broken`, and the record is not
written — a cut key would coalesce two runs, and a cut identity would name
another runtime. The descriptive `task` label is cut to 512 bytes on a character
boundary and ends with `[truncated]`, so a long label stays visible as cut
rather than looking complete. `TerminalRecord::encoded_bytes` then refuses any
row over `run::TERMINAL_ROW_MAX_BYTES` = 960 stored bytes, which also keeps the
row below one 4096-byte page so an insert never takes an overflow chain; the
module asserts that at compile time. The maxima are wide against what the
writers produce (a `RunId` renders in 26 characters, a boot identity is a
36-character UUID). Exactly one row is written per settled v2 run, and only after
a settled outcome: a run that did not settle writes nothing, a refused record
writes nothing, and a failed write is a warning that changes no task result.

Write-ahead log: contribution, not peak. The store open applies
`journal_size_limit` = 16 MiB (`store::WAL_LIMIT_BYTES`) and
`wal_autocheckpoint` = 1024 pages (`store::WAL_AUTOCHECKPOINT_PAGES`), then
reads both back and refuses the open if either did not apply
(`store::Store::check_sidecar_bounds`). **These bound the log after a completed
checkpoint reset, not its peak.** A reader that pins an older snapshot stops a
checkpoint from completing, and writers keep appending while it does
(<https://sqlite.org/wal.html#avoiding_excessively_large_wal_files>), so the log
file's high-water mark on a shared store is a function of the writers and
readers that share it. Those connections are not this invocation's, and this
contract does not claim a peak for them.

What this invocation contributes is the frames of its own single transaction. A
terminal write issues one bounded-row INSERT and one indexed DELETE limited to
`run::TERMINAL_PRUNE_MAX` = 32 expired rows
(`run/terminal.rs::write_row`). The store cannot exceed 1 GiB = 262144 pages of
4096 bytes: `PRAGMA max_page_count` is set from `store::MAX_DB_BYTES` on every
open, and SQLite refuses growth past it with `SQLITE_FULL`. A b-tree over such a
store is at most four levels, so an insert or delete path touches at most eight
pages per tree; with the table and the `closed_at` index that is
`run::TERMINAL_INSERT_MAX_PAGES` = 16 for the insert and
`run::TERMINAL_PRUNE_MAX_PAGES` = 80 for the widest prune — 96 frames
(`run::TERMINAL_WRITE_MAX_PAGES`) or 384 KiB at the 4096-byte page size. Two of
those terms are enforced by this code: the row cap above (which is why a row
never chains to overflow pages) and the delete's `LIMIT`. The b-tree term follows
from the store's enforced page cap. `test:kernel:run`
(`run_terminal_write_appends_at_most_its_stated_frames`) measures the frames the
real `write_row` statements append while a reader pins the log, which is the
case where no checkpoint can hide them; `store_fresh_schema_is_bounded` holds a
created store to `store::SCHEMA_MAX_PAGES`.

A closing store still attempts `PRAGMA wal_checkpoint(TRUNCATE)` inside a 250 ms
window: when no reader blocks it, the log is carried into the store and
released, and when one does, the log is left as it was. Neither outcome is
needed for the contribution above. Effigy never writes a rollback journal in WAL
mode; the shared-memory index is SQLite's own wal-index and is sized by the log
the shared store reaches, which is not this invocation's to bound either.

Excluded from this contribution, with the reason: other runs' and readers'
frames in the shared log (not this invocation's writes); rows and files that
already existed, including rows an older build wrote before this bound
(`ORDER BY closed_at LIMIT` prunes them in bounded slices, but their pages are
pre-existing state); and the child's or hook's own files.

### Strict persistence: the complete contribution (effigy#981)

The contribution above is honest but partial: a route that opens a store at an
older schema migrates it and backfills it (`store.rs::Store::schema`), prunes
expired rows (`run/terminal.rs::write_row`), and checkpoints its log at close
(`store.rs::Drop for Store`). Those are writes the invocation performs, so
calling them "pre-existing state" would be a false exclusion. A consumer that
must account for every byte can instead require the route to perform none of
them, with `EFFIGY_STRICT_PERSISTENCE=1`
(`store::STRICT_PERSISTENCE_ENV`; read by `store::strict_persistence`, exactly
`1`, off otherwise, so every other run stays byte-identical). In strict mode the
route either writes only its own terminal row and its log/index frames, or it
refuses before admission.

Refusal, before the hook runs. A v2 admission settles one terminal row, the only
durable write this route makes, so a strict run that will write one verifies the
store first (`run/foreground.rs::begin_foreground_with` →
`task_engine/admission.rs::require_strict_store` →
`store::verify_prepared`). It refuses, before any gate, hook or workload effect,
when the store is absent, is missing any part of the current schema
(`store::schema_gaps`), records another schema version, does not record the
write-ahead log as its format (`store::header_facts`, the file header's byte 18),
writes pages at a size other than 4096 (`store::header_facts`), or has a
rollback journal on disk (`store::refuse_recovery`, whose first open would roll
it back into the database). The error is `StoreError::NotPrepared`, and it names
`store::PREPARE_COMMAND` — `effigy run prepare` — as the step that prepares the
store outside the route. That step opens the store *without* strict persistence
whatever the switch says (`store::Store::open_preparing`), so the switch it
satisfies cannot refuse it; `effigy run prepare --check` is the same verification
without any write.

Nothing in the route migrates, converts, prunes or checkpoints.

| Enforcing check | What it makes impossible |
| --- | --- |
| `store.rs::open_mode` opens without `O_CREAT` under strict mode; a missing file is `NotPrepared` | creating `kernel.db` |
| `store.rs::refuse_recovery` | opening a store whose rollback journal would be rolled back |
| `store.rs::configure` (strict branch) calls `require_wal` instead of `set_journal_mode` | converting the journal mode, which writes the database header |
| `store.rs::configure` skips `PRAGMA auto_vacuum=INCREMENTAL` in strict mode | writing an auto-vacuum choice |
| `store.rs::verify_schema` (`schema_gaps`) | every additive `ALTER TABLE`, table creation, index creation and backfill `Store::schema` would run |
| `store.rs::configure` sets `PRAGMA wal_autocheckpoint=0` and reads it back (`check_sidecar_bounds`) | SQLite's own checkpoint at COMMIT |
| `store.rs::configure` sets and confirms `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` | SQLite's checkpoint when the last connection closes |
| `store.rs::Drop for Store` returns early in strict mode | Effigy's own `PRAGMA wal_checkpoint(TRUNCATE)` |
| `run/terminal.rs::write_row` returns after the insert when `Txn::strict` | the bounded prune's delete statement and its frames |

The one bookkeeping write strict mode skips rather than refuses is `meta`'s
`last_written_by`: a later non-strict open updates it, and its absence does not
change what this invocation writes.

This invocation's complete physical contribution in strict mode:

| File (under `<checkout>/.effigy/state/`) | This invocation's maximum | Enforced or derived by |
| --- | --- | --- |
| `gates/<scope>.scope` | `s` files, 0 bytes each | as above |
| `gates/<scope>/<gate>.gate` | `s + 1` files, 4096 bytes each | as above |
| `kernel.db` | **0 bytes**: no checkpoint, no migration, no prune | the strict checks above |
| `kernel.db-wal` | `32 + 16 × (page size + 24)` = 65,952 bytes at 4096-byte pages | `run::TERMINAL_STRICT_WAL_MAX_BYTES` from `run::TERMINAL_STRICT_WRITE_MAX_PAGES` = `INSERT_MAX_PAGES` |
| `kernel.db-shm` | one 32 KiB wal-index page beyond what the shared log already indexed | `store::WAL_INDEX_PAGE_BYTES` |
| `kernel.db-journal` | none: a store that has one is refused | `store::refuse_recovery` |
| `foreground_terminals` row | 1, at most 960 stored bytes | `run::TerminalRecord::checked` |
| run journal, capture log, run history, per-run scratch | none | as above |

WAL arithmetic. SQLite writes a 32-byte log header
(`store::WAL_HEADER_BYTES`; <https://sqlite.org/fileformat2.html#walformat>) and
precedes every page with a 24-byte frame header
(`store::WAL_FRAME_HEADER_BYTES`, `WAL_FRAME_HDRSIZE` in SQLite's `wal.c`), so one
strict terminal write appends at most `32 + 16 × (4096 + 24)` = 65,952 bytes. The
frame count is the one this code enforces: the insert is refused unless the row
fits `run::TERMINAL_ROW_MAX_BYTES` and one page, and the prune it would have run
does not happen. The b-tree term follows from the store's enforced
`store::MAX_DB_BYTES` page cap. `effigy run prepare` also settles the page size:
a store that writes pages at another size is refused, so the number above is not
a bound for a page size this build never creates.

SHM arithmetic, and its one premise. `kernel.db-shm` holds SQLite's wal-index,
which is allocated in `WALINDEX_PGSZ` = 32 KiB pages (`store::WAL_INDEX_PAGE_BYTES`;
SQLite's `wal.c` asserts that size, and each page indexes `HASHTABLE_NPAGE_ONE` =
`store::WAL_INDEX_FIRST_PAGE_FRAMES` = 4062 frames on the first page and
`store::WAL_INDEX_PAGE_FRAMES` = 4096 on each later one). This route appends at
most 16 frames and never checkpoints, so it can at most cross one page boundary:
**one 32 KiB block**. The remaining premise is stated rather than assumed: when
the wal-index file does not exist yet, SQLite creates it sized for every frame
other writers left in the shared, unreset log — `1 + ⌈(frames − 4062) / 4096⌉`
blocks — and the read that creates it is `store::verify_prepared`, which reads
through SQLite's own log. That size is a function of other writers' frames, which
are not this invocation's to bound; the block this invocation adds on top is one.

Measured, not only derived. `test:kernel:store`
(`store_strict_open_refuses_a_store_it_would_have_to_create`,
`store_strict_open_refuses_an_unmigrated_store_and_writes_nothing`,
`store_strict_write_leaves_the_main_database_byte_identical`,
`store_strict_wal_and_index_stay_within_their_stated_maximum`,
`store_strict_persistence_switch_is_opt_in_and_exact`,
`store_nonstrict_open_keeps_creating_migrating_pruning_and_resetting`) and
`test:kernel:run`
(`run_terminal_strict_write_appends_at_most_its_stated_frames_and_defers_the_prune`)
measure each bound across a fresh store (refused), an at-schema store, a
needs-migration store (refused), a committed write, a write that fails after its
statements ran, and a write abandoned before any statement ran — with a reader
pinning the log and through the last-connection close. The main database is
compared byte for byte, and a refusal is compared byte for byte over
`kernel.db` and its log. `test:task-engine:admission`
(`strict_persistence_refuses_before_admission_and_prepare_readies_the_route`)
exercises the whole route through the real binary: a refused strict run runs no
hook and no workload and writes no journal byte, `effigy run prepare --check`
makes the same refusal without writing, `effigy run prepare` readies it, and the
same strict run then records exactly one row.

Not claimed in strict mode: the size of the shared wal-index before this
invocation, the peak of the shared log, and anything the child or the hook writes.

Not persisted. This route writes no run incarnation, request, event, capture
log, run-history row or object, and opens no per-run scratch. It mints no
checkout token: it reads the recorded one, or the same filesystem identity the
journal binding uses.

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
  remain unique within retention (newest 20 settled runs per task). Records
  found after a concurrent admission failure are checked against the recorded
  task before attachment; an unreadable record never authorizes attachment.
  Managed tasks keep their existing `--run-id` refusal.
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
| An unverifiable workload observation or unread stream ends unknown within its own budget, never settled or drained success; a recovering observation resumes at the exact offset with no gap or duplicate (hook `RunEffects`; hidden stdout segments) | `test:run-control` |
| Log segments, offsets, expiry, truncation, read fault | `test:kernel:run` |
| One foreground admission writes a fixed, bounded gate set, and a gate body stays inside its bound whatever the holder's `argv0` is | `test:kernel:gate` (`gate_foreground_admission_writes_a_fixed_bounded_set`); `test:kernel:gate` (unit: `a_gate_body_is_bounded_whatever_the_holder_looks_like`) |
| A terminal row's identities are refused rather than cut, and a refused record writes nothing | `test:kernel:run` (`run_terminal_record_identity_is_refused_not_cut`) |
| The task label is cut with the truncation marker, and the stored row is inside the stated maximum | `test:kernel:run` (`run_terminal_record_label_is_cut_and_the_row_is_bounded`) |
| One terminal write prunes a bounded slice of expired rows | `test:kernel:run` (`run_terminal_write_prunes_a_bounded_slice_of_expired_rows`) |
| One terminal write appends at most its stated frames to the log, measured with a reader pinning it | `test:kernel:run` (`run_terminal_write_appends_at_most_its_stated_frames`) |
| A created store holds only the fixed, bounded schema | `test:kernel:store` (`store_fresh_schema_is_bounded`) |
| The store open applies and verifies its log settings, and a closing writer truncates the log it wrote | `test:kernel:store` (`store_open_states_and_enforces_the_side_file_bounds`) |

Q-010 implementation adds the classification, zero-write and multi-checkout
discovery proofs in contract 052 §12.6 to these same run-control selectors. These are future acceptance requirements, not Phase 1
proof results.
