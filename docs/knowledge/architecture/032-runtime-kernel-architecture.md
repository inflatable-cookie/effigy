# 032 - Runtime Kernel Architecture

Contract: [052](../contracts/052-runtime-kernel-contract.md)
Status: foundation, gates and store, run journal and settle, local and remote
custody, supervision, strict fakes and the QA ratchet are implemented. Executable
probes and engine adoption remain specified. This note places the kernel in the
codebase, names the processes it runs, and orders the kernel
lane's implementation cuts. Contract 052 owns every rule and primitive; this
note adds no semantics.

## Placement

One binary, three layers. Isolation comes from crate boundaries and
dependency direction, not separate binaries.

```text
 façade    CLI parsing · help · one output owner (Outcome → text or JSON)
 engines   task engine · run supervisor · environment reconciler · gateway
           delivery (release, state, deploy) · vault · deps · graph · bundles
 adapters  compose backends · Colima · Docker · Apple Containers (planned)
           macOS platform (launchd, resolver, lo0) · mkcert · host-run client
 kernel    effigy-kernel: identity · context · gate · store · run · custody
           signal · remote custody · observe · probe · outcome
```

Dependency rules, enforced by `qa:architecture`:

| From | May depend on | Must not depend on |
| --- | --- | --- |
| `effigy-kernel` | std, `libc`, `rusqlite` (bundled), `serde`, `serde_json`, `sha2`, `unicode-normalization` (NFC identity fields) | any other Effigy crate; Tokio; any adapter |
| adapters | `effigy-kernel` | engines, the façade |
| engines | `effigy-kernel`, adapter traits | the façade, other engines' internals |
| façade | engines | kernel internals beyond `Outcome` and contexts |

`rusqlite` is already a workspace dependency (`crates/effigy-codegraph`). The
kernel API is synchronous. Engines that use Tokio call it from blocking
contexts.

## Modules

| Module | Contract § | Absorbs |
| --- | --- | --- |
| `id` | 5.1 | typed-selector lock names (`src/runner/manifest.rs`), FNV status keys (`crates/effigy-execution/src/task_status.rs`), `safe_component` slugs, runtime-scope tokens (`crates/effigy-core/src/worktree_scope.rs`) |
| `ctx` | 5.2, 5.3 | ad hoc budgets (`src/runner/graph_time_budget.rs`, `crates/effigy-env/src/exec.rs`) |
| `gate` | 5.4 | the task lease (`src/runner/locking/io.rs`), `RouteTableLock`, `fs2` sites; keeps the flock rationale in `crates/effigy-codegraph/src/refresh.rs` |
| `store` | 5.5 | seven atomic-write helpers; the fsynced journal in `crates/effigy-host-run/src/secure_fs.rs` is the model |
| `run` | 5.6, 5.9 | `session.json`, task status records, environment markers, `stop.requested`, transition receipts |
| `custody` | 5.7 | `crates/effigy-process`, `src/runner/owned_children.rs`, the retained host child identity in `src/runner/host_process.rs` |
| `signal` | 5.7 | seven handler installations |
| `supervise` | 5.8 | headless owner loop, setsid sidecar supervisors |
| `remote` | 5.10 | exec group markers, `crates/effigy-containers/src/exec/managed_stop.rs` |
| `observe`, `probe` | 5.11 | six readiness waiters, eleven liveness functions; read-side subprocesses such as `docker ps` and `git status`, now run under the probe guard |
| `outcome` | 5.12 | string round-trips of reports (`src/runner/render.rs`) |
| `ident` | 8.5 | `crates/effigy-process/src/identity.rs`, with macOS start identity moved to `proc_pidinfo` |
| `fault` (test feature) | 12.1 | — |
| `testkit` (test feature) | 12.2 | strict fakes now spread across test helpers |

## Processes

