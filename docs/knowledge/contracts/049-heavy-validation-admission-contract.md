# 049 - Heavy Validation Admission Contract

Owner: Platform
Last Updated: 2026-10-01

Heavy Effigy validation shares one host-wide admission budget. This contract
covers invocations started by people, workers, and orchestrators on the same
machine. The default mechanism is Effigy-owned: Queue supplies caller identity
and observes Effigy's state. An explicit opt-in backend (below) routes heavy
execution through the host-run scheduler; the cutover that makes it the default
and removes the lease store is not approved or implemented.

## Scheduler ownership ruling

Tom ruled on 2026-09-30: "The QA lease we added to Effigy should have been a Queue
feature - Effigy should be the runtime that executes QA, Queue and Nucleus
should manage who runs what when."

Queue and Nucleus own the shared scheduler contract: admission, capacity,
fairness, scheduling and lease recovery. Effigy executes QA and supplies the
runtime boundary the scheduler needs: clean SIGTERM handling, truthful exit
results and telemetry. The scheduler launches its Effigy run in an owned process
group. Heavy invocations started directly through Effigy will request admission
from the scheduler endpoint rather than an Effigy-owned lease store.

Freeze new implementation of Effigy's admission and lease store. Keep the
existing mechanism unchanged until the scheduler is live and the cutover is
proved. Removal follows cutover; it must not leave a period of unadmitted heavy
execution. Endpoint discovery, grants, nested execution, cancellation, failure
and compatibility semantics must be pinned to the agreed shared contract before
an implementation brief is dispatched. Queue Spec 031 owns Queue's side.

The client boundary is Nucleus
[contract 010, Client protocol v1](https://github.com/inflatable-cookie/nucleus/blob/16fcb59cff96de581f4cb3141b4927d6ab53b4e5/docs/knowledge/contracts/010-host-run-scheduling.md#client-protocol-v1).
This pin defines submit/attach/status/cancel/report, validated parent tokens,
settlement envelopes and durable container facts. Client implementation follows
the conforming Queue server; accepting the protocol does not switch execution
or authorize removal of the current store.

`crates/effigy-host-run` provides the Rust client primitives for that pin:
descriptor-relative trust discovery, Unix peer proof, bounded NDJSON calls,
submit ambiguity recovery, attach offsets, settlement validation, parent-token
verification and durable report-fact replay. Its protocol-specific start
identity is `PID@boot_id:starttime_ticks` on Linux (proc stat field 22 after
the last command-name parenthesis) and `PID@UTC-second` on macOS, read from the
kernel process start time. An unknown identity never matches. Unsupported
platforms fail closed. Selector execution uses them only through the opt-in
backend below; the existing Effigy lease store and every legacy admission and
execution path remain unchanged and remain the default until the separately
approved scheduler cutover.

The 2026-10-01 re-sign-off accepts verified traversal only for implementations
without descriptor-relative open. Its ancestor-chain recheck detects persistent
swaps; reverted swaps remain inside the declared same-uid trust boundary. The
Rust client retains `openat` traversal. This amendment changes neither the wire
nor the cutover prerequisites.

The subsequent start-identity clarification pins Linux boot ID plus process
start ticks, and macOS kernel start time in UTC whole seconds. Unknown
identities never prove a match. The transition accepts only whole-second macOS
`.000Z` values as equivalent to `Z`; other fractional values refuse proof.
The macOS same-second PID-reuse residual is explicit in the shared contract.

An operator-approved recovery of an exact orphan record is an exception under
the current mechanism, not permission to weaken automatic reclamation. Preserve
the locked before-state, closure evidence, authorization and resulting record;
change no other runs. A queue-wait deadline is not a running lease's expiry.

## Opt-in scheduler backend

Migration step (b) of the shared contract is shipped behind a setting. Step (c)
(removing the lease store) and any default activation are not.

- `EFFIGY_HOST_SCHEDULER=1` routes top-level heavy work through the scheduler.
  Unset or `0` keeps legacy lease admission, unchanged. Any other value refuses
  with exit 2 before any effect. The setting is the only switch: nothing
  detects a scheduler and switches on its own.
- Classification is the existing one: published and draft tasks resolved as
  implicit `qa`/`ci`/`ci:fresh` or explicit `admission = "heavy"`, and QA groups
  with a heavy member. Light work, `--plan` and discovery stay direct and acquire
  nothing, whether or not the scheduler is reachable.
- Effigy resolves and preflights first, so selection, scope gaps
  (`needs_planner`) and refusals happen before submit. No build, setup,
  container activation or group member runs before the scheduler launches the
  run. The client then submits the invocation it was started with: the absolute
  executable, the original arguments and canonical working directory, and the
  caller's environment without scheduler, token or legacy-lease names. The
  scheduler provides only `PATH`, `HOME` and its own run variables on top of that
  map, so a task sees the scheduler's `PATH`/`HOME`. A fresh client request ID
  makes submit idempotent; the client resolves an ambiguous reply by lookup
  before it resubmits. Capacity and run deadlines are separate: the capacity
  wait reuses `EFFIGY_ADMISSION_TIMEOUT_SECS` (default 30 minutes) and the run
  deadline is `EFFIGY_HOST_SCHEDULER_RUN_TIMEOUT_SECS` (default 2 hours). The
  reservation fallback reuses `EFFIGY_ADMISSION_CPU_UNITS` and
  `EFFIGY_ADMISSION_MEMORY_MIB`, and the run receives `CARGO_BUILD_JOBS` from it.
  The scheduler root is `~/.local/state/host-run`; `EFFIGY_HOST_RUN_ROOT` names
  an absolute private root for isolated fixtures.
- The scheduler-launched child is the same Effigy invocation with
  `HOST_RUN_TOKEN` set. A present token always takes the validation path,
  regardless of the setting: a valid token (current or immediately previous
  epoch, within its root, unexpired, MAC verified, class `heavy`) executes the
  work in place with no lease, no submit and a reported `nested` fact; a present
  token that fails validation exits 77 `invalid_parent_token` and never queues.
  Token verification that cannot reach the scheduler exits 75. Caller strings,
  flags and the legacy lease variable never substitute for a token. Tokens and
  `HOST_RUN_ID` are stripped from container exec environments and `sudo`
  commands and are never logged.
- Heavy work with an unreachable, untrusted or missing scheduler fails closed
  with exit 75 and `scheduler_unreachable`. There is no automatic legacy
  fallback. `EFFIGY_SCHEDULER_OVERRIDE=<reason>` is the explicit operator
  override: the reason is mandatory, is recorded as a durable `override` fact in
  the client's pending journal before the run (a journal failure refuses the
  run), then the work executes directly with no admission, never also taking a
  legacy lease. The journal replays until the scheduler acknowledges it. There
  is no other bypass flag.
