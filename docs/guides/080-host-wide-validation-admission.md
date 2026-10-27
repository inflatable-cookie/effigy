# Heavy tasks and run control

Effigy runs heavy tasks itself. If you want them to wait for capacity, give
Effigy an admission hook; Effigy never schedules work on its own. Every run,
heavy or not, can be named, inspected, followed and stopped.
Contract [049](../knowledge/contracts/049-heavy-validation-admission-contract.md)
is the authority.

## Mark a task heavy

```toml
[tasks."qa:ci:fast"]
admission = "heavy"
run = "cargo nextest run --workspace"
```

The class is metadata. With no hook set, a heavy task runs exactly as any
other. `effigy --json tasks` and `effigy --json --plan <task>` show
`"admission": "heavy"`. Task names mean nothing: `qa`, `ci` and `ci:fresh` are
heavy only when they declare it.

## Gate heavy runs with a hook

Set `EFFIGY_ADMISSION_HOOK` in the environment you launch agents or shells
from:

```sh
export EFFIGY_ADMISSION_HOOK="$HOME/bin/heavy-slot"
```

Before a heavy run starts its command, Effigy runs the hook with the request
in `EFFIGY_ADMISSION_REQUEST`:

```json
{"schema":"effigy.admission-request.v1","run_id":"agqvltk3r7jktaa6vi23dpcu4a","task":"qa:ci:fast","class":"heavy","cwd":"/work/app","root":"/work/app"}
```

The hook prints `admit` or `deny <reason>` on stdout. Anything it prints on
stderr reaches the operator while they wait. After `admit` it keeps running;
Effigy stops it (TERM) when the run ends, however the run ends. Hold your slot
for as long as the hook lives:

```sh
#!/bin/sh
# heavy-slot: take a host slot, hold it until Effigy stops this process.
slot=$(my-scheduler acquire --request "$EFFIGY_ADMISSION_REQUEST") || {
  echo "deny no capacity"; exit 0; }
trap 'my-scheduler release "$slot"; exit 0' TERM
echo admit
while :; do sleep 1; done
```

Effigy waits up to `EFFIGY_ADMISSION_HOOK_TIMEOUT_MS` (default one hour).
Denial, a timeout, a crash or a malformed answer refuses the run: its command
never starts, and `--json` errors carry `effigy.admission.v1` details.

Nested heavy invocations inside an admitted run do not call the hook again:
children inherit `EFFIGY_ADMITTED`, and QA-group members run inside the
group's one admission.

## Report custody back to the hook (protocol v2)

An orchestrator that must settle a run whose Effigy process died needs more
than the hook's own PID. Setting `EFFIGY_ADMISSION_PROTOCOL=2` next to the hook
opts that launcher in to a second protocol; v1 consumers are untouched, since
without the variable the request, stdin and behaviour are exactly what they
were. Any other value refuses the run, naming the variable.

v2 is for foreground runs, because its first report names the memory guard
that keeps a foreground run's custody. A durable run (`--run-id`) refuses v2
before the hook starts, naming the variable: unset it for those runs; they
already record their own run row.

Under v2 the request is `effigy.admission-request.v2` (every v1 field plus
`"protocol": 2`), and Effigy sends newline-delimited
`effigy.admission-event.v1` reports to the hook's **stdin**. Each report must
be acknowledged with `ack <seq>` on stdout within 10 seconds:

| Report | Carries | When |
| --- | --- | --- |
| `custodians` (seq 1) | the owner's and the guard's `ProcessIdentity` | after `admit`, before any workload |
| `workload` | the group's real `pgid` and its leader `ProcessIdentity` | before that group's first instruction runs |
| `terminal` | outcome, exit code or signal, cleanup, group count | at settle, before the hook is stopped |

```sh
#!/bin/sh
# heavy-slot for an orchestrator that settles dead runs itself.
slot=$(my-scheduler acquire --request "$EFFIGY_ADMISSION_REQUEST") || {
  echo "deny no capacity"; exit 0; }
trap 'my-scheduler release "$slot"; exit 0' TERM
echo admit
while IFS= read -r event; do
  seq=$(printf '%s\n' "$event" | sed 's/.*"seq":\([0-9]*\).*/\1/')
  my-scheduler record --slot "$slot" --event "$event"   # durably
  echo "ack $seq"                                       # only after recording
done
```

