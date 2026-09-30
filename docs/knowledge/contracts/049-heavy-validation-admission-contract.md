# 049 - Heavy Validation Admission Contract

Owner: Platform
Last Updated: 2026-09-30

Heavy Effigy validation shares one host-wide admission budget. This contract
covers invocations started by people, workers, and orchestrators on the same
machine. The current mechanism is Effigy-owned: Queue supplies caller identity
and observes Effigy's state. The scheduler cutover below replaces that ownership;
it is not implemented yet.

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

An operator-approved recovery of an exact orphan record is an exception under
the current mechanism, not permission to weaken automatic reclamation. Preserve
the locked before-state, closure evidence, authorization and resulting record;
change no other runs. A queue-wait deadline is not a running lease's expiry.

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