```text
 host                                                          guest (container mode only)
 ┌──────────────────────────────────────────────┐              ┌───────────────────────────┐
 │ effigy <task>        inline owner (client)   │──exec──────▶│ workload session (setsid) │
 │   └─ effigy __kernel-sentinel   (per run)    │  in-band ack │                           │
 │ effigy dev           owning client           │◀────────────│                           │
 │   └─ effigy __kernel-supervise  (per run)    │              │                           │
 │        └─ effigy __kernel-sentinel (per run) │              │                           │
 │ workspace session    __kernel-supervise      │◀──channel───│ effigy workspace client   │
 │   └─ channel listener (per session)          │  TLS, pinned │   reads the context       │
 │ effigy status        observer                │              │   no state, gates or runs │
 │   └─ effigy __kernel-probe-guard (backup)    │              └───────────────────────────┘
 │ effigy gateway daemon  (user-owned, resident)│
 │ ~/.effigy/state/  kernel.db · gates · sock   │
 └──────────────────────────────────────────────┘
```

- The only resident process is the user gateway. Supervisors and sentinels
  live for one run and its settle; a probe guard for its observer's open
  probes. The channel listener lives with its workspace session run.
- Every per-run process re-execs the binary identified at startup.
- Guests never see `~/.effigy/state` or the vault. Only immutable object
  closures may be mounted, read-only, through a backend.
- In container mode the guest is a workspace client: local reads against a
  materialised context, every authority request over the host channel, and
  workloads in the container under host remote custody (contract §8).
  Hybrid mode has no guest.

## Flows

### One-shot task

```text
resolve (Observe) → Run::begin: gate, settle or retry every unresolved predecessor
(refuse on residue unless acknowledged), Admitted
→ sentinel → intend(process) → fork, barrier, record, go → wait, bounded capture
→ Stopping → settle (verify group closure) → Closed → release gate → Outcome
```

Signals reach the client's router; it cancels, forwards to custody groups and
settles under a cleanup budget.

### Managed run with an environment

```text
client: resolve → spawn supervisor (invocation on a pipe)
supervisor: Run::begin → socket intent → bind → receipt → "admitted" → setup steps (journalled)
  → attach(env, Borrow or Exclusive): Gate(Env), one transaction, release gate
    (EnvNotActive → ensure; the apply run inserts the attachment in its last commit)
  → wait_all(ExecReady, …) → workloads (local custody, remote custody) → Active
client: render events and logs; EOF without detach → stop request
supervisor: Stopping → settle: remote workloads, local workloads, attachment
  (the reconciler applies its release policy under a permit) → Closed
```

### Environment transition

```text
Gate(Env), held for the whole run → holders(env) → for each losing holder:
request settle, wait for it to resolve (holders release attachments without
Gate(Env); new attaches wait on it) → permit → claims transaction
→ per destructive effect: intent re-checks holders and generation → adapter
apply or down → probes → commit applied generation
```

### Observation

```text
Observe command → ObserveContext only (no EffectContext, Run or journal)
→ probe_process: the observer forks the probe behind a barrier, registers it
  with __kernel-probe-guard, releases it on acknowledgement, keeps the leader
  unreaped and kills the group at the deadline; the guard (descriptor group
  signal on Linux 6.9 or later, detected at registration; otherwise verified
  leader or nonce) stops it on observer death or at the deadline plus grace;
  processes without evidence are left alone
  → bounded output
→ connect: local TCP, HTTP GET or HEAD, control sockets, gateway DNS
→ Outcome; filesystem unchanged
```

### Recovery

```text
next Mutate on an owner: an unresolved predecessor is either non-closed with a
free gate (Interrupted) or Closed with unacknowledged residue
→ settle or retry under the new holder: observe each effect, undo verified
  ones with generation evidence (unreaped leader, verified leader identity,
  or custody nonce), record residue for Unknown
→ the new run is admitted, or refused PredecessorUnsettled unless the
  residue is explicitly acknowledged
```

## Implementation cuts

Kernel-lane cuts, in order. Each lands with its contract §5 proof list.
Engine migrations belong to the engine lanes and start only after cut 7.

