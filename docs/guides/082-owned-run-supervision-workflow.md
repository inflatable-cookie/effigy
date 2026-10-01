# Owned Run Supervision Workflow

Status: proposed. Run-control commands and signal evidence are not available
yet. Until an implementation brief lands, use selector status
(`effigy tasks status`), managed-session controls, and `effigy admission`
queries; do not claim run-scoped stop or interruption attribution.

Shipped narrow subset: with `EFFIGY_HOST_SCHEDULER=1`, Ctrl-C or SIGTERM on a
heavy `effigy` run asks the host-run scheduler to cancel that run and then
reports how it settled (see [080](080-host-wide-validation-admission.md#opt-in-host-scheduler-backend)).
That is not run control: there is still no stop or logs command for runs and no
hard timeout.

Contract: [052](../knowledge/contracts/052-owned-run-supervision-contract.md)
Architecture: [032](../knowledge/architecture/032-owned-run-supervision-runtime.md)
Related: [080 host-wide validation admission](080-host-wide-validation-admission.md),
[081 bounded QA groups](081-bounded-qa-groups-workflow.md)

## What this covers

One run is one Effigy invocation. A QA-group run is one run with members
inside it. Supervision answers three questions with evidence:

- is this run still the same process generation it recorded?
- can I stop this run, and did it actually stop?
- what was observed about its interruption, and what remains unknown?

It does not make a selector a run id, does not stop a process by name or PID,
and does not change `effigy qa`, selector status, or release gates.

## Current workflow (no run control)

Inspect a selector, not a run:

```sh
effigy tasks status <selector>
effigy admission status --json
effigy admission run <RUN_ID> --json
```

Managed headless sessions keep their own controls:

```sh
effigy tui status --profile <profile>
effigy tui logs --profile <profile>
effigy tui stop --profile <profile>
```

These read selector status, host capacity, or one managed session. None of
them stops an ordinary task run or a QA-group member, and none records signal
attribution.

## Proposed workflow

Once the runtime is implemented and the capability probe reports the schema,
run control is run-id scoped:

```sh
effigy tasks run capabilities --json
effigy tasks run status <RUN_ID> --json
effigy tasks run logs <RUN_ID> --follow
effigy tasks run stop <RUN_ID>
effigy tasks run stop <RUN_ID> --signal SIGKILL --grace-ms 2000 --json
```

The capability probe is read-only:

```json
{
  "schema": "effigy.run-capabilities.v1",
  "platform": "macos",
  "supervision": { "inline": true, "detached": true },
  "stop": true,
  "hard_timeout": true,
  "logs": true,
  "journal_schema": "effigy.run-journal.v2"
}
```

If `stop` or `hard_timeout` is false, do not declare a run that depends on it.
The QA-group plan rejects an unsupported `hard_timeout_ms` or an
unsupervisable member route before execution; fix the route or ask the planner
rather than starting a run that cannot be stopped.

### Reading a stop

`stop` returns the request and the proof:

```json
{
  "schema": "effigy.run-stop.v1",
  "run_id": "<RUN_ID>",
  "accepted": true,
  "request_source": "operator",
  "signal": "SIGTERM",
  "grace_ms": 5000,
  "outcome": "cancelled",
  "confirmed_gone": true,
  "groups": [{ "pgid": 1240, "state": "gone" }]
}
```

`confirmed_gone` is `true`, `false`, or `null`. `null` means unproven. Never
read it as "gone", and never treat `outcome` as a check result. A refused stop
(`refused_foreign_checkout`, `refused_unauthorized`,
`refused_untrusted_journal`, `not_found`) delivered no signal.

### Reading interruption evidence

`status` distinguishes what was observed:

| `interruption` | Meaning |
| --- | --- |
| `child_signalled` | A surviving owner observed a child end by signal or proved it gone after this run signalled it. |
| `owner_terminated` | The owner's own handler recorded the `SIGTERM` it received. |
| `owner_lost` | A surviving detached witness proved the owner generation gone. Per-child state is `gone`, `live`, or `unknown`. |
| `unknown` | No surviving witness. Nothing is claimed; wait for reconciliation. |
| `none` | No interruption recorded for a live or normally finished run. |

A signal is recorded only when this run delivered it or observed it. A missing
signal is `unknown`, not a claim that nothing was killed. A run whose owner
was killed without a witness never becomes a pass.

## Operating rules

- Stop by run id, from the run's own checkout. A stop from a different
  checkout is refused; this protects sibling worktree runs on the same host.
- Never derive a target from `ps`, a process name, or a PID you found
  elsewhere. Only the recorded generation is signalable.
- A live or uncertain child is never declared gone. If `confirmed_gone` is
  `null`, surface the uncertainty and let reconciliation settle it.
- Stopping a capacity waiter cancels only the waiter. It never frees or steals
  a lease from another run.
- A hard timeout is a separate policy from expected runtime and from the
  admission deadline. Over-budget time is evidence, not a timeout.
- Logs can be incomplete (`complete = false`). A partial log is never proof of
  a complete run.

## QA group example

A worker stops a bounded group run whose active member is stuck:

```sh
effigy tasks run status <GROUP_RUN_ID> --json
effigy tasks run stop <GROUP_RUN_ID> --json
effigy tasks run logs <GROUP_RUN_ID> --follow
```

The group run stops the active member generation, marks later members
`not_started`, and keeps the evidence of which member was active. It does not
synthesize a pass for unrun members and does not change the group definition.

A group that declares `hard_timeout_ms` runs only when the capability probe
and every resolved member route support closure proof. Otherwise the plan
fails before execution with `unsupported_run_control`.

## Implementation status

Every command and field on this page is proposed and unavailable. This guide
does not enable a command. Implementation cuts, acceptance cases, and the
non-negotiable `unknown`/never-kill rules live in contract
[052](../knowledge/contracts/052-owned-run-supervision-contract.md).