- The parent streams stdout and stderr verbatim, with no duplicate bytes across
  a reconnect, and exits with the child's real status: the JSON envelope and
  exit code are the child's. Outcomes stay distinct: `capacity_timeout` (never
  launched, exit 1, a capacity message), cancelled before launch (exit
  128+signal after a local interrupt, else 1), run timeout (exit 124), lost
  (exit 70, result unknown, never success), and a signalled child (128+signal).
  A non-pass outcome never exits 0. Output the scheduler no longer retains
  (`output_expired`) is reported on stderr and leaves the relayed output marked
  incomplete.
- SIGINT, SIGTERM or SIGHUP on the parent asks the scheduler to cancel its own
  run once, then the parent follows the settlement; it never signals the
  scheduler's process group itself. Inside a scheduler-launched run, Effigy
  forwards termination only to the process groups its own tasks started and
  runs their cleanup. External kills are not claimed to be attributable, and a
  host process-group closure is not proof of container closure.
- Owned containers an Effigy task starts and tears down (inline workspace
  tasks) report `started` and `removed` facts with the scheduler run ID, epoch
  and a stable fact UUID. Removal is `true` only when the teardown command
  succeeded, `false` when it ran and failed, and `unknown` when it could not be
  observed. Facts persist in `pending-facts.jsonl` before sending and replay
  until acknowledged; nothing invents container closure. Container lifecycles
  outside that flow do not report yet.
- QA groups: a heavy group is submitted whole, and the launched child owns the
  ledger, so a launched run leaves exactly one record. The record adds an
  optional `backend` object (`kind`, `scheduler_run_id`, `scheduler_epoch`,
  `queue_wait_ms`, `settlement`); absent means legacy or light. `queue_wait_ms`
  comes from the scheduler's status record and is `null` when unavailable. A
  group the scheduler settles without launching (capacity timeout, prelaunch
  cancel) leaves a completed record with that outcome, `settlement` and the
  measured wait. Members stay serial and `needs_planner` still stops before
  submit. The run-record schema stays `effigy.qa-group-run.v1`: the field is
  additive and optional, so older readers ignore it and older records stay
  valid.

