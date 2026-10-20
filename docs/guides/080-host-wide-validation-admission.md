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
