# 049 - Heavy Validation Admission Contract

Owner: Platform
Last Updated: 2026-10-04

Heavy Effigy validation runs through the Queue/Nucleus host-run scheduler.
Queue and Nucleus own admission, capacity, fairness, scheduling, run identity,
and settlement. Effigy resolves selectors, executes the launched invocation,
preserves output and exit results, handles nested calls, and reports owned
container facts. Effigy no longer implements a heavy-admission store or a
second scheduler.

Kernel supersession (2026-10-18): the runtime kernel contract
[052](052-runtime-kernel-contract.md) supersedes this contract's lifecycle and
ownership sections: process groups and signal forwarding inside launched
runs, container execution boundary ownership, and custody of owned
containers whose facts are reported. Scheduler routing and protocol behavior
stay here. What remains of heavy-task handling is open (Q-006).

## Scheduler ownership ruling

Tom ruled on 2026-09-30: "The QA lease we added to Effigy should have been a
Queue feature - Effigy should be the runtime that executes QA, Queue and Nucleus
should manage who runs what when."

Tom's 2026-10-02 ruling (decision `219dc10a`) approves the coordinated
retirement after Queue removes its legacy admission joins and that change is
live. Queue PR 211 was reviewed at `c60d2333f6d1f4b7d056a142e31dafae638eff42`,
merged as `42accd88473572ee192dde7439e1098547028b1e`, and the planner confirmed
the prerequisite live source `d421820` contains that merge. The planner recorded
live build `1ec45a12ce4f8a91` at 08:52:01Z. The subsequent prerequisite build
`2d9d4b1faeb17fcf`, PID `89978`, has been live since 08:59:09Z; its scheduler
socket connection succeeded at 08:59:51Z. This evidence resolves the
prerequisite for removing Effigy's former store and joins. The
planner owns the newer-source Queue milestone and any later gated local-channel
refresh; this contract does not authorize a worker install.

The retirement removes Effigy's heavy lease coordinator, status/run/runs
queries, and store writers. It does not delete or migrate files. The former
default directory was `~/.cache/effigy/admission`; `EFFIGY_ADMISSION_DIR` could
select another root. Files such as `state.json` and `state.lock` remain as-is.
The old writer encoded JSON state with schema `effigy.admission.state.v1`,
`schema_version: 1`; old query payloads used
`effigy.admission.status.v1`, `effigy.admission.run.v1`, and
`effigy.admission.runs.v1`. These are historical format facts, not schemas the
current binary reads, writes, validates, or migrates. Treat existing files as
opaque historical data. The prior `48183cf` local-channel binary was retained
in the planner's machine-local rollout receipt as rollback evidence for that
host refresh and supports the former explicit-zero setting. It is not a
published release asset or a general consumer rollback guarantee. No new
history reader or migration product is introduced.

Host-container lease reapers and other independent leases are not heavy-run
admission and remain unchanged. Container execution continues to strip
`HOST_RUN_TOKEN` and `HOST_RUN_ID` at container and `sudo` boundaries.

## Verified local-channel retirement rollout

Under ruling `219dc10a`, the planner refreshed the host, ARM64 Linux artifact
and canonical installed skill on 2026-10-04 to `v0.13.1+local.47078933`, from
exact reviewed source `47078933e81aca0c5e1477b8107ffd36339183ad`.
Combined Queue milestone `55d6e801-afa1-4393-af4a-43d97bb79309` passed at
that source with parent-token reuse, no legacy lease or escaped descendant,
and automatic capacity release.

The live follower proof `f972e238…` survived one supported Queue roll with
an ENOENT gap of 1791 ms inside contract 010's 5 s window. Authority PID
changed while epoch 1 and the signing key stayed unchanged. The follower
received all 323 output markers exactly once; nested invocations before and
after the roll reused the original run. Queue recorded one original
submission, launch and passed settlement, with automatic capacity release.
The bounded cancellation and installed nested-call proofs also settled
cleanly, without escaped descendants or containers.

