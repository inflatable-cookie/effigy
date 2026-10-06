# 049 - Heavy Validation Admission Contract

Owner: Platform
Last Updated: 2026-10-04

Heavy Effigy validation runs through the Queue/Nucleus host-run scheduler.
Queue and Nucleus own admission, capacity, fairness, scheduling, run identity,
and settlement. Effigy resolves selectors, executes the launched invocation,
preserves output and exit results, handles nested calls, and reports owned
container facts. Effigy no longer implements a heavy-admission store or a
second scheduler.

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
