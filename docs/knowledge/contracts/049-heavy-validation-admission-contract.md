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

A v2 hook-admitted heavy foreground run writes it: **one row** to the owning
checkout journal at settle, in the additive schema-1 `foreground_terminals`
table (Q-010 storage; additive, so an older reader selects the columns it knows
and needs no version bump). A foreground run of any admission whose *owner*
died is settled by its own surviving memory guard, which writes the same one
row for it (effigy#986; contract 052 §3.1). The row holds the run
ID, the `CheckoutId`, the boot identity, the task, the outcome, the exit code
or signal, `cleanup`, the acknowledged `workload_groups` count and the closed
time. It is retained for seven days.

Under strict persistence (`EFFIGY_STRICT_PERSISTENCE=1`, effigy#985) the same
record and the same fields are first written to **one slot** of the fixed-size,
preallocated ring described below instead of to SQLite, because a SQLite row has
no small physical maximum. The next writable non-strict store open ingests the
slot into this table and frees it. Until then `run status` and `runs` read the
slot directly, so the record is visible as soon as the run settles; both readers
deduplicate by run id, and the retention, history and identity semantics are the
row's own once ingested.

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

**The owner-death writer.** A guest-free foreground run keeps a memory guard
that outlives a killed owner. When the owner is verifiably gone — its recorded
boot and start identity name no live process, another process now holds that
PID, or the recorded process has exited and is an unreaped zombie — the guard
settles the run and writes exactly one row itself: `interrupted` with
`cleanup: "complete"` when the run's custody closed, `unknown` with
`cleanup: "incomplete"` when a cleanup pin or a registered effect did not. The
row is written after the guard releases its gates, so a reader never sees a
held foreground gate and a terminal record for one run (052 §3.1). It never
writes `succeeded`, and the record still carries no identity. An owner this
guard cannot verify is never treated as gone, so an unverifiable observation
keeps the gates held and writes nothing. The guard carries the
persistence class of the run it observed, so a strict run's owner-death record
is one ring slot and opens no SQLite connection, exactly like the run's own
settle would have. Runs that settle while their owner is alive keep Q-010
exactly as before, including no journal write at all for a run without v2.

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
| `kernel.db` `foreground_terminals` row | none | 1 | 960 stored bytes (non-strict; strict mode writes a ring slot instead, effigy#985) |
| `kernel.db` pages, when the store is created | none | fixed schema only | `store::SCHEMA_MAX_PAGES` = 64 pages, once per store lifetime |
| `kernel.db-wal` | none | at most 1 | 96 frames = 384 KiB appended by this write (non-strict; **0** in strict mode) |
| `kernel.db-shm` | none | at most 1 | SQLite's wal-index, sized by the log the shared store reaches (**0** in strict mode) |
| `kernel.db-journal` | none | none in the steady state | a single transaction's rollback journal, removed at commit (**0** in strict mode) |
| run journal, capture log, run history, per-run scratch | none | none | no file |

The terminal-record line counts a run that reports its own result. A run whose
owner died is settled by its surviving guard, which writes the same bounded
record instead (effigy#986) — the table's row, or its one ring slot under strict
persistence; nothing else in this table changes, and a run that settles while
its owner lives still writes none of these lines for a v1 route.

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
(`run/terminal.rs::write_row`). The page counts are **budgets the tests
measure**, not modelled worst cases: `run::TERMINAL_INSERT_MAX_PAGES` = 16 for
the insert and `run::TERMINAL_PRUNE_MAX_PAGES` = 80 for the widest prune — 96
frames (`run::TERMINAL_WRITE_MAX_PAGES`) or 384 KiB at the 4096-byte page size.
Two of their inputs are enforced by this code: the insert is refused unless the
row fits `run::TERMINAL_ROW_MAX_BYTES` and one page (so it never chains to
overflow pages), and the delete is one statement with `LIMIT`. `test:kernel:run`
(`run_terminal_write_appends_at_most_its_stated_frames`) measures the frames the
real `write_row` statements append while a reader pins the log, which is the
case where no checkpoint can hide them; `store_fresh_schema_is_bounded` holds a
created store to `store::SCHEMA_MAX_PAGES`. A modelled worst case for one
SQLite statement is *not* claimed here: `balance_nonroot` balances up to three
sibling pages into up to five new ones and rewrites their parent, and it writes
a pointer-map entry for every cell it moves, so a closed form would be far above
what this write does. The strict section below states the measured, verified
budget that replaces such a claim for the route that needs one.

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

### Strict persistence: the complete contribution (effigy#981, effigy#985)

The contribution above is honest but partial: a route that opens a store at an
older schema migrates it and backfills it (`store.rs::Store::schema`), prunes
expired rows (`run/terminal.rs::write_row`), and checkpoints its log at close
(`store.rs::Drop for Store`). Those are writes the invocation performs, so
calling them "pre-existing state" would be a false exclusion. A consumer that
must account for every byte can instead require the route to perform none of
them, with `EFFIGY_STRICT_PERSISTENCE=1` (`store::STRICT_PERSISTENCE_ENV`; read
by `store::strict_persistence`, exactly `1`, off otherwise, so every other run
stays byte-identical).

A SQLite terminal row still has no small physical maximum: the write-ahead log
it may append, with the b-tree balance and pointer-map pages it can dirty, is
bounded only by roughly the store's own size (effigy#981 derived 1,899,352
bytes from a 1 GiB store). That does not fit a consumer whose whole budget for
everything outside its guard is smaller than that. Strict mode therefore does
not write the row to SQLite at all (effigy#985): the settled record goes to one
slot of a fixed-size, preallocated ring file, and **the route opens no SQLite
connection of any kind** — not a writer, not a reader. `kernel.db`, its
`-wal` and its `-shm` are byte-identical across the route; that is measured,
not assumed.

Refusal, before the hook runs. A v2 admission is the only route that settles a
terminal record, so a strict run that will write one verifies the ring first
(`run/foreground.rs::begin_foreground_with` `:51` →
`src/runner/task_engine/admission.rs::require_strict_terminal_ring` `:140` →
`run::terminal_ring::status` `:200`). It refuses, before any gate, hook or workload
effect and without writing any file, when the ring does not exist, is not the
fixed size, holds a header this build does not write, has no free slot, or
holds a committed slot whose checksum does not match. The error is
`StoreError::TerminalRing`, which names the ring's path and, for each condition,
the step that clears it. A write that meets one of those conditions at settle
instead fails closed with the same typed error, having written at most one
slot's body and never falling back to SQLite.

`effigy run prepare` creates and fully preallocates the ring outside the route
(`src/runner/run_control.rs::prepare` → `run::terminal_ring::prepare`), beside
preparing the checkout journal, and `effigy run prepare --check` verifies both
without writing. `prepare` never repairs a ring it cannot verify: a missing file
is created, the whole 16 KiB is written through a temporary name and published
by rename, and an existing file that is the wrong size, has the wrong header, or
holds a corrupt slot is refused rather than silently reset. Opening the store
without strict persistence also ingests every committed slot and frees it, so
`run prepare` is the one remedy for a full ring.

The ring file: `<checkout>/.effigy/state/foreground-terminals.ring`, **16,448
bytes**: a 64-byte header and 16 slots of 1,024 bytes. The format lives in
`crates/effigy-kernel/src/run/terminal_ring.rs`:

| Part | Bytes | Layout |
| --- | --- | --- |
| header | 64 | magic `EFRGTERM` (8), format `1` (4), slot count `16` (4), slot size `1024` (4), header size `64` (4), 8 reserved zero bytes, SHA-256 of the first 32 bytes (32) |
| slot | 1024 | magic `EFTS` (4), slot format `1` (2), reserved (2), sequence `u64` (8), `closed_at_ms` `i64` (8), `workload_groups` `u64` (8), exit-code presence (1) and value (4), signal presence (1) and value (4), reserved (4), `run_id` (64), `checkout` (128), `boot` (64), `task` (512), `outcome` (32), `cleanup` (32), reserved, SHA-256 of bytes 0-990 (32), commit marker (1, value `0xA5`) |

Numbers are little-endian and every text field is NUL-padded to its fixed width.
The widths are the bounds `run::terminal` already enforces: an oversized
`run_id`, `checkout`, `boot`, `outcome` or `cleanup` is **refused, never cut**, so
two runs cannot coalesce on one key, and only the descriptive `task` label is cut
and marked with `[truncated]` (`run::TerminalRecord::checked`).

Crash safety. A slot is written in two ordered steps: the body (bytes 0-1022,
which include the sequence number and checksum) is written and `fsync`ed, then
the one-byte commit marker is written and `fsync`ed. A crash before the marker
leaves the slot uncommitted — any value other than `0xA5` means "no record" —
so a half-written body is never read. A slot whose marker is set but whose
magic, slot format or SHA-256 does not verify is corrupt evidence: readers
report `unknown`, and the route refuses with `StoreError::TerminalRing`. It is
never returned as a value.

Identity, idempotence and readback. Records are keyed by `run_id`. A write first
reads the committed slots, and a committed slot naming the same `run_id`
returns success **without writing**, so a repeated settle neither changes nor
duplicates the slot. Two distinct runs take distinct slots and strictly
increasing sequence numbers; two routes settling together serialize on an
`flock(LOCK_EX)` taken on the ring file itself, which is not another file and
writes no byte. `effigy run status <id>` and `effigy runs` read ring slots
directly (`src/runner/task_engine/control.rs::strict_terminal` `:312`, ring
listing `:580`), so a strict run's outcome is visible before any ingestion, and
they deduplicate a record that is both a slot and a row by `run_id`.

Ingestion. The next writable non-strict open of a checkout journal — including
`effigy run prepare` — ingests every committed slot into the ordinary
`foreground_terminals` table (`crates/effigy-kernel/src/store.rs:566` →
`run::terminal_ring::ingest`). Each row is inserted with
`ON CONFLICT(run_id) DO NOTHING` (`run/terminal.rs::insert_row`), then the slots
that were copied are cleared in one `fsync`ed step (`terminal_ring.rs::clear_ingested`);
a crash between commit and clear re-inserts idempotently and clears on the next
open. Identity, retention and history are unchanged because the rows are the same
rows, with their own `closed_at`: the seven-day retention and its bounded prune
treat them exactly as before. The strict route never ingests.

This invocation's complete physical contribution in strict mode:

| Path (under `<checkout>/.effigy/state/`) | Count | Physical bytes | Failure / finalization | Enforced by |
| --- | --- | --- | --- | --- |
| `foreground-terminals.ring` | 0 created; 1 opened read-write | 0 growth, 0 allocation: **exactly one slot's 1,024 bytes rewritten** (body 1,023 then marker 1); header and the other 15 slots unchanged; length stays 16,448 | missing, wrong size, wrong header, full or corrupt refuses before admission; a settle-time failure writes at most that slot's body and falls back to nothing | `run::terminal_ring::write` (`terminal_ring.rs:280`), `verify_size` (`:457`), `verify_header` (`:471`), `decode` (`:595`), `admission.rs::require_strict_terminal_ring` (`:140`) |
| ring file metadata | — | the file's inode `mtime`/`ctime` update; no directory entry is created or removed, so the state directory's `mtime` is unchanged | — | the only writes are the two `pwrite`s into the existing file: body `terminal_ring.rs:329`, commit marker `:335` |
| `kernel.db` | 0 | **0 bytes** | no connection is opened | `run/terminal.rs::record` strict branch (`:132-134`); measured by `strict_persistence_refuses_before_admission_and_prepare_readies_the_route` |
| `kernel.db-wal` | 0 | **0 bytes** | — | same |
| `kernel.db-shm` | 0 | **0 bytes** | — | same |
| `kernel.db-journal` | 0 | none | — | same |
| `gates/<scope>.scope` | `s` (created empty when absent) | 0 bytes each | a gate failure refuses the run | `gate.rs::Gate::acquire` (`gate.rs:388-488`; the scope marker is created at `:457-458`), unchanged by strict mode |
| `gates/<scope>/<gate>.gate` | `s` files (**no store writer gate**: the route opens no store) | at most 4,096 bytes each, rewritten in place | a failed diagnostic rewrite leaves the old body | `gate::DIAG_MAX` (`gate.rs:28`) written by `write_body` (`gate.rs:865`) and `gate/foreground.rs::set_foreground` (`:49`) |
| run journal, capture log, run history, per-run scratch | 0 | none | — | as above |

`s` is the number of checkout-scope spellings the run's task identity resolves
to (1 or 2), fixed before any file is written. Unlike the non-strict v2 route
(`s + 1`, where the terminal row opens the store and takes its writer gate), the
strict route carries only the task's own gate files, `s`: it opens no store, so
no store writer gate exists. The gate files are the route's only possible
creations, and only when absent: the **terminal record adds zero files, grows
nothing and rewrites exactly one 1 KiB slot**. A route on an already-used
checkout rewrites existing gate bodies and nothing else.

What strict mode still closes on a store it opens. These checks are not the
foreground route's — it opens no store — but a strict process may open one for
another operation, and strict mode still refuses to create, migrate, backfill,
prune, checkpoint or repair it (effigy#981):

| Enforcing check (file:line) | What it makes impossible |
| --- | --- |
| `store::open_mode` opens without `O_CREAT` under strict mode (`crates/effigy-kernel/src/store.rs:519`) | creating `kernel.db` |
| `store::refuse_recovery` (`store.rs:1816`) | opening a store whose rollback journal would be rolled back, whose log is torn, or whose log holds frames with no wal-index |
| `store::configure` strict branch calls `require_wal` (`store.rs:608`) instead of `set_journal_mode` | converting the journal mode, which writes the database header |
| `store::configure` skips `PRAGMA auto_vacuum=INCREMENTAL` in strict mode (`store.rs:603`) | writing an auto-vacuum choice |
| `store::verify_schema` with `schema_gaps` (`store.rs:795`, `:1768`) | every additive `ALTER TABLE`, table creation, index creation and backfill `Store::schema` would run |
| `store::configure` sets `PRAGMA wal_autocheckpoint=0` (`store.rs:619-624`) and reads it back (`check_sidecar_bounds`) | SQLite's own checkpoint at COMMIT |
| `store::configure` sets and confirms `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` (`store.rs:631-641`) | SQLite's checkpoint when the last connection closes |
| `store::Drop for Store` returns early in strict mode (`store.rs:1235`) | Effigy's own `PRAGMA wal_checkpoint(TRUNCATE)` |
| `store::Store::maintain` refuses a strict handle (`store.rs:991`) | the durable path's checkpoint, `VACUUM` and incremental vacuum |
| `retention::sweep` returns before opening a store (`crates/effigy-kernel/src/retention.rs:105-118`); `retention::retain` returns `skipped` (`:156-172`) | the retention pass's deletes and its maintenance |
| `run::settle::compact_machine_closed` returns early (`crates/effigy-kernel/src/run/settle.rs:67-80`) | the machine store's compaction deletes |
| `store::Store::repair` refuses under the switch (`store.rs:1093`) | moving `kernel.db` and its side files aside |

The former strict terminal-row claims are gone with the SQLite write: strict
mode states **no** write-ahead-log maximum, no wal-index growth and no page-cache
premise, because it appends no terminal frame at all. `store::STRICT_CACHE_KIB`
and `store::refuse_oversized_keys` were removed with them (effigy#985); the
`kernel.db-wal` and `kernel.db-shm` maxima above remain the *non-strict* route's.

Measured, not only derived. `test:kernel:run` proves the ring's mechanics
(`run_terminal_ring_slot_round_trips`,
`run_terminal_ring_repeated_settle_is_idempotent`,
`run_terminal_ring_distinct_runs_take_distinct_slots`,
`run_terminal_ring_refuses_an_oversized_identity_and_writes_nothing`,
`run_terminal_ring_refuses_a_missing_file_and_writes_nothing`,
`run_terminal_ring_refuses_a_size_mismatch_and_writes_nothing`,
`run_terminal_ring_refuses_a_full_ring_and_writes_nothing`,
`run_terminal_ring_detects_a_torn_slot`,
`run_terminal_ring_ingests_into_the_journal_and_frees_the_slots`,
`run_terminal_ring_status_writes_nothing_and_prepare_creates_it`).
`test:task-engine:admission`
(`strict_persistence_refuses_before_admission_and_prepare_readies_the_route`)
exercises the whole route through the real binary: a strict run without a ring
runs no hook and no workload and writes no byte, `run prepare --check` makes the
same refusal without writing, `run prepare` preallocates the ring, the strict run
then changes exactly one slot while `kernel.db`, `-wal` and `-shm` stay
byte-identical, `run status` reads the settled slot, and the next non-strict open
ingests it into the journal and frees the slot. `test:kernel:store` keeps the
strict store checks (`store_strict_open_refuses_a_store_it_would_have_to_create`,
`store_strict_open_refuses_an_unmigrated_store_and_writes_nothing`,
`store_strict_write_leaves_the_main_database_byte_identical`,
`store_strict_refuses_a_log_that_needs_recovery`,
`store_strict_refuses_a_torn_log`,
`store_strict_maintenance_and_retention_write_nothing`,
`store_strict_wal_frames_observes_without_a_checkpoint`,
`store_strict_persistence_switch_is_opt_in_and_exact`,
`store_nonstrict_open_keeps_creating_migrating_pruning_and_resetting`), which is
also the non-strict byte-identical proof: without the switch nothing changed.

Not claimed in strict mode: anything outside the maximums above — other writers'
files, the ring's pre-existing contents, the child's or the hook's own files, and
the filesystem's own allocation unit. The ring is one file of fixed size, but a
filesystem allocation observation is not a contract.

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
  when that phase ended (`run_deadline.rs:120-235`). Numbers are monotonic
  milliseconds; nothing is converted to a wall clock.
- Owned tests drive phase progression through the same seam: production reads
  `Instant::now()`; a test advances a monotonic offset, never a wall clock
  (`run_deadline.rs:20-28,247-262`).

### One ceiling for every phase

| Phase | Bound | Location |
| --- | --- | --- |
| Task-gate wait | `min(--lock-wait, ceiling - allowance)`; `FailFast` unchanged | `src/runner/task_engine/run/foreground.rs:89-103` |
| Admission (including a hung hook or a late grant) | capped at `ceiling - allowance`; expiry refuses before any workload spawn. The v2 `custodians` and `workload` report exchanges, including their writes, are bounded at the same boundary; a stalled acknowledgement refuses with the same typed `deadline` decision | `src/runner/task_engine/admission.rs:431-470,486-598,634-660` |
| Workload lifetime | `min(timeout_ms, ceiling - allowance)`; an in-process step or nested scope may shorten it, never extend it | `src/runner/task_engine/spawn_env.rs:24-31,60-69,104-115`; `src/runner/task_engine/process.rs:145-152` |
| Kernel operation waits | an open shell operation is capped at the ceiling; a finite one is shortened to it | `crates/effigy-kernel/src/ctx.rs:192-200` |
| Settlement (escalation, guard closure, cleanup, terminal evidence) | one cleanup context and one signal window, both capped at the ceiling; the v2 `terminal` report exchange and its write are bounded at the ceiling | `crates/effigy-kernel/src/ctx.rs:251-276,418-425`; `src/runner/task_engine/admission.rs:700-760` |

A deadline that cannot fit the allowance is refused **at start**, before any
capture, gate or effect: `EFFIGY_RUN_DEADLINE_MS <= 10000` exits 2 with a
message naming the allowance (`run_deadline.rs:154-170`, `src/bin/effigy.rs`).

### Failure shapes

- **Admission expiry.** The refusal is typed: `effigy.admission.v1` with
  `decision: "deadline"` (not `"timeout"`, which stays the hook's own
  timeout) at `src/runner/task_engine/admission.rs:1180-1195`. The workload
  never spawns. A grant, or a completed v2 report, that arrives after the
  relevant boundary is a **late grant** and refuses the same way before the
  workload spawns. A stalled v2 `custodians`/`workload` acknowledgement is the
  same typed refusal: the exchange's write and poll are bounded by
  `ceiling - allowance` (`admission.rs:486-598`). Admission protocol v1/v2 and
  the terminal accounting are unchanged.
- **Settlement not positively complete.** Under the opt-in, *any* settlement
  that is not positively complete -- a cleanup-context timeout, a settle
  error, a residue effect, or a settled outcome whose cleanup is
  `Incomplete` -- records the run as incomplete and is never presented as
  success (`crates/effigy-kernel/src/run/foreground.rs:258-345`). The
  surviving custodian is kept: an incomplete settle does not close, signal or
  discard the guard, so the guard retains the gates and settles residue after
  its owner's EOF; the outcome's residue names the retained guard and every
  unresolved effect (`crates/effigy-kernel/src/run/foreground.rs:320-345`).
  The owner drop does not retry stops after that
  (`crates/effigy-kernel/src/run/foreground.rs:390-400`), and the guard's own
  fallback wait is capped at the remaining time
  (`crates/effigy-kernel/src/supervise/memory.rs:548-566`). The task returns
  `effigy.run-deadline.v1` with `outcome: "incomplete"`, `residue: true`,
  `settlement_notes` naming the retained resources and the deadline evidence,
  and exits 124 (`src/runner/execute/pipeline/standard.rs:178-233`;
  `src/runner/error.rs`). An already-failed result keeps its own failure; the
  marker never upgrades a failure to success.
- **Terminal report.** A v2 `terminal` exchange is bounded at the ceiling;
  expiry is a warning (the workload result is already decided) and the
  settled outcome remains the record of truth, so no terminal row is written
  without a settled outcome (`admission.rs:700-760`).
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
- Strict persistence and the terminal ring are unchanged, and a run with no
  settled outcome still writes no terminal row.
- Off by default means byte-identical behaviour: `current()` is `None`, no
evidence line and no `deadline` field are written, and every budget keeps its
  previous value.

### Files and physical model

This feature adds **no file and no helper write**. It changes time budgets
only; the per-invocation persistence inventory above (state and gate files,
`kernel.db` and its sidecars, the private unlinked helper log, the optional v2
terminal row) is unchanged.

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
  Live foreground state requires verified holder identity; a held gate whose
  recorded owner is verifiably gone reads `interrupted` and names
  `effigy run stop <run>` as its settlement. After exit the answer is
  `not recorded (foreground or expired)`. It never infers success from a free
  gate. JSON remains `effigy.run.v1` with evidence provenance.
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
| Strict mode refuses a store it would create, or one at an older schema, and writes no byte of it | `test:kernel:store` (`store_strict_open_refuses_a_store_it_would_have_to_create`, `store_strict_open_refuses_an_unmigrated_store_and_writes_nothing`) |
| Strict mode refuses a store that needs recovery (rollback journal, torn log, log without its wal-index) | `test:kernel:store` (`store_strict_refuses_a_log_that_needs_recovery`, `store_strict_refuses_a_torn_log`) |
| A strict write never checkpoints: the main database is byte-identical with a pinned reader and through the last close | `test:kernel:store` (`store_strict_write_leaves_the_main_database_byte_identical`) |
| Strict maintenance, retention and repair write nothing | `test:kernel:store` (`store_strict_maintenance_and_retention_write_nothing`) |
| The switch is opt-in and exact, and the non-strict open still creates, migrates, prunes and resets | `test:kernel:store` (`store_strict_persistence_switch_is_opt_in_and_exact`, `store_nonstrict_open_keeps_creating_migrating_pruning_and_resetting`) |
| A strict route refuses before the admission hook when the ring is missing, the wrong size, full or corrupt; `run prepare --check` refuses without writing; `run prepare` preallocates the ring; the run then changes exactly one slot and no SQLite byte | `test:task-engine:admission` (`strict_persistence_refuses_before_admission_and_prepare_readies_the_route`) |
| A strict slot round-trips, a repeated settle is idempotent, and two runs take distinct slots | `test:kernel:run` (`run_terminal_ring_slot_round_trips`, `run_terminal_ring_repeated_settle_is_idempotent`, `run_terminal_ring_distinct_runs_take_distinct_slots`) |
| An oversized identity is refused and nothing is written; a missing file, a size mismatch and a full ring refuse and write nothing; a torn slot is never a record | `test:kernel:run` (`run_terminal_ring_refuses_an_oversized_identity_and_writes_nothing`, `run_terminal_ring_refuses_a_missing_file_and_writes_nothing`, `run_terminal_ring_refuses_a_size_mismatch_and_writes_nothing`, `run_terminal_ring_refuses_a_full_ring_and_writes_nothing`, `run_terminal_ring_detects_a_torn_slot`) |
| Ingestion copies a slot into the journal with its identity and outcome, is idempotent, frees the slot, and leaves retention pruning expired rows | `test:kernel:run` (`run_terminal_ring_ingests_into_the_journal_and_frees_the_slots`) |

Q-010 implementation adds the classification, zero-write and multi-checkout
discovery proofs in contract 052 §12.6 to these same run-control selectors. These are future acceptance requirements, not Phase 1
proof results.