Unchanged and unavailable: the generic run stop/logs commands, group
`hard_timeout_ms`, and contract 052 remain unavailable. The opt-in does not
refresh any installed channel, restart anything, or change workflows.

### Backend evidence

Private-server proof used Queue's `bin/host-run-private-server.mjs` at reviewed
merge `7563a61ef3a1efdb8c6cce43cdb3ad1207cee424` (Queue PR186), run from an
isolated archive of that commit with a throwaway state directory and never the
live endpoint (`test:host-run:integration`, see guide 080). The reviewed server emits padded standard base64 keys and integer-millisecond
expiry. Contract 010 at the current pin makes these encodings normative: a key
is exactly 32 bytes in 44-character RFC 4648 standard base64 with padding; token
parts are unpadded base64url and the MAC covers the transmitted first part.
Claims contain exactly `runId`, `epoch`, `class`, `root`, and `exp`; validity is
`now_ms < exp`. Effigy re-signed this clarification on 2026-10-01 after independently
checking its key and HMAC vectors. The shipped client still accepts additional
spellings and RFC 3339 expiry; strict enforcement and server expiry-boundary
conformance are prerequisites for activation. This pin does not authorize cutover.

## Selection and ownership

- The resolved task selectors `qa`, `ci`, and `ci:fresh` are heavy even when a
  repository omits metadata. A task may also declare `admission = "heavy"` in
  its manifest table. Shorthand tasks can be converted to tables without
  changing their run steps. Unknown admission values fail manifest validation.
- Effigy's own `qa:ci:fast` and `qa:ci:local` are explicitly marked heavy.
  Other tasks remain unchanged until their owners mark them. `effigy test` is
  not implicitly heavy in isolation; its repository may provide a heavy
  wrapper. `--plan` and read-only task discovery never acquire a lease.
- Admission happens once after selection and before setup, build, container
  activation, or test execution. A nested Effigy task within that run shares
  its parent's lease, including a heavy nested task. A separate top-level
  invocation always requests its own lease. Reentry must prove the parent
  lease, not trust a forgeable environment flag alone.
- The host-wide state is outside repositories and worktrees. All Effigy
  processes on one host use the same coordinator, lock, queue, and run journal.
  State creation validates directory ownership, permissions, and symlinks;
  an unavailable or untrusted coordinator fails closed for heavy work. It
  never silently falls back to a per-repository lock. A configured shared
  location and permissions are required when several OS users run Effigy on
  one machine.

## Capacity and fairness

- The coordinator measures host logical CPU count and physical memory. Its
  default aggregate reservation budget leaves at least half of each for
  interactive work: `max(1, floor(cores / 2))` CPU units and half of physical
  memory, rounded down to MiB. It admits no more than one unconfigured heavy
  run at once: that run reserves the full budget. These are measured values,
  not fixed assumptions about Tom's machine.
- Machine settings may override the budget, reserved headroom, and per-task
  CPU and memory reservations. A reservation above the current budget is a
  configuration error, not an endless wait. Budget changes do not evict a
  running lease. The default and overrides are visible in JSON status.
- The scheduler bounds the sum of admitted reservations by both budgets.
  Within one repository, requests are FIFO. Across repositories it rotates
  the next eligible head in round-robin order; a caller identity is the key
  when repository identity is unavailable. A small request may use spare
  capacity without moving ahead of an older request in its own key.
- Admission is a reservation bound, not an OS hard cap on arbitrary child
  processes. Effigy passes granted parallelism to supported tools and reports
  measured use. It must not claim a hard memory or CPU limit on a platform
  where it cannot enforce one. The conservative one-run default protects
  headroom from *concurrent* heavy validations; a single task that exceeds
  its reservation remains visible as an overrun.

## Wait and run lifecycle

- The caller may set `EFFIGY_CALLER` to an opaque identity such as
  `queue:<taskId>:<runId>`. Effigy records it without treating it as an
  authorization token. An absent value gets a local interactive identity.
  Each invocation has a unique run ID and a persisted queue entry.