Private host and native ARM64 Linux smokes passed. The new binary rejects
explicit zero before effects; the backed-up prior binary passed a private
legacy rollback proof. Historical admission files were hash-verified
unchanged. Channel and skill backups, hashes, proof outputs and rollback
instructions are in
`~/.cache/effigy/rollbacks/retirement-47078933-bvk4csh1/rollout-receipt.json`.
This was a local-channel refresh, with no release, global environment change,
VM start, forced process restart or live cleanup. Later source-only catalog
imports are not silently included in this pinned rollout.

## Scheduler routing

### Container execution boundary investigation

The operator-authorized Acowtancy feedback on 2026-10-11 requests supported
recovery for a Linux workspace invocation that failed with missing local
host-run state. The Mac scheduler is currently reachable. That error does not
establish that Mac state disappeared, or that allocation-only port reclaim
caused it. Earlier container execution is not proof that every nested heavy
selector has a supported route.

Recovery work must first reproduce the actual selector and execution path
with bounded private evidence. Prefer retaining selector resolution, parent
validation and scheduling on the host, executing only the existing resolved
container payload under the owned run. This is authority to diagnose and
correct that execution boundary, not to delegate admission to Linux, forward
parent credentials, trust a handoff environment flag, copy host state, add a
second scheduler or change frozen protocol 010. If a supported correction
requires new cross-kernel authentication authority, stop with the exact
proposal. A stale reusable Linux artifact is a separate compatibility issue;
an update alone is not a proved scheduler recovery.

The reproduced failure boundary was selector handoff. The host had already
resolved and admitted a nested selector under its authenticated parent, but
the standard container path selected `Handoff` whenever the container reported
an installed Effigy binary. The handoff intentionally removed scheduler token
and run ID before invoking that binary. Linux Effigy then attempted to open its
own local scheduler state and failed closed when it was unavailable. The
`determine_strategy` policy was the same in the retained `0.13.1` inner-source
identity and the current source, so refreshing that artifact alone did not
correct this path.

For a standard direct-command task whose resolved execution binding targets a
container, an authenticated host parent now keeps execution on the host: it
passes the already-resolved command, mapped working directory, task arguments
and environment through the existing raw container exec route. Selector
resolution, parent verification, lock ownership, task status/cache handling,
nested composition, and result settlement stay with host Effigy. This applies
to heavy nested tasks and light container tasks running under the same verified
parent. It does not convert task sequences or arbitrary container commands.
Scheduler credentials remain excluded from container and `sudo` environments.

Direct heavy invocation inside a container still requires supported scheduler
authority and fails closed without it. `EFFIGY_INTERNAL_CONTAINER_HANDOFF` is
only a recursion marker; it never validates a parent. Invalid tokens still
refuse before task effects. The maintained production-path regression uses a
private Queue fixture and a fake container backend with a reported stale inner
Effigy version. It verifies routing, mapped cwd and task env, heavy/light lock
contention, host-only sibling composition, one host admission, credential
exclusion, and refusal controls. This is host/fake-backend evidence, not Linux
kernel trust qualification; a later disposable consumer proof is still needed
for genuine cross-kernel behavior and any broader selector shape.

### Caller-controlled validation recovery authority

Tom selected "Extend the Effigy client (Recommended)" on 2026-10-08,
Nucleus decision `decision-8be0903e0a00fa4494455fff944b3e5a`, recorded in
Nucleus contract 007 at `0481460`. This authorizes a bounded Effigy client
extension for a caller-persisted request identity established before the first
possible scheduler submission, and authenticated exact-identity recovery
after client restart. It does not authorize a second scheduler client in
Nucleus or change frozen scheduler contract 010.

A missing candidate file is not proof that no submission occurred. Lost,
unreadable or ambiguous replies remain held; only authenticated `unknown_run`
permits same-key submission. Preserve selector resolution, budgeting,
classification, priority, both deadlines, endpoint/peer trust, environment
projection, authenticated result and settlement binding, and credential
secrecy. No raw-command bypass, Queue settings/database change, scheduler
authority or settlement-policy change, or release is included. The supported
CLI/API shape must be documented and independently reviewed before Nucleus
integrates it; implementation is not asserted by this ruling.

### Supported durable request identity surface

For a top-level heavy selector, callers may provide a UUID before the selector:

```sh
EFFIGY_CALLER=northstar-worker effigy --host-run-request-id "$REQUEST_ID" qa:ci:fast
effigy tasks request status --caller northstar-worker --request-id "$REQUEST_ID" --json
effigy tasks request follow --caller northstar-worker --request-id "$REQUEST_ID" --json
```

