# Host-wide validation admission

Queue and Nucleus schedule heavy Effigy work through the host-run scheduler.
Effigy selects and executes work; it no longer keeps a host-wide heavy-run
lease store or admission query commands.

The built-in selectors `qa`, `ci`, and `ci:fresh` are heavy by default. A repo
can also mark a task table:

```toml
[tasks."qa:ci:fast"]
admission = "heavy"
run = ["cargo test --workspace"]
```

`admission = "heavy"` classifies the task for scheduler routing. It does not
request a local Effigy lease. Unknown values fail manifest parsing.

## Routing and prerequisites

Unset or `EFFIGY_HOST_SCHEDULER=1` routes heavy work through the scheduler.
`EFFIGY_HOST_SCHEDULER=0` is retired: the command exits 2 with an unsupported
setting diagnostic before task effects. It never runs heavy work unadmitted or
silently chooses a backend. Other values are rejected. Light tasks, discovery,
and `--plan` keep their direct behavior.

Effigy resolves the selector and completes preflight before submission. A
present `HOST_RUN_TOKEN` is verified before routing settings or task effects.
Invalid parent authority exits 77 and is never queued. A valid parent lets
nested heavy calls reuse the scheduler run in place. Tokens and run IDs are
removed at container and `sudo` boundaries.

Heavy work with an unavailable or untrusted scheduler fails closed with exit
75 and `scheduler_unreachable`. There is no automatic fallback. The existing
`EFFIGY_SCHEDULER_OVERRIDE="<reason>"` is the explicit audited operator
override. It records a bounded reason before direct execution; it does not
broaden authority and cannot bypass invalid parent tokens or the retired zero
setting.

### Container tasks under an admitted host run

When a selector runs under a verified host parent, Effigy keeps selector
resolution and admission on the host. For a standard direct-command task whose
resolved binding targets a container, the host executes the resolved command
through the existing mapped container exec route. It does not re-enter Effigy
inside the container. Host Effigy keeps the task lock, status/cache handling,
nested composition, and result settlement. Heavy nested selectors and light
container tasks both validate a present parent before using this route.

The host passes the mapped working directory, resolved command, selector
arguments, and task environment. `HOST_RUN_TOKEN` and `HOST_RUN_ID` stay out of
container and `sudo` environments. This route does not cover task sequences or
authorize a selector launched directly inside a container. Direct heavy work
without a valid parent still needs scheduler authority and fails closed;
`EFFIGY_INTERNAL_CONTAINER_HANDOFF=1` is not authority. A fake-backend test
covers this host boundary with a stale reported container Effigy version. It
does not qualify Linux kernel trust or replace a later disposable consumer
proof.

## Waiting and execution

The scheduler runs the submitted executable, arguments, working directory,
and caller environment. Output streams as normal and the launched child's exit
status and JSON envelope are preserved. A heavy QA group is submitted whole;
nested heavy members reuse that scheduler run, and the group ledger records the
scheduler run ID, epoch, queue wait, and settlement where available.

`EFFIGY_ADMISSION_TIMEOUT_SECS` sets the scheduler's capacity wait, 30 minutes
by default. `EFFIGY_HOST_SCHEDULER_RUN_TIMEOUT_SECS` sets the separate run
deadline, two hours by default. The names below remain supported as scheduler
request or capacity-hint fallbacks; they do not configure local admission:

- `EFFIGY_ADMISSION_CPU_UNITS`
- `EFFIGY_ADMISSION_MEMORY_MIB`
- `EFFIGY_ADMISSION_CPU_BUDGET`
- `EFFIGY_ADMISSION_MEMORY_BUDGET_MIB`

`capacity_timeout` means the run never launched. Prelaunch cancellation, run
timeout, lost result, signalled child, and output expiry remain distinguishable
and never report success. Ctrl-C, SIGTERM, or SIGHUP asks the scheduler to
cancel its run once and follows settlement. Inside a launched run, Effigy
forwards termination only to process groups its own tasks started, then lets
the task's normal cleanup complete.

Container start/removal facts use the scheduler run identity and replay until
acknowledged. A removed fact is `true` only after observed successful teardown,
`false` after observed failure, and `unknown` when teardown could not be
observed. Other host-container leases and their reapers are independent and
remain unchanged.

## Private binary fixtures

The JSON contract checker owns its generated repositories for one invocation.
Skill consumers and QA-group repositories execute below the selected checkout's
ignored `.effigy/tmp/contract-fixtures` root; inspection, dependency dry-run and
literal-cwd fixtures retain their external temporary roots. Fixture paths are
substituted into parsed arguments without interpreting their spaces or quotes.
The genuine QA-group run record stays available to every selected status probe;
its setup result is cached only for that invocation, including a refusal.
Generated repositories are removed on return or unwind. Parent verification
still applies to light tasks and group members.

`test:audit:contracts` checks the whole maintained contract index in both fast
and full modes under a genuine private parent, plus deliberate outside-root and
no-parent controls. It runs only against the reviewed private Queue fixture;
it does not grant access to the live scheduler or qualify a consumer runtime.