| # | Cut | Proof gate |
| --- | --- | --- |
| 1 | `id`, `ctx`, `outcome`, `observe` types; encoding v1 golden table | identity proofs (§5.1); compile-fail class tests (§5.3) |
| 2 | `gate`, `store`, `atomic_replace`, state-root trust and filesystem checks | gate and store proofs, including SIGSTOP and ENOSPC |
| 3 | `run`: begin, journal, requests, settle; attachments and claims | journal-boundary kills; concurrent settle; closed-with-residue predecessor before `begin`; permit refusals; attach stalled between permit and apply |
| 4 | `custody` and `signal`: barrier spawn, `spawn_all`, stop with the unreaped leader, nonce-evidenced recovery stop, router, sentinel, detached supervisor | signal × run-kind kill matrix; partial spawn; reused or foreign process group never signalled; secrets out of argv |
| 5 | `remote`: adapter contract, wrapper, conformance suite; Docker and nerdctl adapters | conformance on the strict fake, then real-engine qualification |
| 6 | `probe`: `ObserveContext` capabilities and the probe guard | `Unknown` injection; HTTP status policy; guarded subprocess and HTTP probes on a permission-denied root with an empty diff; guard SIGKILL, observer SIGKILL and SIGSTOP, both-dead and descendant cases on macOS and Linux |
| 7 | `fault` harness and the full oracle suite green | §12.1 oracles over every scenario |
| 8 | `qa:kernel-ratchet` active with its baseline | the gate fails on a new raw primitive |

## Boundaries

- The kernel owns mechanisms. Policy belongs to engines: idle stop, release
  modes, Compose recreation (Q-004), heavy admission (Q-006), gateway
  activation.
- No second execution path. Contract
  [013](../contracts/013-task-execution-request-contract.md)'s request and
  plan stay the only entry to execution; the kernel sits under them.
- QA-group controls from contract
  [051](../contracts/051-bounded-qa-groups-contract.md) reuse the kernel's
  run, custody and deadline. They get no private cancellation path.
- Contracts [005](../contracts/005-container-runtime-contract.md) and
  [049](../contracts/049-heavy-validation-admission-contract.md) describe
  current behavior until their engine lanes reconcile them with 052.

## Foundation API and QA

Identity constructors consume resolved coordinates; selector grammar stays in
engines. Catalog coordinates normalize separators and dot components without
escaping the checkout. Unicode fields normalize to NFC with
`unicode-normalization`. Encoding v1 tags follow the enum order in §5.1, from
zero; checkout and run key tokens are lowercase hex and lowercase base32 text,
respectively. The fixed canonical-bytes and gate-name table lives in
`crates/effigy-kernel/tests/fixtures/identity-v1.json`.

A `ResolvedInvocation` owns immutable resolved facts. Its digest covers the
`effigy.invocation` NUL/version-1 prefix and the serialized `InvocationSpec`
field order, with sorted maps, canonical gate order and raw Unix argv bytes.
`ResolvedInvocation::decode` accepts at most 1 MiB of the exact canonical
serialized frame. It rebuilds through the identity constructors and checks
schema, normalization, gate ordering and digest; unsupported schemas,
noncanonical or tampered frames are refused. Unix argv byte arrays round-trip
without Unicode conversion. The decoder performs no selector resolution or I/O.
Plan, execution and status consume this same object. Outcome next commands
require an engine-supplied parser at construction; kernel code contains no
command grammar. Observe capabilities expose bounds and checks only, with no
conversion back into an effect context. Runtime Gate/Store/Custody compile-fail
proofs follow their primitives; foundation compile-fail tests exercise their
required effect-capability boundary now.

The root manifest selects the façade and kernel as Cargo default members, so
existing macOS/Linux CI test, doc-test and lint commands include the kernel.
No engine depends on the kernel yet.
`test:kernel:<area>` selectors own the narrow proof families; `check:kernel`
compiles all kernel targets and runs strict clippy. The `fault-injection`
feature's observation hook exists only under `cfg(test)`.