Before starting Effigy, the caller persists the caller label and UUID together
with its own effect/head/base/candidate/settings binding. `EFFIGY_CALLER` is
the existing caller label; when it is unset, Effigy uses its existing default
caller identity. The request UUID is scheduler metadata and is removed from
the launched argv. The normal generated-UUID path remains unchanged.

The Rust client exposes `HostRunClient::request_status(caller, request_id)`.
It returns `Ok(Some(status))` only for a status body with a nonempty run ID,
nonempty state, and an epoch equal to the current trusted authority; `Ok(None)`
is returned only for the authenticated `unknown_run` wire response, and every
other reply or transport condition is an error. Callers follow a found request
with the existing `HostRunClient::attach(run_id, stdout_offset, stderr_offset)`
API. The lookup uses the same trusted endpoint, peer checks, and protocol v1
client as selector submission.

The CLI status success payload uses
`effigy.host_run.request-status.v1`; follow uses
`effigy.host_run.request-follow.v1`. An authenticated absence is a JSON error
with `error.details.state = "authenticated_absence"` and process exit 3. Held
ambiguity uses `error.details.state = "held"` and exit 75. Only the former
allows a caller to retry the same request UUID. `follow --json` keeps the JSON
envelope on stdout and relays captured run output to stderr; its exit code is
the launched run's real status. Text follow relays stdout and stderr to their
matching streams. Expired output remains identified as a gap.

The UUID flag is supported only for a top-level heavy selector request. Light,
planned, nested, managed-control, and audited override routes refuse it before
task effects; nested parent-token validation still occurs first. The caller
remains responsible for its body/effect binding:
Effigy does not persist candidate files or selector settings and missing local
files never establish scheduler absence. A same-key/different-body scheduler
conflict remains an error, while an unreadable or malformed reply stays held.

- Unset or `EFFIGY_HOST_SCHEDULER=1` routes selected heavy work through the
  host-run scheduler. `EFFIGY_HOST_SCHEDULER=0` is retired and exits 2 with an
  explicit unsupported diagnostic before task effects; it never runs heavy
  work directly and never silently selects the scheduler. Other values also
  fail before effects. Light work, discovery, and `--plan` keep their existing
  direct, non-admission behavior.
- Parent-token validation remains first. A present token is verified before
  routing settings or task effects. Invalid authority exits 77
  `invalid_parent_token`; a valid heavy parent lets nested work run in place
  and report a `nested` fact without a second submit. Token-verification
  unavailability fails closed with exit 75.
- Classification is unchanged: `qa`, `ci`, `ci:fresh`, tasks declaring
  `admission = "heavy"`, and QA groups with a heavy member use heavy routing.
  `admission = "heavy"` remains manifest classification metadata; it is not a
  local lease request.
- Effigy resolves and preflights before submission. The scheduler receives the
  absolute executable, original arguments, canonical working directory, and
  caller environment with scheduler-owned routing and token variables
  removed. Submission is idempotent across an ambiguous reply. The scheduler
  owns the launched process group and settlement.
- Heavy work with an unavailable or untrusted scheduler fails closed with exit
  75 and `scheduler_unreachable`. There is no automatic fallback. The existing
  `EFFIGY_SCHEDULER_OVERRIDE=<reason>` remains the explicit audited operator
  override: its nonempty bounded reason is durably reported before direct
  execution. It grants no new authority and cannot bypass a present invalid
  parent token or retired `EFFIGY_HOST_SCHEDULER=0`.
- Capacity wait and run deadlines are distinct. `EFFIGY_ADMISSION_TIMEOUT_SECS`
  sets the scheduler capacity wait (default 30 minutes);
  `EFFIGY_HOST_SCHEDULER_RUN_TIMEOUT_SECS` sets the run deadline (default two
  hours). `EFFIGY_ADMISSION_CPU_UNITS`, `EFFIGY_ADMISSION_MEMORY_MIB`,
  `EFFIGY_ADMISSION_CPU_BUDGET`, and
  `EFFIGY_ADMISSION_MEMORY_BUDGET_MIB` remain request/fallback settings used
  for scheduler capacity hints, not a local admission budget. The scheduler
  root defaults to `~/.local/state/host-run`; isolated fixtures may set
  `EFFIGY_HOST_RUN_ROOT`.
