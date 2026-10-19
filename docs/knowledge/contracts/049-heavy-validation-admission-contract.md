# 049 - Heavy Task Admission and Run Control Contract

Owner: Platform
Last Updated: 2026-10-19

Effigy offers generic heavy-task controls that any orchestrator may use, with
no commitment to one workflow (Q-006, answered by Tom on 2026-10-18). A heavy
task carries metadata only. An optional environment hook gates heavy runs, and
every run, heavy or not, gets identity and control from the kernel run journal
(contract [052](052-runtime-kernel-contract.md) §5.6).

The Queue/Nucleus host-run client this contract used to specify is retired
(see [Retired](#retired)). Effigy implements no scheduler, capacity model or
admission store.

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

## Admission hook

`EFFIGY_ADMISSION_HOOK` is the only mechanism. It is set in the environment
by whoever launched the agent or shell, so it reaches agent-typed and nested
invocations, which a flag cannot. Configuration never names an orchestrator
command.

When the variable is set and non-empty, a heavy run calls it after the run's
own admission (its task gate and `Admitted` row) and before its first workload
effect:

1. Effigy runs `/bin/sh -c "$EFFIGY_ADMISSION_HOOK"` in the invocation's
   working directory, as a process of the run under kernel custody
   (`Custody::spawn_auxiliary`): journalled first, in its own process group,
   sentinelled, and settled with the run.
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
   killed: the run's sentinel settles. An orchestrator holds the slot for
   exactly as long as the hook process lives. A hook that exits after
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
run has ended does not bypass the hook.

## Run control

Every task run is a kernel `Run`. Run control reads and addresses it through
the journal only.

- **`--run-id <key>`.** A caller-chosen key (1–128 of `A–Z a–z 0–9 . _ : -`)
  derives the run's id, so equal keys name one run on the host. The first
  invocation starts the run. A repeat, or a racing second invocation, attaches
  to it and exits with its result; it never starts a second run. A key that
  already names a run of another task refuses. Keys are unique within the
  journal's retention (the newest 20 settled runs per task).
- **`effigy runs [--limit N] [--all]`.** This checkout's task and QA-group
  runs, newest first; `--all` lists the host. JSON: `effigy.runs.v1`.
- **`effigy run status <run>`.** One run by id or key: state, exit code,
  cleanup, owner pid while live. JSON: `effigy.run.v1`.
- **`effigy run logs <run> [--follow] [--stdout-offset N] [--stderr-offset N]`.**
  The run's workload output, per stream, by absolute byte offset. `--follow`
  reads until the run is terminal, drains the rest, and exits with the run's
  result. JSON returns one `effigy.run.logs.v1` envelope with base64 data and
  each stream's `next_offset`; a client polls with those offsets.
- **`effigy run stop <run> [--wait-ms MS]`.** The kernel stop path: a
  generation-fenced `Stop` request in the journal. A plain task's owner reads
  it and stops the task's whole group as TERM would (`cancelled`, exit 143).
  An owner that reads no requests gets TERM through its verified gate holder
  (`Gate::signal_holder`), never a pattern kill. An interrupted run is settled
  directly under its gate.

A plain run named by `--run-id` captures its output, and so logs it, unless
both stdout and stderr are terminals; Effigy relays it as it arrives, with
declared secrets redacted (K12). `--json` runs capture as before. Any other
text run keeps the operator's descriptors untouched and has no log, so a task
that prints a secret into a pipe still delivers it. `run logs` reports a
missing log and still reports the outcome.

### Follow guarantees

These close the lost-attach class (papercut 910c578a):

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

Logs are kernel family 6: `~/.effigy/state/runs/<run-id>/<seq>.<stream>.<offset>.log`,
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
| `--run-id` racing invocations and repeats | `test:run-control:cli`; `test:run-control` |
| `run stop` settles a running task and its children | `test:run-control`; `test:run-control:cli` |
| `logs --follow` ends on settle; killed follower reattaches by offset | `test:run-control:cli` |
| Stream loss before, at and after settle (engine fault points `FollowRead`, `FollowSettled`, `FollowDone`) | `test:run-control` (fault-injection build) |
| Terminal run without logs; expired bytes; unreadable journal | `test:run-control`; `test:run-control:cli` |
| Log segments, offsets, expiry, truncation, read fault | `test:kernel:run` |