The ratchet's `syn`/`proc-macro2` parser, `tempfile` fixtures and Rhai
cancellation conformance are QA-only dev dependencies, never runtime dependencies. It parses source without host cfg
expansion so platform-specific constructs are included on either host. It
resolves imports and local receivers, inspects macro arguments, propagates
state-path ownership through ordinary external module ancestors, and keeps
`cfg(test)` items and `tests/` trees separate. State writes are gated in modules
containing `.effigy` path literals or Effigy-home builders. Unparseable Rust or
source symlinks fail the scan. Production file/construct counts must match the
baseline exactly; a legitimate reduction lowers the baseline in that change.
`qa:kernel-ratchet:baseline` explicitly regenerates the inventory for review;
ordinary `qa:kernel-ratchet` never changes it. The ratchet tests include an
added raw command against the real baseline and controls for rises, new pairs
and unrecorded falls. This activates the baseline against the existing engines
without migrating any of them.

## Gate and store implementation

`root` opens `~/.effigy/state/` from `$HOME`: for effects it creates the
root 0700 and refuses guests before any filesystem call, untrusted
components and network filesystems; for observation it never creates and
reports absence or `Unknown`. The operator's spelling of `HOME` is resolved
once; every component from `/` down is then reopened no-follow and
owner-checked, and everything below `HOME` is descriptor-relative (`fs`).

`gate` takes `LOCK_SH` on every scope gate, then `LOCK_EX` on each resource
gate in canonical order, polling to the earlier of the wait's instant and
the context deadline. `.scope` inodes are never unlinked; scope removal
deletes only the scope directory under `LOCK_EX | LOCK_NB`. A probe of a
never-created gate is `Absent(NoGateInode)`. The kernel enforces class
capabilities by resource kind; whether a store is derived or a source is
the engine lane's assignment. Gate acquisition refuses inside a store
transaction (K14). On macOS one of two concurrent `O_CREAT` opens of the
same new name can fail with ENOENT, so creating opens retry briefly.

`store` holds schema 1 with the §7.2 tables; run, event, attachment, claim
and request rows are written by their owning primitives. `record` provides
`atomic_replace`, bounded `read_record` and `read_cache`, and the one-pointer
`commit_generation`. `retention` follows §7.4 under a per-call removal
budget. A tombstoned checkout's own resource rows do not hold its tombstone:
once nothing unsettled, claimed or attached references it for 30 days, its
gate scope, settled runs, resources and row are removed together.

Fault points (`fault-injection`, test builds only) cover gate acquire, store
commit before and after, and every `atomic_replace` step. `test:kernel:gate`
and `test:kernel:store` run each traced point with ENOSPC and EPERM in
process and SIGKILL in a re-executed child, plus the SIGSTOP, two-process
scope-removal race and path-safety proofs.

## Run, attachment and claim implementation

`run::Run` owns its store handle and owner gate. Admission acquires the
invocation's gates in canonical order, then drops auxiliary gates before
recovery or adapter calls (K14). A run ID is supplied by the engine and rejected
on reuse. The owner generation and `Admitted` row commit together. Recovery
uses the same settle implementation as normal shutdown; acknowledged residue
remains visible and is retried before later admissions.

Effect tickets bind a run, owner generation and journal sequence. The typed
`EffectSpec` envelope in `events.target` contains the target fence, adapter
payload, settlement and required-obligation flag. Callbacks recheck both owner
and target generations. `applied` records occurrence; `completed` separately
records the live owner's natural completion evidence. Only `finish` commits a
validated workload outcome. Recovery never infers success from an applied
spawn, absence or a free gate. A failed journal write prevents further effects
on that handle. Status is a pure projection with an engine-validated next
invocation for interruption and residue.

`SettlementAdapter` observes exact ownership before release or reconciliation.
Unknown preserves residue; retained and irreversible effects never call undo.
Cleanup uses its independent context, writes each result before proceeding,
and retains the gate through retryable close-commit failures until the cleanup
budget ends. It preserves the workload outcome when retrying a closed run.

Attachments and claim reservations commit with their journal effects in one
transaction. Attachment acquisition follows the owner-to-environment canonical
gate order. An environment transition permit borrows its gated run and checks
holders again in each intent transaction. The kernel does not exempt an attached
holder from destructive transitions: the exception is the holder's own release,
which needs no environment gate. Unresolved external effects retain attachments
and claims, including after a residue acknowledgement. Release policy stays in
the environment engine; the recorded attachment mode remains available in the
effect projection. `Attachments::publish_active` publishes a ready environment's
applied generation under its owner gate.