- Output is relayed without duplicate bytes and the launched child's real exit
  status is preserved. Capacity timeout, prelaunch cancellation, run timeout,
  lost result, signalled child, and output expiry remain distinct and never
  turn a non-pass into success. Parent SIGINT, SIGTERM, or SIGHUP asks the
  scheduler to cancel once and follows its settlement. Inside a launched run,
  Effigy forwards termination only to process groups its tasks started and
  completes their normal cleanup.
- A heavy QA group is submitted as one scheduler run. The launched child owns
  the group ledger; nested heavy members reuse its validated token. Group run
  records report scheduler backend, run ID, epoch, queue wait, and prelaunch
  settlement when available. `admission_wait_ms` is the scheduler wait
  measurement, not an Effigy lease wait. Light groups have no scheduler
  backend object. Group members remain serial and `needs_planner` stops before
  submission.
- Owned containers started and removed by an Effigy task report scheduler
  facts with run ID, epoch, and stable fact UUID. Removal is `true` only after
  observed successful teardown, `false` after observed failure, and `unknown`
  when not observed. Facts persist and replay until acknowledged; no closure is
  invented for an unobserved container.

The client follows Nucleus [contract 010, Client protocol v1](https://github.com/inflatable-cookie/nucleus/blob/16fcb59cff96de581f4cb3141b4927d6ab53b4e5/docs/knowledge/contracts/010-host-run-scheduling.md#client-protocol-v1).
Its wire contract pins submit/attach/status/cancel/report, parent-token
verification, settlement envelopes, and durable container facts. The Rust
client retains descriptor-relative trust discovery, Unix peer proof, bounded
NDJSON calls, submit ambiguity recovery, attach offsets, and report-fact replay.
Token encodings and claims follow the pinned protocol; unsupported or invalid
authority fails closed.

The peer start identity is Linux boot ID plus process start ticks (proc stat
field 22, parsed after the command name) or macOS kernel process start time in
UTC whole seconds. Unknown identities never prove a match; only whole-second
macOS `.000Z` timestamps are equivalent to `Z`, and other fractional values
refuse proof. The documented same-second macOS PID-reuse residual remains.

After an attach is interrupted, recovery has a five-second monotonic budget
with capped backoff and retries only endpoint absence/refusal or transport
closure. Each connection revalidates authority, peer identity, epoch, and token
keys, checks status for the same run, and resumes both streams at their exact
delivered byte offsets. Status/attach responses without offset progress do not
reset the budget; advancing output and `output_expired` offsets do. Interrupt
cancellation retries against the same run through the bounded roll. Recovery
never resubmits: trust/protocol failures fail closed, and exhaustion exits 75
with final run state unknown.

## Selection and ownership

The resolved task selectors `qa`, `ci`, and `ci:fresh` are heavy even when a
repository omits metadata. A task may also declare `admission = "heavy"` in a
manifest table. Shorthand tasks can be converted to tables without changing
their run steps; unknown admission values fail manifest validation. Effigy's
own `qa:ci:fast` and `qa:ci:local` are marked heavy. `effigy test` is not
implicitly heavy in isolation; its repository may provide a heavy wrapper.
`--plan` and read-only task discovery never submit a run.

The selector is resolved and preflighted before the scheduler is contacted.
Heavy group submission includes no member effects before launch. Nested heavy
work validates and reuses its scheduler parent token. A separate top-level
invocation requests its own scheduler run. Caller labels, flags, and removed
lease credentials are never parent authority.

## Evidence and validation mapping

The private Queue conformance server must come from the reviewed `e9e4d12`
baseline or a verified descendant and run with an isolated state directory;
tests never contact the live endpoint. Required proofs cover heavy pass and
failure, graceful cancellation and child closure, nested token reuse, scheduler
unreachable exit 75, bad-parent exit 77, light direct execution, retired-zero
rejection before effects, and host-container token exclusion. The former
Effigy-only store scheduling, recovery, RSS, query, and run-join assertions are
retired with that backend; they are not replaced with another Effigy store or
reader. Group metadata/plan tests continue to prove classification and
preflight, while scheduler integration owns admission and cancellation
evidence.

Installation remains planner-owned after independent exact-head review, CI,
the newer-source Queue milestone, preservation of historical files and
channel backups, and bounded smoke evidence. The retained `48183cf` executable
is local rollout evidence, not a published consumer rollback target or general
release guarantee. Host-local rollback follows its planner-owned receipt; no
data cleanup or migration is part of rollback or retirement.

Run stop/logs commands, group `hard_timeout_ms`, workflow changes, releases,
automatic VM starts, forced restarts, and live cleanup remain outside this
contract.

## Workspace fixture admission qualification

`test:workspace:artifact-parent` qualifies the complete maintained
`test:workspace:artifact-freshness` composition and the executable catalog-handoff
route/refusal matrix under real private scheduler admissions, one per maintained
member. The integration
harness walks the actual task manifest; a composition check refuses membership
drift. Catalog pack/parser/selection/session tests and bounded process-reader
tests retain the full ordinary catalog-handoff covering. Those unit helpers do
not enter the scheduler; shared workspace/exec cases also run in the artifact
parent members. The harness retains exact compiled nextest membership, each
terminal case result, command exit codes and the private settlement under a unique
`.effigy/reports/fixture-family/workspace-*` directory. Each member stays within
the existing test deadline; the parent selector runs all members without
fail-fast. An absent member, incomplete result set or any failure prevents
success. Assertion failures do not stop later independent members. Unavailable
runners and incomplete result sets fail qualification; nextest retains its
existing forced-exit, timeout and leak policy. Zero retries remain unchanged. Run ordinary covering selectors first to compile their
constituents, including the fresh CLI prerequisite used by managed helpers.

Fixture placement follows the actual execution boundary:

| Maintained member | Fixture ownership and admission boundary |
| --- | --- |
| Artifact provisioning | Artifact/cache/identity files and private fake builder executables; fake executables do not re-enter the scheduler. Invalid-output and expiry oracles remain refusing. |
| Artifact admission | Rendered Compose labels and policy drift fixtures; no task entrypoint. External roots remain valid inspection inputs. |
| Artifact activation | Nested catalog tasks use the existing canonical checkout-contained workspace owner. Runtime-preparation, exec transport and mount probes use private fake operations; outside mount/sibling inputs remain negative oracles. |
| Artifact builder | Direct-builder Rhai tasks and the shared container-helper task enter the real task runner. Their fresh canonical roots are checkout-contained and removed by their existing owners. Full-ID replacement, foreign siblings and ownership refusals remain asserted. |
| Owner integrations and diagnostics | Fake secret/exec/backend and permission/doctor probes retain ordinary coverage; runnable deferral callers use the checkout-contained workspace owner. |
| Managed callers | All stream and TUI case-table helpers use the existing workspace owner. The container-bound Rhai setup also uses that owner before real task entry; canonicalization alone does not admit an external directory. |
| Gateway callers | All four managed-host tests own checkout-contained temporary roots, including their disposable Git worktrees, listeners, gateway state and foreign sentinels. Host CLI calls preserve the inherited scheduler state path across isolated `HOME` changes. The existing `EFFIGY_TEST_CONTAINERS_HOME` seam selects that same owned home for test-support binaries. |
| CLI callers | The partial-startup family uses its existing checkout-contained owner and exercises actual CLI entry with private transports. |
| Catalog handoff | Selection/pack/parser/session/exec tests retain their complete ordinary covering. The Python route/refusal matrix owns a checkout-contained temporary root and preserves inherited host authority and its original state path. The existing test-support home seam follows each host/guest `HOME`; actual guest transport continues to strip host credentials. |

The catalog fake engine keeps the host client's authority on the host side.
Its simulated guest starts from private adapter controls and the declared exec
environment; it rejects forwarded parent credentials. Copying the entire host
client environment into a fake guest does not model a container boundary.

The separate forged, expired and outside-root token control still validates
real refusal before effects. Moving positive execution fixtures never widens a
parent token, changes admission policy, or converts an outside-root negative
oracle into a positive one. These proofs qualify private fixtures only; they
do not publish or qualify an operational Linux artifact or consumer stack.