What the acknowledged sequence proves, even if Effigy and its guard are killed:

- The acknowledged seqs are always a gap-free prefix `1..n`, because Effigy
  never writes event n+1 before n is acknowledged.
- A workload group's `go` is sent only after its own report is acknowledged,
  so a group whose report was never acknowledged never runs. A child already
  spawned is held at the barrier and exits without executing.
- `custodians` plus two `workload` reports with no gaps means exactly those
  custodians and groups existed; `custodians` alone means no workload group
  ever started (a legitimate zero). No `custodians` means nothing is known.
- `workload_groups` in the terminal report is only a cross-check.

A report that is not acknowledged — a write failure, EOF, a malformed or
out-of-order `ack`, or the timeout — stops the workload before it runs and
fails the run closed with the usual `effigy.admission.v1` `failed` error.

After a normal v2 settle, the run also leaves one terminal row in its checkout
journal, so `run status` still answers once the process is gone:

```sh
effigy --json run status agqvltk3r7jktaa6vi23dpcu4a
# "evidence_source": "foreground-terminal-record.v1",
# "checkout_id": "4:6f0106cdfc7327d913ccf9dd28c91c7d", "boot": "..."
```

That record is evidence for your own lease, not an Effigy authority: it holds
no PID, PGID or start identity, `run logs` reports no retained log and
`run stop` refuses. Effigy never recovers or signals from it, and a provider
release on this evidence is never shown as `cleanup: "complete"`. The release
rule and its accepted risk are contract 049's.

## Name and control runs

```sh
effigy --run-id ci-1234 qa:ci:fast        # start, or attach if ci-1234 exists
effigy runs                               # this checkout's runs
effigy run status ci-1234                 # by key or run id
effigy run logs ci-1234 --follow          # output until it settles, then its exit code
effigy run stop ci-1234                   # stop it and its whole process group
```

A repeat of `--run-id ci-1234` never starts a second run: it attaches to the
first and exits with that run's result, even when the run already finished.

A run started with `--run-id` logs its output, redacted, unless both stdout
and stderr were terminals; other runs keep no log. A follower that
loses its connection resumes exactly where it stopped:

```sh
effigy run logs ci-1234 --follow --stdout-offset 5120 --stderr-offset 96
```

The offsets are the bytes you already received on each stream. In `--json`
mode, `run logs` returns base64 data and each stream's `next_offset` to poll
with. Bytes older than the kept log are reported as expired, never skipped.
If the log is gone, `run logs --follow` still exits with the run's real
result from the journal; only an unreadable journal reports `unknown`.

## Migration note for Queue

Queue workers used the host-run client: `--host-run-request-id`, parent
tokens, `effigy tasks request status|follow`, and the scheduler socket. Those
are gone, and the old flag and command refuse with a pointer here. To keep
host-wide capacity control, Queue:

1. Sets `EFFIGY_ADMISSION_HOOK` in each worker's environment to a small
   command that asks Queue's scheduler for a slot, prints `admit` once it has
   one (or `deny <reason>`), and holds the slot until it receives TERM. The
   request's `run_id` correlates the slot with the Effigy run.
2. Replaces `--host-run-request-id <uuid>` with `--run-id <key>`. Keys are
   1–128 of `A–Z a–z 0–9 . _ : -`; use the Queue task or attempt id. A
   retried submission with the same key attaches instead of running twice.
3. Replaces `tasks request status|follow` with `run status <key>` and
   `run logs <key> --follow`, resuming a lost stream with the offsets already
   received. Exit codes are the task's own; 75 now means only that the task's
   gate is held by another live run.
4. Stops setting `HOST_RUN_TOKEN`, `HOST_RUN_ID`, `EFFIGY_HOST_RUN_ROOT`,
   `EFFIGY_HOST_SCHEDULER` and `EFFIGY_SCHEDULER_OVERRIDE`; Effigy ignores
   them.

Heavy selectors need no scheduler: with no hook set they run directly, on CI
runners too.


## Runtime migration state

v0.15 imports nothing (contract 052 §11), so there is no migration command to
run and no state to report. A retired `$HOME/.effigy/state/kernel.db`, a
cutover manifest and a v0.14.1 stack's lifecycle markers are all ignored: never
read, never changed, never deleted, and never a refusal. Host-wide admission
capacity is unaffected: legacy state is not a machine-wide gate.