Claim reservations protect the complete conflict set. They release atomically
with their journal result and retain exact reservation tokens through quarantine;
a release retry cannot change a successor's claim or restart quarantine. Reuse
requires a released row or positive boot-clock expiry. Pending claims are
resolved through their owning run's settle, never reclaimed from liveness age.

`test:kernel:run`, `test:kernel:attachments` and `test:kernel:claims` cover these
primitives. The run suite traces successful and closed-with-residue scenarios,
then injects SIGKILL, ENOSPC and EPERM at every crossed journal boundary in a
fresh HOME. Strict fake external objects and durable call/release logs exercise
O1a/O1b, O2, O3, O5, O11 and O13 at the adapter boundary. Local process-group
closure is covered by the custody proofs below; remote custody belongs to its
adapter and fault-harness cuts.


## Local custody and supervision implementation

`Custody::spawn` accepts only an admitted `Run` with registered sentinel custody.
The journal owns the intent and receipt. Prepared async-signal-safe child code
establishes the process group and waits for a go byte; inherited sibling barriers,
owner gates and sentinel writers close before it waits. Record failure kills and
reaps the blocked child. After recording, the Run retains the process core even
when the caller drops its handle. Owner cleanup retains the direct leader through
group closure and bounded capture drain. PTY children own their terminal session;
foreground children borrow the caller's terminal and restore its foreground group.

Recovery signals a group only with fresh leader generation evidence. Linux 6.9's
pidfd group operation targets the original kernel group; EINVAL selects the
contract's older-kernel fallback after another identity check. macOS and older
Linux retain the documented numeric check/signal window. After leader loss,
member signals require both nonce and start identity, with a pidfd opened first
on Linux. Unreadable or cleared environments grant no authority. Apple-protected
binaries can omit their environment from `KERN_PROCARGS2`; that is `Unknown`,
not a missing nonce that permits a group kill. The nonce qualification uses a
controlled executable whose environment is observable.

`SignalRouter` is installed once by binary main; libraries receive its root
cancel token, which observes the signal mailbox without requiring an owner-loop
poll. Cleanup tokens remain independent. Supervisor and sentinel are hidden
same-binary re-exec entry points; product dispatch remains engine-lane work.
The executable's file identity and digest are checked on both sides of startup.
Invocation and explicit spawn environments travel over bounded pipes, never argv.
Capture merges stdout and stderr before redacting into the rotating log, bounded
tail or event queue. A second streaming pass prevents replacement text or
merged boundaries from reconstructing secrets; its fallback separator is NUL,
which spawn secrets cannot contain. Log ENOSPC
changes capture metadata without changing the workload outcome.

`Run::begin` starts the sentinel after admission and waits for its validated-main
acknowledgement. One owner-gated transaction registers owner, supervisor and
sentinel identities, then Run takes the sole death-pipe writer. Closed commits
before the writer closes and the owner gate releases. Pure metadata names missing,
dead and replaced sentinels as gaps. Sentinel EOF uses the same Run settle writer
with a bounded gate wait; SIGSTOP leaves the pipe and gate held.

Detached startup records socket intent before binding, then records the socket
inode receipt. A bind fault starts no workload and settles `Closed(NotStarted)`;
a failure to commit closure remains interrupted. This implements the 2026-10-19
kernel-lane ruling recorded in contract 052 §5.8. Requests are durable before
socket notices; only generation-fenced requests act, and peer credentials are
checked. Owning-client EOF stops; secondary-client EOF does nothing; detach is a
durable request. Front ends consume the same redacted log frames and typed final
workload/exit outcome. No engine topology or command grammar lives in the kernel.

### Clause coverage (§5.7–§5.8)