A command that launches a nested task from its selected repository needs that
execution root inside its authenticated parent root. Changing only `--repo`
does not establish parent authority. Discovery-only fixtures may use a separate
root; negative fixtures deliberately retain outside roots and must refuse
before task effects.

A fixture that isolates `HOME` must preserve the actual inherited
`EFFIGY_HOST_RUN_ROOT`, or resolve its original canonical state path before
changing `HOME`. An empty fixture home has no trusted scheduler state. Copying
credentials, stripping the parent token or adding an override is not an
admission repair.

The maintained `test:audit:fixture-family` selector exercises the complete CLI
integration binary, including ignored private-parent controls, against one
private authenticated parent. It treats inherited-output leaks as failures and
retains a full native diagnostic stream separately from the scheduler's output
tail. A passing family run qualifies that family, not the full repository QA.

## Caller-controlled request recovery

A caller that may restart or lose the submit reply can choose a stable UUID
before starting one heavy selector. Persist the caller label and UUID with the
caller's own effect/head/base/candidate/settings binding first, then pass the
UUID to Effigy:

```sh
EFFIGY_CALLER=northstar-worker effigy --host-run-request-id "$REQUEST_ID" qa:ci:fast
effigy tasks request status --caller northstar-worker --request-id "$REQUEST_ID" --json
effigy tasks request follow --caller northstar-worker --request-id "$REQUEST_ID" --json
```

`EFFIGY_CALLER` is the existing selector caller label. Set it to the stable
label you persisted; when unset, Effigy uses its existing default caller
identity, which the caller must persist exactly. The UUID flag is stripped from
the launched argv and does not alter the normal generated-ID path. It is
accepted only for a top-level heavy selector request. Light, `--plan`, nested,
managed-control, and scheduler-override routes refuse it before task effects.
A present parent token is still validated first.

Status and follow query the exact caller/UUID through the trusted host-run
client. A found request has a run ID and nonempty state in the current trusted
authority epoch, and can be followed after Effigy exits.
Only the scheduler's authenticated `unknown_run` response is an
`authenticated_absence` (JSON error details schema
`effigy.host_run.request-status.v1`, exit 3), which permits the caller to retry
the same UUID. Endpoint loss, untrusted or malformed responses, and missing
local candidate/result files remain held (state `held`, exit 75); they never
imply that submission did not happen. A same caller/UUID with a different
request body remains a scheduler conflict.

The JSON envelope contains these versioned result or error detail shapes:

- Status success: `schema = "effigy.host_run.request-status.v1"`,
  `schema_version = 1`, `caller`, `request_id`, `state = "found"`, and the
  scheduler's `run` status object.
- Authenticated absence: the same status schema/version, `caller`,
  `request_id`, `state = "authenticated_absence"`, and `reason = "unknown_run"`;
  process exit 3.
- Held ambiguity: the status schema/version, `caller`, `request_id`,
  `state = "held"`, and a diagnostic `reason`; process exit 75.
- Follow success: `schema = "effigy.host_run.request-follow.v1"`,
  `schema_version = 1`, `caller`, `request_id`, `state = "followed"`,
  `run_id`, `epoch`, `outcome`, `exit_code`, `settlement`, and
  `output_expired`.

Follow returns the scheduler settlement with the launched run's exit code.
Text follow relays stdout and stderr to their matching streams. With `--json`,
the JSON envelope stays on stdout and captured run output goes to stderr.
Expired scheduler output is reported as a gap and is never replaced with
guessed output.

Rust callers can use `HostRunClient::request_status(caller, request_id)` and
then the existing `attach(run_id, stdout_offset, stderr_offset)` API. That
method returns `Ok(None)` only for authenticated `unknown_run`; every other
failure is an error. See [contract 049](../knowledge/contracts/049-heavy-validation-admission-contract.md)
for the binding and recovery limits.

## Historical state and rollback

The former local store defaulted to `~/.cache/effigy/admission/state.json` and
could be rooted elsewhere with `EFFIGY_ADMISSION_DIR`. Existing directories,
`state.json`, `state.lock`, and all recorded reservations remain untouched.
The current binary does not read, write, delete, or migrate them. The old
writer used `effigy.admission.state.v1` JSON with schema version 1; treat saved
files as opaque historical records, not a current query or migration format.
The prior `48183cf` local-channel binary was retained in a planner-owned local
rollout receipt; it is not a published consumer rollback target or general
release guarantee. No installation or cleanup is performed by this change.

Maintainers prove scheduler behavior with the targeted host-run protocol and
integration selectors against the private Queue server at `e9e4d12` or a
verified descendant. The isolated test server has a throwaway state directory
and never contacts the live scheduler endpoint. Required proofs include heavy
pass/failure/cancellation, nested token reuse, unreachable exit 75, bad-parent
exit 77, light direct execution, retired-zero rejection before effects, and
host-container token exclusion.

See [contract 049](../knowledge/contracts/049-heavy-validation-admission-contract.md)
for the scheduler contract and retained-history policy. The local installation
guide describes the separately gated planner-owned refresh.