- Waiting reports a typed `waiting_for_capacity` state with position,
  fairness key, queue time, budget, and deadline. Human stderr says
  `waiting for QA capacity, position N`; JSON queries expose the same state.
  The default admission deadline is 30 minutes and is configurable separately
  from task execution timeout. Expiry returns a distinct capacity-timeout
  error, never a test failure. Existing execution timeouts begin when their
  execution phase normally begins and are not loosened by waiting.
- The admitted lease covers the complete heavy run and its children. Normal
  exit and cancellation release it. A cancelled waiter leaves the queue.
  Effigy never kills another caller's run to free capacity.
- Each lease records host boot identity, owner PID and process start identity,
  plus the supervised process group. After a crash, a lease is reclaimed only
  when the owner generation and its process group are proven gone. PID reuse,
  an unverifiable boot identity, or a live child prevents reclamation and
  produces an actionable stale/unknown status rather than unsafe admission.

## Result identity and joins

Tom ruled on 2026-09-28 to ship host-wide admission before same-input joins.
Each current invocation takes its own lease and runs every gate. Joining stays
disabled until a separate task proves an enforceable input boundary and exact
result and log replay on supported platforms. This is a delivery split, not a
weaker identity rule for future joins.

The deferred joining contract is:

- Concurrent same-input calls may share execution only when Effigy proves a
  complete input identity. The digest covers the resolved selector and args;
  tracked, modified, and untracked source bytes; dependency locks; all
  manifest includes and effective config; validator and tool versions; the
  relevant environment; execution platform; and declared external inputs.
  Ignored files or opaque scripts that can affect results must be included or
  make the proof incomplete. Git commit or tree equality alone is insufficient.
- A task opts into joining only when its complete input set and environment
  are declared and fingerprintable. Arbitrary `qa` shell commands have no
  proven identity by default and run separately. An incomplete, failed, or
  changing fingerprint disables joining rather than weakening the key.
- A joiner receives the owner's exact exit status and complete captured log,
  with its own caller/run record pointing to the owner. Joining does not
  create a second lease or rerun any gate. A log that cannot be captured
  completely disables joining before execution. Completed-result caching
  across worktrees is outside this contract.

## Telemetry and query

Each invocation retains `queued_at`, `admitted_at`, `started_at`, `ended_at`,
`queue_wait_ms`, `wall_ms`, CPU user and system time, and peak RSS when the
platform can measure it. Null means unavailable, not zero. The record also
contains caller, repository, selector, fairness key, run ID, lease, state,
budget/reservation, exit classification, and log reference when available.
Future joining adds an owner ID; a joiner must report its own wait plus that
owner ID, not invented duplicate CPU use.

`effigy admission status --json` exposes live budget, queued positions, and
leases. `effigy admission run <id> --json` returns one durable record, and
`effigy admission runs --caller <identity> --json` returns bounded, paginated
history. These read-only queries work while a validation is waiting or
running, so Queue can show capacity wait as alive and attribute time to the
right task run. JSON uses versioned schemas and typed states/errors; text and
JSON agree. Logs and telemetry redact secret values and do not echo relevant
environment contents.

## Validation

- CLI fixtures that invoke heavy selectors use private admission state inside
  their owned temporary root. They clear inherited caller, lease credentials
  and admission settings before explicitly configuring that sandbox. Merely
  removing caller identity does not isolate a fixture from host admission.
  Tom authorized this test-isolation repair on 2026-09-30; it does not change
  the frozen product admission mechanism.
- Independent Effigy processes in different checkouts contend for one budget;
  a direct human invocation cannot bypass it. Nested task refs use one lease.
- Synthetic capacities prove CPU and memory admission, headroom, FIFO within a
  repository, round-robin across repositories, and waiter cancellation.
- A stopped owner, PID reuse, live child, and stale boot identity exercise
  recovery without killing a foreign process or oversubscribing.
- Tests prove capacity wait and deadline remain distinct from validation
  failure; task coverage, gate order, exit code, and run timeout stay intact.
- For the admission release, two concurrent calls with identical apparent
  inputs still execute separately and retain distinct leases and results.
  No incomplete fingerprint enables joining.
- Queue can correlate a caller identity with a live wait and a final record
  containing separate queue wait, wall, and CPU measures.

The later joining task must prove an immutable input snapshot or equivalent
read restriction, complete process-tree log capture and replay, and the
complete-input mutation cases above before any task may opt in.