| Clause | Implementation | Proof family |
| --- | --- | --- |
| I-P1/I-P2, K6; partial spawn | `custody`, child barrier, Run effect tickets | partial-spawn rollback; Kill/ENOSPC at fork, before record, before go, after go |
| I-P3; group and streams | retained direct child and group inventory | exited leader with live descendant; PTY and foreground terminal restoration |
| I-P4/I-P8, O12 | process-table trait; leader/pidfd or member nonce plus start | reused/foreign/mixed/cleared-environment strict fakes; race refusal; native nonce recovery |
| I-P5/I-P6, K10/K12 | streaming redactor, rotating log, bounded queues; explicit env | split secret and ps argv; log ENOSPC preserves ring and success |
| I-P7, K9; cascade/escalation | one main router; signal-sourced cancel token; durable escalation request | HUP/INT/TERM cascade; duplicate router refused; Rhai try/catch cannot swallow token cancellation; repeated INT without polling |
| I-SV1/I-SV4; startup | same-binary re-exec, checked invocation decoder, Run-owned registration/socket | replacement refusal; socket fault produces NotStarted; registration atomicity under Kill/ENOSPC |
| I-SV2; owner and sentinel lifetime | sole owner death pipe, same settle, pure sentinel metadata | owner/supervisor SIGKILL; both-dead successor recovery; SIGSTOP does not settle; dead sentinel gap; descriptor closes exactly once in order |
| I-SV3; clients and matrix | common events; owning versus detached/secondary liveness | owning-client kill and terminal-close matrix; detach then stop; secondary death; 288-cell front-end/prior/stop/attachment model |
| K4; one writer | lifecycle, socket and process results enter through Run | source ratchet rejects journal writes in supervision/custody; idempotent settle and attachment release |

`test:kernel:custody` builds the disposable, feature-gated same-binary fixture and
runs the real-process, strict-fake, signal, registration and socket fault proofs.
`test:kernel:run` owns the 288-cell matrix. `test:kernel:custody:linux` qualifies
those proof families, identity and contexts in an isolated temporary Colima VM
and ARM64 Linux container through host admission. It needs Colima and Docker
client tools and leaves its stopped VM/cache and logs in its printed temporary
directory. It never activates a Docker context or mounts the original checkout.
The fixture binary requires `fault-injection` and is not a product executable.

## Remote custody and adapter qualification

`remote::RemoteCustody` records the exact container identity, environment fence
and recovery nonce before it asks an adapter for a local exec-client spec. The
client uses local `Custody`; its private nonce is distinct from the guest's
`EFFIGY_CUSTODY_NONCE`. Adapters forward the guest environment privately, never
through command arguments. The fixed POSIX shell wrapper uses `setsid` when
available, reads Linux process start ticks, boot and pid-namespace identity, emits
the framed acknowledgement on stderr and execs the workload. It writes no files.

Custody strips bounded registration frames before redaction, logs, queues and
tails. Only an acknowledgement matching the intended nonce, boot and environment
becomes a remote receipt. A client exit cannot complete the remote obligation.
The existing Run writer settles the client first, then the remote effect via the
selected engine's `SettlementAdapter::remote`, including in sentinel recovery.
That adapter must also be installed in the engine's same-binary hidden entry
points. Without it, remote effects remain residue.

`remote::session` provides shared stop-helper evidence rules. Session delivery
needs the live registered leader; fallback delivery needs member nonce and start
identity. Transport delivery rechecks those facts in the registered container.
No replacement container inherits a registration. Unknown identity, an exec
failure, an unreadable member or a reused foreign session gives no signal
permission. A nonce lookup returning no registration without a positive closure
proof remains residue. Boot changes and exact container-inventory absence can
release old effects. All calls have separate phase bounds within settle's budget;
wall clocks across the host and guest are never compared.

The `fault-injection` feature exposes `strict_fake` typed call tapes for backend,
remote, platform and probe transports. Each invocation is recorded and consumes
one exact expectation. Scripts can inject Unknown, timeout and partial effects;
foreign sentinels reject writes, and unselected adapters assert an empty log.
`remote::conformance::run` enumerates the remote qualification cases. A backend
fixture supplies crash/guest perturbations and independent survivor, foreign and
unselected-backend oracles. `test:kernel:remote` proves the kernel path with strict
fakes, local exec clients and journal-boundary owner kills. It does not qualify a
real engine: Colima docker/containerd, Docker Desktop and Linux Docker still need
their disposable engine fixtures and qualification runs; Apple Containers remains
unsupported until it passes the same suite.
