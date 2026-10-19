# 052 - Runtime Kernel Contract

Status: foundation types are implemented in `crates/effigy-kernel/`: identity
encoding v1, operation contexts and class capabilities, observation and Probe
API types, outcomes, and macOS/Linux platform facts. Gates (§5.4), the store,
records and `atomic_replace` (§5.5), the state root, its kernel-owned path
families and retention (§7) are implemented with their proof lists and
fault points. `qa:kernel-ratchet` enforces the committed production baseline
in `config/kernel-ratchet.json`. The run journal and settle (§5.6), attachments and claims (§5.9) are implemented
with generation-fenced tickets, positive completion, holder protection and
journal-boundary fault proofs. Local custody, signal routing, detached supervision
and per-run sentinels (§5.7–§5.8) are implemented in the kernel, with a
same-binary qualification harness. Remote custody (§5.10), its in-band registration
wrapper, backend conformance runner and feature-gated strict fakes (§12.2) are
implemented. Real-engine remote qualification remains engine-lane work. Executable
Observe capabilities (§5.11), the in-memory probe guard and nonce-only maintenance
are implemented in the kernel, with macOS/Linux qualification selectors. The
§12.1 fault-injection driver, oracles O1–O14 and the §12.3 supervisor model
matrix are implemented (`test:kernel:faults`, `test:kernel:matrix`);
engine-specific §12.1 scenarios are placeholders owned by their engine lanes.
Fault points inside helper processes (sentinel settle, guard acknowledgement,
detached supervisor) are not yet armed; architecture 032 records the limit.
Bundle-source preparation uses Prepare runs, source custody, store gating and
atomic lock publication. Other engines have not adopted the kernel; their
mechanisms in the adoption map (§10) remain current behavior. QA-group `stop` and `hard_timeout_ms` stay unavailable: contract
[051](051-bounded-qa-groups-contract.md) keeps refusing them until this
kernel's run journal, custody and deadlines exist.

Owner: runtime kernel (Queue lane `v015-kernel`)
Architecture: [032](../architecture/032-runtime-kernel-architecture.md)
Supersedes: the former owned-run supervision proposal (see
[retired concepts](../retired.toml)); the lifecycle and ownership sections of
contracts [005](005-container-runtime-contract.md) and
[049](049-heavy-validation-admission-contract.md), whose full reconciliation
is engine-lane work
Related: [013](013-task-execution-request-contract.md),
[017](017-task-status-record-and-active-run-model-contract.md),
[018](018-task-status-query-surface-and-read-model-contract.md),
[051](051-bounded-qa-groups-contract.md), [vision](../vision.md),
[open questions](../questions.md)
Evidence: October 2026 deep audit (outside the repository,
`~/.local/share/northstar/release-audits/effigy/deep-audit-2026-10/`):
`synthesis/SYNTHESIS.md` §3 and §6, `synthesis/X-opus.md` §2,
`synthesis/X-sol.md` §2, `analysis/D1`–`D4`; PR #328's authority evidence.
Code citations are against main `bcb5bf40e`.

## 1. Purpose and boundary

Effigy has no runtime foundation. Identity, exclusion, durable intent,
process custody, deadlines and "unknown" observations are re-implemented per
domain, each unsoundly, and every cross-domain operation rebuilds another
domain's truth from files after the fact. All eleven fixes #192–#202 sit on
such a seam (§13).

`effigy-kernel` is one crate below every engine. It owns:

- identity: `ResolvedInvocation`, `TaskId`, `RunId`, `ResourceId`,
  `Generation`, `CheckoutId`, with one versioned encoding;
- exclusion and liveness: `Gate`;
- durable state: `Store`, its records and `atomic_replace`;
- intent and recovery: the `Run` journal and one `settle`;
- process custody: `OwnedProcess`, `SignalRouter`, remote custody, the
  per-run supervisor and sentinel;
- bounds: `OperationContext` (deadline, cancellation, limits);
- truth: `Observed<T>`, `Probe`, `EffectClass`, `Outcome`.

Engines on the kernel, each the single authority for its resources:

| Engine | Lane | Owns |
| --- | --- | --- |
| Task engine | task | one-shot runs: tasks, sequences, tests, QA groups |
| Run supervisor | supervisor | managed runs and their workloads |
| Environment reconciler | environment | environments, data claims, sidecar workloads |
| Gateway | gateway | endpoint claims and the user-owned daemon |
| Delivery, vault, deps, graph, bundles | owning lanes | their `Run`s and `Store` families |

The kernel contains no command grammar, Compose, Docker, Colima, launchd,
mkcert, scheduler or deployment code. Those sit behind adapters that call
kernel primitives (§8.4). The kernel adds no public command; it adds three
hidden internal entry points: `__kernel-supervise`, `__kernel-sentinel` and
`__kernel-probe-guard`.

## 2. Vocabulary

| Term | Meaning |
| --- | --- |
| Host | The machine kernel that owns the state root (§7). macOS or Linux. |
| Guest | Effigy inside a container-mode workspace container managed by a host Effigy: a workspace client with no state of its own (§8). |
| Host authority channel | The one authenticated, per-session protocol that carries a guest's requests to host engines (§8.3). |
| Workspace session | The durable host run a container-mode workspace's terminals attach to (§8.7). |
| Resource | Anything with one authority: task, run, environment, data, endpoint, store, gateway, checkout. |
| Authority | The engine that alone may mutate a resource, acting while it holds the resource's gate. |
| Gate | A never-unlinked host `flock` on a stable inode. Holding it is the holder's liveness. |
| Run | One journalled operation by one owner resource. |
| Effect | One external change a run makes: process, remote process, attachment, claim, file, adapter call, checkpoint. |
| Intent | An effect's journal entry, committed before the effect starts. |
| Settle | The one idempotent path that resolves a run's recorded effects. |
| Holder | A run attached to an environment. |
| Claim | An exclusive hold on a data, endpoint or address resource. |
| Generation | A per-resource counter. Every effect, request and callback carries it. |
| Residue | An effect settle could not prove released. |
| Settled | `Closed` with `Complete` cleanup. |
| Resolved | Settled, or `Closed` with residue an operator explicitly acknowledged. Anything else is unresolved. |
| Probe guard | A per-observer helper that backs up the observer's custody of its probe subprocesses. It writes nothing. |
| Observed | `Verified`, `Absent` (with proof) or `Unknown` (with reason). |

## 3. Decisions where the audit designs differ

The two cross-domain designs (`X-opus.md` §2.4, `X-sol.md` §2) converge on
the kernel. Where they differ, the brief, Tom's 2026-10-18 rulings or the
code settle it:

| # | Topic | X-opus | X-sol | Decision | Ground |
| --- | --- | --- | --- | --- | --- |
| D1 | Runtime store | per-checkout JSON records, revision field | one SQLite store: operations, resources, attachments, claims, requests, events | SQLite `kernel.db`. The vault and user-owned files stay separate files written through `atomic_replace`. | Brief; SYNTHESIS §3 |
| D2 | Store and gate location | `<checkout>/.effigy/kernel/` | outside disappearing worktrees | `~/.effigy/state/` on the host, gates included | Brief. A path never bind-mounted into a guest makes R1 structural. A deleted worktree cannot take a held gate's inode with it. |
| D3 | Settle | replay recorded undo in reverse | per-adapter semantics, positive settlement | Reverse order, with a declared `Settlement` per effect. Irreversible effects are reported, never replayed. | Both; X-sol's objection to blind undo of borrowed or irreversible effects is absorbed |
| D4 | Meaning of a free gate | free gate + live record = interrupted | release permits reconciliation, not successor activation | Both. A free gate with a non-closed run reads `Interrupted`. A successor must settle it before its own run is admitted (K5). | X-sol §3: lock release does not prove grouped or remote work ended |
| D5 | Effect classes | static, read, refresh, mutate | Static, Observe, RefreshDerived, PrepareSource, Mutate | Static, Observe, RefreshDerived, Prepare, Mutate | Brief |
| D6 | Managed supervisor | detached | foreground stays foreground | One detached supervisor per managed run. Owning-client death means stop and settle. Attaching to a running stack leaves it running. | Tom ruling 5 |
| D7 | Guest Effigy | isolate, opt-in, runs under the host run's token | cut auto-provisioning, host owner protocol | Workspace client in container mode only: local reads against a materialised context; every authority request over one authenticated host channel, re-resolved, gated and journalled on the host; a durable workspace session run; a version-locked Linux binary installed at Prepare. Hybrid mode has no guest (§8). | Lane rule 1; Tom 2026-10-18 (container mode needs full coverage); `synthesis/GUEST-EFFIGY.md` option (c) |
| D8 | Endpoint store | `state.json` + lock | SQLite + effect outbox + reuse quarantine | Endpoint claims are kernel store rows; the gateway engine is their only writer; released addresses are quarantined | D1; X-sol gap 6 |
| D9 | Remote registration | ack `{container_id, nonce, pgid}` | ack with namespace and start identity | In-band ack on the exec stream, plus the nonce in the remote environment for recovery; no bind-mount marker files | R1: guests never write host state |
| D10 | Owner SIGKILL of a one-shot run | "no custody process outlives its run" incl. KILL | a small supervisor may outlive the front end | Inline runs with custody get a per-run sentinel that settles on owner death | X-opus K5 |

Not decided here: Q-003 deployment ownership, Q-004 the Compose boundary,
Q-005 service catalogs, Q-006 heavy tasks, environment idle-stop policy,
gateway activation mode, the ports-registry fate and graph corpus
semantics. Kernel rules constrain those engines; they do not choose for them.

## 4. Rules and invariants

Rules R1–R5 are the lane rules. Invariants K1–K15 make them enforceable.
Each lists the states it forbids. A brief or review that touches an engine
names the rule or invariant it relies on.

| # | Rule | Forbidden states |
| --- | --- | --- |
| R1 | **Host authority.** Every mutable resource has exactly one authority and one gate. Gates and the store live on the host. Guests never lock host gates or write host records. Effigy in a guest keeps working by requesting host authority (§8). | A guest holding or opening a host gate; a guest writing the host store; a host acting on a guest-resolved plan; authority inferred from a hostname, heartbeat age or a file in a bind mount. |
| R2 | **Pure reads.** Static and Observe commands write nothing anywhere. Observe may run guarded, non-mutating probes (§5.11). | `help`, `status`, `tasks`, `--plan`, `graph status` or `doctor` creating, renaming or truncating a file, appending a Git exclude, allocating a port or address, rendering Compose, minting a scope token, fetching a bundle, writing the store or a journal, or acquiring a gate exclusively. |
| R3 | **Unknown grants nothing.** An `Unknown` observation never authorises a kill, teardown, reclaim, reuse, displacement, restart or deletion. Destructive action needs the resource's own gate plus `Verified` ownership and identity, or `Absent` with proof. | A restart because an exec probe failed; reuse of a gateway whose identity could not be read; a route displaced because owner evidence was unreadable; git failure read as clean; a lock reclaimed because a heartbeat aged. |
| R4 | **Holder protection.** Nothing stops, restarts, recreates, resets or hands over an environment while a live or unknown holder of a bound run exists, except that run's own settle releasing its own attachment. | Any `down`, restart, recreate, reset, handover or reap of an environment with a holder whose liveness is `Verified` or `Unknown`. |
| R5 | **Positive settlement.** Settled requires a closed run, complete cleanup, and every effect `Released`, `Retained`, `Irreversible` or `Failed`. Succeeded requires natural completion of every required obligation. | A missing record, dead owner, free gate, elapsed time, empty inventory or killed client manufacturing `Settled`, `Succeeded` or `stopped`. |

| # | Invariant | Forbidden states |
| --- | --- | --- |
| K1 | **One identity.** Equivalent spellings resolve to one canonical `ResourceId`. Distinct resources never collide. The encoding is versioned (§5.1). | Three gates for `build`, `api/build`, `./apps/api/build`; `a:b` sharing a gate with `a-b`; any identity derived from `std::hash` or a lossy slug. |
| K2 | **Gate is liveness.** A gate is held exactly while a live process on this host holds it. | Heartbeats, staleness, unlink-reclaim or an unlock command; two processes believing they hold one gate. |
| K3 | **Intent first.** No external effect without a committed intent. `Admitted` commits before the first effect of a run, including setup steps, env-schema commands, gateway ensure and spawn. Observation probes are not effects: they change nothing Effigy owns and run under a probe guard, never a journal (§5.11). | A child, container, route or file effect with no prior journal entry; publication before bind. |
| K4 | **One settle.** One settle implementation, run only by the owner gate's holder, in reverse effect order. It is idempotent and resumable. | Two teardown authorities for one effect; cleanup only on the success path; settle reconstructed from directory scans. |
| K5 | **One unresolved run per owner.** `Run::begin` admits a run only when every earlier run of its owner is settled, or that run's remaining residue was explicitly acknowledged. A non-closed predecessor is settled first; a `Closed` predecessor with `Incomplete` cleanup gets a settle retry first. | A successor running beside a predecessor's unsettled record or unacknowledged residue. |
| K6 | **Custody closure.** No Effigy-spawned process outlives its run's settle. No probe process with generation evidence outlives its deadline plus grace while its observer or its guard lives (§5.11). The owner exists before the first spawn. A spawn barrier prevents an unrecorded workload. Partial spawns roll back. Process groups are retained through stream close. Signals need positive generation evidence (I-P4). | Orphans after SIGTERM, SIGHUP or owner SIGKILL; leader exit read as group closure; client exit read as remote closure; a group or session signalled because its number is in use. |
| K7 | **Fencing.** Every effect, request and callback carries `(ResourceId, Generation)`. A stale generation is refused. | A stale stop stopping a successor; a late callback deleting a successor's route. |
| K8 | **Bounded.** Every kernel wait takes a deadline. Expiry cancels at a safe boundary and never detaches a writer. Only declared workload lifetimes are open-ended. | A `starting` loop that skips its deadline; curl without a time limit; a detached writer killed mid-transaction. |
| K9 | **One signal router.** One router per process, installed in `main` before any spawn. Libraries take a cancel token and never install handlers. HUP, INT and TERM are handled. | A library swallowing INT or TERM; HUP killing an owner with no cascade. |
| K10 | **Observability is not outcome.** A failed status, log or report write never changes a workload's outcome or exit code. | A succeeded task turned into an error by a status write; ENOSPC on a log failing the workload. |
| K11 | **Output independence.** Output format never changes execution. One output owner per invocation. | `--json` selecting a different executor, scheduler, timeout or cleanup. |
| K12 | **Secrets stay out of argv and global env.** Secrets reach a child only through its spawn spec's environment or a 0600 file or descriptor, and are redacted before any sink. | Secret bytes in `ps`-visible argv; `set_var` of secrets or run tokens; unredacted TUI transcripts. |
| K13 | **Safe files.** Every kernel open is descriptor-relative, no-follow, owner- and mode-checked and size-bounded. Mutation on a network filesystem is refused. | Following a symlink in the state root; reading an unbounded or non-regular file; flock on NFS or SMB. |
| K14 | **Acquisition order.** Multi-gate acquisition follows canonical order. No store transaction, and no gate other than an owner gate, is held across a subprocess, prompt or network call. | A global allocator lock held during `colima list`; an admin prompt under `routes.lock`. |
| K15 | **Bounded retention.** Retention never removes an unsettled run, a referenced object or a live claim. Garbage collection never runs in Static or Observe. | Unbounded history; deleting evidence a settle still needs. |

## 5. Primitives

Each primitive has an API sketch, invariants, a failure model and a proof
obligation. The sketches fix semantics, not exact Rust signatures. Failure
rows: **Crash** (process exits abnormally), **Kill** (SIGKILL),
**ENOSPC**, **Perm** (EPERM or EACCES), **Clock** (wall-clock jump, suspend).
No primitive claims exactly-once across a filesystem, engine or network
boundary.

### 5.1 Identity and `ResolvedInvocation`

#### API

```rust
pub struct CheckoutId { kind: CheckoutKind, token: [u8; 16] }
pub enum CheckoutKind { Primary, LinkedWorktree, EphemeralClone, Machine }

pub struct TaskId {
    checkout: CheckoutId,
    catalog: CatalogPath,        // root-relative, '/'-separated, "" for the root
    surface: Surface,            // published, or draft while drafts exist
    task: TaskName,
    profile: Option<ProfileName>,
}

pub struct RunId([u8; 16]);      // 48-bit ms time + 80-bit random; text: 26-char base32
pub struct Generation(u64);      // per resource, strictly increasing

pub enum ResourceKind { Checkout, Task, Run, Env, Data, Endpoint, Store, Gateway }
pub struct ResourceId { kind: ResourceKind, scope: Scope, key: Vec<Field> }
pub enum Scope { Machine, Checkout(CheckoutId) }

impl ResourceId {
    pub fn canonical(&self) -> Vec<u8>;   // encoding v1, below
    pub fn name(&self) -> GateName;       // 32-char file-safe digest name
    pub fn label(&self) -> String;        // display only
}

pub struct ResolvedInvocation {
    schema: u16,                          // 1
    task: TaskId,
    route: Route,                         // task, builtin, exec alias, defer
    argv: Vec<OsString>,
    cwd: CheckoutPath,
    target: ExecutionTarget,              // Host, or Container { env, service, user, workdir }
    env: EnvPlan,                         // names, provenance, secret references; never values
    owner: ResourceId,
    gates: Vec<ResourceId>,
    attachments: Vec<AttachmentRequest>,
    class: EffectClass,
    placement: Placement,
    provenance: Provenance,               // bundle pins, catalog digest, manifest digests
    digest: Digest,                       // SHA-256 of the canonical invocation
}
```

Encoding v1:

```text
canonical := "effigy.rid" 0x00 u8(1) u8(kind) scope key
scope     := u8(0)                       machine
           | u8(1) u8(checkout kind) 16-byte token
field     := u8(present) u32_be(len) bytes      UTF-8, NFC; paths '/'-separated,
                                                 no '.', '..' or trailing '/'
name      := base32_lower(SHA-256(canonical)[0..20])
```

| Kind | Scope | Key fields, in order |
| --- | --- | --- |
| Checkout | machine | token |
| Task | checkout | catalog path, surface, task name, profile |
| Run | machine | run id |
| Env | checkout | environment name |
| Data | machine | backend id (engine kind, context or profile), volume name |
| Endpoint | machine | endpoint kind (domain, address, port), normalized value |
| Store | checkout or machine | store name (`vault`, `bundles`, `graph`, `deps`, `migrate:<scope>`) |
| Gateway | machine | instance (`default`, or the private home's digest) |

`CheckoutId` tokens live in Git's per-checkout state. Linked worktrees and
ephemeral clones reuse the existing runtime-scope token
(`crates/effigy-core/src/worktree_scope.rs`), so Compose project and volume
names do not change. Primary checkouts get a token beside it. Only Prepare
and Mutate classes mint a token; Observe reports "no runtime state yet".

#### Invariants

- I-ID1: one resolver produces `ResolvedInvocation`, once. Lock, status,
  logs, plan and JSON consume that object; nothing re-derives identity from a
  selector spelling (K1).
- I-ID2: the store keeps each resource's canonical bytes. Inserting a name
  whose canonical bytes differ is a hard error, never a merge.
- I-ID3: a `RunId` is never reused and is never authority. Authority is the
  owner gate.
- I-ID4: a data resource's key includes its backend identity, so an action on
  the wrong backend names a different resource.
- I-ID5: a new encoding version gets a new version byte. Old names are read
  through an explicit importer, never re-hashed in place.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill | Identity is pure computation; nothing to recover. |
| ENOSPC, Perm | Minting a checkout token fails before any effect. Observe proceeds without one. |
| Clock | `RunId` time bits are ordering hints only. Ordering authority is journal sequence. |
| Collision | I-ID2 refuses with both canonical forms. |

#### Proof

- Four spellings of one task give one `TaskId`, one gate and one status row.
  `a:b` and `a-b` stay distinct.
- A golden canonical-bytes table per kind, fixed across Rust toolchains.
- Fuzzed field boundaries (empty, separators, NFC/NFD) never collide.
- A plan, a run and a status for one invocation report the same digest.

### 5.2 `OperationContext`

#### API

```rust
pub struct OperationContext {
    deadline: Deadline,
    cancel: CancelToken,
    limits: Limits,
    class: EffectClass,
    placement: Placement,
}

pub enum Deadline { At(Instant), Open(OpenReason) }  // Open: dev servers, logs -f, shells

impl OperationContext {
    pub fn child(&self, budget: Duration) -> OperationContext; // min(remaining, budget)
    pub fn cleanup(&self, budget: Duration) -> CleanupContext; // fresh deadline; not
                                                               // cancelled by the parent
    pub fn remaining(&self) -> Option<Duration>;
    pub fn check(&self) -> Result<(), Interrupt>;              // TimedOut { phase } or
                                                               // Cancelled { cause }
}

pub struct Limits {
    capture_tail: usize, log_rotate: u64, log_keep: u8,
    line_max: usize, queue_bytes: usize, probe_concurrency: u8,
}
```

#### Invariants

- I-CX1: every kernel wait takes a context: gate wait, store busy wait,
  process wait, capture drain, probe, adapter call (K8).
- I-CX2: `Open` applies only to a declared workload lifetime. Its start, stop
  and probe phases always run under `At`.
- I-CX3: expiry or cancellation stops work at a safe boundary: between store
  transactions, before the next effect, or by stopping owned processes. It
  never detaches a writer.
- I-CX4: cleanup runs under its own bounded `CleanupContext`. A first
  cancellation never cancels cleanup. A second signal escalates it (§5.7).
- I-CX5: no process-global state carries context. It is passed explicitly.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill | Context is in memory. Persisted deadlines (§7.1) carry boot identity and boot-monotonic time. |
| ENOSPC, Perm | The context holds no files. Failures surface from the operations it bounds. |
| Clock | Deadlines use `Instant`. Wall time is display only. Suspend pauses the budget on both platforms. |
| Expiry | `TimedOut { phase, elapsed }`; owned work is stopped, or retained work is reported as residue. |

#### Proof

- Grep gate: no kernel wait without a context argument.
- Stuck `starting`, hung HTTP peer, hung git and hung inventory fixtures each
  end within deadline plus cleanup budget.
- A cancelled parent still completes cleanup. A second signal escalates it.

### 5.3 `EffectClass`

#### API

```rust
pub enum EffectClass { Static, Observe, RefreshDerived, Prepare, Mutate }

impl Kernel {
    pub fn open(class: EffectClass, placement: Placement, router: &RouterHandle)
        -> Result<Kernel, KernelError>;            // Static never opens the state root
    pub fn observe(&self) -> ObserveContext;        // no write capability
    pub fn effects(&self) -> Result<EffectContext, ClassError>; // Err for Static, Observe
}
```

| Capability | Static | Observe | RefreshDerived | Prepare | Mutate |
| --- | --- | --- | --- | --- | --- |
| Load manifests | no | yes | yes | yes | yes |
| Read store, probe gates | no | yes | yes | yes | yes |
| Acquire gates | no | no | derived stores | source stores | all |
| Write store or files | no | no | derived caches | sources, user files | all |
| Observation I/O: local TCP connect, HTTP `GET` or `HEAD`, Unix control sockets, DNS to the gateway | no | yes, guarded (§5.11) | yes | yes | yes |
| Network for source acquisition | no | no | no | yes | no |
| Network for declared adapter effects | no | no | no | no | yes |
| Spawn processes | no | guarded probes only (§5.11) | indexers, under a run | fetch, clone, install, under a run | all, under a run |
| Git exclude, token minting | no | no | no | yes | yes |

Probes run non-mutating forms, for example `git --no-optional-locks status`,
and only through `ObserveContext::probe_process` and `ObserveContext::connect`
(§5.11). Observe obtains no `EffectContext`, no `Run` and no journal.

#### Invariants

- I-EC1: every command declares one class in the command inventory. The
  engine lane assigns them; the kernel enforces them.
- I-EC2: write capabilities are types that Static and Observe contexts cannot
  construct.
- I-EC3: executable sources (bundles, provider packages, catalog packs) are
  acquired only in Prepare. Discovery never acquires executable source.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill | Static and Observe wrote nothing, so there is nothing to recover. Other classes recover through their runs (§5.6). |
| ENOSPC | Static and Observe are unaffected. Other classes fail before their next effect. |
| Perm (read-only or permission-denied state root) | Observe succeeds or returns `Unknown` per fact. Mutate refuses before any effect. |
| Clock | No class decision depends on time. |
| Missing pinned source | Observe reports it and names the Prepare command. It never fetches. |

#### Proof

- Every Static and Observe command runs against a permission-denied state
  root and checkout. It succeeds, and a filesystem snapshot diff is empty.
- Compile-fail tests: an Observe context cannot reach `Store::txn`,
  `Gate::acquire` or `Custody::spawn`.
- A guarded subprocess probe and an HTTP probe run against a
  permission-denied state root and checkout with an empty diff (§5.11).

### 5.4 `Gate`

#### API

```rust
pub enum Wait { FailFast, Until(Deadline) }

impl Gate {
    pub fn acquire(cx: &EffectContext, ids: &[ResourceId], wait: Wait)
        -> Result<GateGuard, GateError>;              // sorted, all-or-nothing
    pub fn probe(cx: &ObserveContext, id: &ResourceId) -> Observed<GateState>;
}

pub enum GateState { Held(Observed<HolderDiag>), Free }

pub enum GateError {
    Busy { id: ResourceId, holder: Observed<HolderDiag> },
    TimedOut { id: ResourceId, waited: Duration, holder: Observed<HolderDiag> },
    HostAuthorityRequired,                            // guest placement, host resource
    UnsupportedFilesystem { path: PathBuf, fs: String },
    Untrusted { path: PathBuf, reason: String },
    Io(IoError),
}
```

Layout under the state root:

```text
gates/<scope-name>.scope           LOCK_SH by every resource acquisition in that scope
gates/<scope-name>/<rid-name>.gate LOCK_EX by the holder; never unlinked while the scope lives
```

| Step | Acquire | Probe |
| --- | --- | --- |
| 1 | Placement check: guest plus host resource returns `HostAuthorityRequired`. | Open the gate file read-only, no create. Missing: `Free`, proof `NoGateInode`. |
| 2 | Sort ids canonically. For each scope: `LOCK_SH` on the scope gate. | `LOCK_SH or LOCK_NB`: success means `Free` (release at once); `EWOULDBLOCK` means `Held`. |
| 3 | For each id: open or create (`O_CLOEXEC or O_NOFOLLOW`, 0600), then `LOCK_EX`, polled until the deadline. `FailFast` still polls for 100 ms, which absorbs a concurrent probe's brief shared lock. | Read the diagnostic body. Torn or missing body: `Unknown` diagnostics, never authority. |
| 4 | On timeout or error: release everything taken, return `Busy` or `TimedOut` naming the holder. | — |
| 5 | Write the diagnostic body (run id, pid, start identity, boot, argv0) with `pwrite` + `ftruncate`. Failure only loses diagnostics. | — |

| Gate state | Probe result | Meaning |
| --- | --- | --- |
| No inode | `Free` (`NoGateInode`) | Never acquired. |
| Free | `Free` | No live holder on this host. |
| Held | `Held` | A live process holds it. |
| Held by a stopped process | `Held` (state `stopped`) | Still held. Never stolen. |
| Probe error | `Unknown` | Grants nothing (R3). |

#### Invariants

- I-G1: gate files are never unlinked while their scope lives. Scope removal
  (§7.4) takes the scope gate `LOCK_EX or LOCK_NB`; it fails while any guard in
  that scope exists.
- I-G2: holding is liveness (K2). The diagnostic body is never authority.
- I-G3: acquisition is canonical-order and all-or-nothing (K14). A gate
  acquired later, while others are held, may wait under a deadline only when
  no holder of that gate ever waits on the gates already held; otherwise it
  uses `FailFast`. `Gate(Env)` is the defined case: attaching runs wait for
  it, and environment runs never wait on a holder's owner gate (I-A2).
- I-G4: a gate held across a subprocess, prompt or network call is a run's
  owner gate, held by a journalled run under a deadline. Work that needs
  exclusion across such calls, such as bundle sync or graph refresh, runs as a
  `Run` owned by that store. A gate taken outside a run, for one
  `atomic_replace` or one claim check, covers one bounded critical section
  with no subprocess, prompt or network call (K14).
- I-G5: a run's terminal record commits before its owner gate is released.
  A successor never sees a free gate with a stale terminal write still
  pending.
- I-G6: gate descriptors are `O_CLOEXEC`. A spawned workload never inherits a
  gate across exec; custody closes unrelated descriptors before its spawn
  barrier (§5.7). A concurrent fork can temporarily copy a gate's open file
  description before those closes or exec. Normal release explicitly issues
  `LOCK_UN` before closing: guard drop, partial-acquisition rollback,
  auxiliary-gate release and scope removal release the shared description
  even while such a child still has a descriptor. Resource locks release
  before scope locks. There is no post-release grace window for `probe` or
  `acquire`; K2 and the probe states above are unchanged.
- I-G7: there is no unlock command. A refusing alias may exist for one minor.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill of holder | The kernel drops the `flock` with the last descriptor. The next acquirer sees the run non-closed and settles it (K5). |
| SIGSTOP of holder | The gate stays held. Waiters time out with `Busy`, holder state `stopped`. Nothing is stolen. |
| ENOSPC | Creating a gate inode fails: acquisition fails before any effect. A diagnostic write failure loses diagnostics only. |
| Perm | Untrusted root, wrong owner or mode, or a symlinked component: `Untrusted`, refusing Mutate. Observe returns `Unknown`. |
| Clock | Gates have no time component. Suspend cannot make a live holder look dead. |
| Network filesystem | `UnsupportedFilesystem`: Mutate refused, Observe allowed. |

#### Proof

- SIGSTOP the holder for 60 s: a second acquirer waits, then reports `Busy`
  naming the stopped holder. Resume: the holder completes.
- SIGKILL the holder: the next acquire succeeds at once and settles the
  predecessor.
- Two concurrent scope removals and one acquirer never yield two holders of
  one resource.
- A guest-placement acquire of a host resource returns
  `HostAuthorityRequired` without touching the state root.
- A forked workload never holds a gate after the owner dies.
- Force a concurrent fork to retain its descriptors before exec: guard drop
  immediately probes `Free` and permits a successor without waiting for the
  child. Partial-acquisition rollback also releases its resource and scope
  locks while the child retains them.

### 5.5 `Store`, records and `atomic_replace`

#### API

```rust
impl Store {
    pub fn read(cx: &ObserveContext) -> Result<StoreReader, StoreError>;  // read-only open
    pub fn txn<T>(cx: &EffectContext, f: impl FnOnce(&mut Txn) -> Result<T, StoreError>)
        -> Result<T, StoreError>;      // BEGIN IMMEDIATE; busy timeout from the context
}

pub fn atomic_replace(
    cx: &EffectContext,
    guard: &GateGuard,                 // the target's store gate
    dir: &DirFd, name: &FileName, bytes: &[u8],
    opts: ReplaceOpts,                 // mode, keep_previous
) -> Result<Replaced, StoreError>;

pub fn read_record<T: Record>(cx: &ObserveContext, dir: &DirFd, name: &FileName)
    -> Observed<T>;                    // never a default value
```

SQLite settings: `journal_mode=DELETE`, `synchronous=FULL`,
`foreign_keys=ON`, `busy_timeout` from the context. Write-ahead logging is
not used: a read-only reader must work on a read-only filesystem (R2).
SQLite opens files by path, so `kernel.db` is the one K13 exception to
descriptor-relative opens: the kernel creates or verifies it
descriptor-relative inside the verified 0700 root (regular, operator-owned,
0600, one link, size-bounded) and SQLite then opens it with
`SQLITE_OPEN_NOFOLLOW`.

`atomic_replace` order: `openat` a temporary name with `O_CREAT or O_EXCL or
O_NOFOLLOW or O_CLOEXEC` and the final mode, write, `fsync`, optionally link
the current file to `<name>.prev`, `renameat`, `fsync` the directory.

A multi-file artefact (vault ciphertext plus key wrap) commits through one
pointer: new files are written under generation-suffixed names, then one
`atomic_replace` of the pointer selects them. Two independent renames are not
a transaction.

#### Invariants

- I-S1: related facts commit in one transaction: a run's phase with its
  outcome; a claim with its reservation token; an attachment with its run.
- I-S2: no transaction spans a subprocess, prompt or network call (K14).
- I-S3: a read returns `Verified`, `Absent` or `Unknown`. A torn, oversized,
  non-regular or unparsable file is `Unknown(reason)`, never a default.
- I-S4: a failed write never destroys the previous generation.
- I-S5: the store records its schema version and the build that last wrote
  it. A binary that does not know the schema refuses Mutate and reads only
  the columns its version defines.
- I-S6: the vault, user-owned files and derived caches never enter
  `kernel.db`. They use `atomic_replace` under their store gate.
- I-S7: a corrupt derived cache is a miss, never a failure of the operation
  that wanted it.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill mid-transaction | SQLite rolls back. The intent was not committed, so its effect never started (K3). |
| Crash, Kill mid-replace | The target is the old or new file, never torn. A leftover temporary is removed by the next writer under the gate. |
| ENOSPC | The commit fails and the operation stops before its effect. A replace leaves the previous file intact. |
| Perm | Mutate refuses before any effect. Observe returns `Unknown(PermissionDenied)`. |
| Clock | Records carry wall time for display and journal sequence for order. Nothing compares wall times for authority. |
| Corrupt `kernel.db` | Observe reports `Unknown(store unreadable)`. Mutate refuses and names the explicit repair, which moves the file aside. Resources it described become `Unknown`, so R3 protects them. |
| Hot rollback journal seen by a read-only reader | `Unknown(store needs recovery)`; the next Mutate recovers it. |

#### Proof

- ENOSPC and Kill at each of the replace steps: the target parses as the old
  or the new generation.
- ENOSPC at every commit: no effect follows a failed intent commit.
- Vault and key-wrap mismatch after a fault at every step is impossible:
  readers always resolve one complete generation.
- Corrupt-cache fixtures read as a miss.

### 5.6 `Run` journal and `settle`

#### API

```rust
impl Run {
    pub fn begin(cx: &EffectContext, inv: &ResolvedInvocation, opts: BeginOpts)
        -> Result<Run, BeginError>;
    pub fn intend(&mut self, effect: EffectSpec) -> Result<EffectTicket, StoreError>;
    pub fn applied(&mut self, t: EffectTicket, receipt: Receipt) -> Result<(), StoreError>;
    pub fn failed(&mut self, t: EffectTicket, error: ErrorInfo) -> Result<(), StoreError>;
    pub fn unknown(&mut self, t: EffectTicket, reason: UnknownReason) -> Result<(), StoreError>;
    pub fn advance(&mut self, phase: Phase) -> Result<(), StoreError>;
    pub fn settle(self, cx: CleanupContext) -> Outcome<Settled>;    // consumes; releases gates
}

pub fn settle_interrupted(cx: &EffectContext, owner: &ResourceId, cleanup: CleanupContext)
    -> Outcome<Settled>;             // acquires the owner gate first

pub fn request(cx: &EffectContext, run: RunId, expected: Generation, action: RequestAction)
    -> Result<RequestId, RequestError>;   // stop, settle, detach

pub enum Settlement {
    Release(UndoSpec),               // idempotent undo: stop a group, withdraw a route,
                                     // release a claim, detach an attachment
    Reconcile(ReconcileSpec),        // observe, then the engine decides: release an
                                     // environment attachment per its policy
    Retain,                          // never undone: borrowed resources, persistent data
    Irreversible,                    // reported, never replayed: SQL import, git push
}

pub enum BeginError {
    Busy(GateError),
    PredecessorUnsettled { run: RunId, residue: Vec<Residue> },
    HostAuthorityRequired,
    Store(StoreError),
}
```

`begin` order:

| Step | Action | Durable state after |
| --- | --- | --- |
| 1 | Check class and placement. | none |
| 2 | `Gate::acquire(owner + inv.gates)`. | gates held |
| 3 | For every unresolved predecessor of the owner: a non-closed one is interrupted (the gate was free) and is settled now; a `Closed` one with unacknowledged residue gets a settle retry now. Both run under a cleanup budget. Residue left: refuse with `PredecessorUnsettled`, unless the caller passed an explicit residue acknowledgement. The acknowledgement is recorded on the predecessor and touches nothing; its residue stays listed and later settles keep retrying it. | every predecessor resolved |
| 4 | One transaction: `generation += 1`; insert the run, phase `Admitted`, owner identity, invocation. | `Admitted` |
| 5 | If custody is planned: spawn the sentinel (§5.8) and record its identity. | sentinel recorded |

Run phases:

| From | Trigger | Write before acting | To | Forbidden |
| --- | --- | --- | --- | --- |
| none | `begin` | gates held, run row | `Admitted` | any effect before the row commits |
| `Admitted` | first `intend` | event `Intended` | `Starting` | effect before intent |
| `Starting` | all startup obligations satisfied | phase | `Active` | `Active` with an unresolved required probe |
| `Admitted`, `Starting`, `Active` | stop request, signal, failure, natural end, deadline | cause | `Stopping` | new effects after `Stopping`, except settle's own |
| `Stopping` | settle resolves every effect | `Closed { workload, cleanup }` in one transaction | `Closed` | `Closed` with an effect still `Intended` or `Applied` |
| `Closed` with `Incomplete` | settle retry: the next `begin` on the owner, an explicit stop, or maintenance | updated cleanup | `Closed` with `Complete` or `Incomplete` | changing the workload outcome; admitting a successor while residue is unacknowledged |
| any non-closed, gate free | reader observation | none | `Interrupted` (derived, never stored) | a reader repairing it |

Effect states:

| State | Meaning | Settle action |
| --- | --- | --- |
| `Intended` | intent committed; outcome not recorded | observe the target first, as `Unknown` |
| `Applied` | receipt recorded | undo per `Settlement` |
| `Failed` | verified not to have happened | none |
| `Unknown` | attempted; outcome not observed | observe: `Absent` gives `Released`; `Verified` ours gives undo; `Verified` another owner's gives `Released` (superseded); `Unknown` gives `Residue` |
| `Released` | undone, or proven absent | none |
| `Retained` | `Settlement::Retain` | none |
| `Irreversible` | reported | none |
| `Residue` | undo failed or the target is `Unknown` | retried by the next settle; blocks `Complete` |

Settle steps: advance to `Stopping`; walk effects in reverse sequence,
recording each result before the next; commit `Closed`; close the sentinel
pipe; release the gates.

Status is a projection of the run row, the owner gate probe and fresh
bounded observations. History never stands in for current state.

| Phase | Gate | Workload | Rendered state |
| --- | --- | --- | --- |
| `Admitted`, `Starting` | held | — | `starting(phase)` |
| `Active` | held | — | `running` |
| `Stopping` | held | — | `stopping` |
| `Closed`, `Complete` | — | `Succeeded` | `succeeded`, or `stopped` for long-lived runs |
| `Closed`, `Complete` | — | `Failed`, `Cancelled`, `TimedOut` | `failed(code)`, `cancelled`, `timed_out` |
| `Closed`, `Incomplete` | — | any | `failed(cleanup)` with residue |
| non-closed | free | — | `interrupted` with the next command |
| row unreadable | any | — | `unknown(reason)` |

#### Invariants

- I-R1: intent before effect (K3), including setup steps, env-schema
  commands and gateway ensure, which today run before the lock.
- I-R2: one unresolved run per owner (K5). A `Closed` run with
  unacknowledged residue blocks the next `begin` exactly as a non-closed run
  does.
- I-R3: only the owner gate's holder writes a run's events, phase or outcome.
  Other processes only insert requests.
- I-R4: requests carry the expected generation. A stale one is refused
  `StaleGeneration` (K7). Requests replace marker files such as
  `stop.requested`.
- I-R5: settle is idempotent. Re-settling a `Released` effect does nothing.
  A crash mid-settle resumes at the first unresolved effect.
- I-R6: settle never runs under a cancelled context (I-CX4).
- I-R7: positive settlement (R5). `Retained` and `Irreversible` count as
  resolved; `Residue` and `Unknown` do not.
- I-R8: a workload failure and a cleanup failure are reported separately. A
  stop of a failed workload can settle cleanly.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill before `Admitted` commits | No row, no effect. Gates drop. |
| Crash, Kill after `intend`, before the effect | `Intended` with nothing behind it. Settle observes `Absent` and releases. |
| Crash, Kill after the effect, before `applied` | `Intended` with something behind it. Settle observes `Verified` and undoes, or `Unknown` and records residue. |
| Crash, Kill mid-settle | The next gate holder (sentinel, successor, explicit stop) resumes. |
| ENOSPC on `intend` | The effect is not started. Earlier effects are settled under the cleanup budget. |
| ENOSPC on `applied` or `failed` | The effect stays `Intended`. The run stops, and settle observes it. |
| ENOSPC on `Closed` | The gate is held until the commit succeeds or the cleanup deadline ends. Then the gate is released with the run non-closed, and the next holder settles it. |
| Perm | `begin` refuses before any effect. |
| Clock | Order is journal sequence. Persisted deadlines carry boot identity plus boot-monotonic time (§7.1). |

#### Proof

- Kill at every journal boundary of each scenario in §12.1: after recovery
  every run is `Closed` or reported `interrupted` with a next command.
- `settle` twice, concurrently, and after a crash mid-settle: same outcome,
  no double undo in strict-fake call logs.
- A stale stop request against a successor generation is refused.
- A non-closed predecessor and a `Closed` predecessor with `Incomplete`
  cleanup both get a settle attempt from the next `begin`. With residue left,
  `begin` refuses `PredecessorUnsettled`; with an acknowledgement it admits,
  touches nothing, and the residue stays listed and retried.

### 5.7 `OwnedProcess` and `SignalRouter`

#### API

```rust
pub struct ProcessSpec {
    argv: Vec<OsString>, cwd: PathBuf,
    env: EnvSpec,                      // explicit; secrets as SecretValue
    stdio: StdioSpec,                  // stdin, capture policy, terminal: None, Pty, Foreground
    grace: Duration,
    lifetime: Lifetime,                // Finite(Deadline) or Open(reason)
    label: String,
}

// Store runs publish engine-journalled file effects through
// Run::replace_record(dir, name, bytes, ReplaceOpts), retaining their owner gate.
// DirFd::prepare_child(cx, name) creates source directories no-follow and syncs
// their parent; SourceDirOpen/SourceDirSync are its fault boundaries.
impl Custody {
    pub fn spawn(run: &mut Run, spec: ProcessSpec, cx: &EffectContext)
        -> Result<OwnedProcess, SpawnError>;
    pub fn spawn_source(run: &mut Run, spec: ProcessSpec, cx: &EffectContext)
        -> Result<OwnedProcess, SpawnError>;               // Prepare: AcquireSource
    pub fn spawn_all(run: &mut Run, specs: Vec<ProcessSpec>, cx: &EffectContext)
        -> Result<Vec<OwnedProcess>, PartialSpawn>;          // rolls back on failure
}

impl OwnedProcess {
    pub fn identity(&self) -> &ProcessIdentity;   // pid, pgid, start identity, boot identity
    pub fn wait(&mut self, cx: &OperationContext) -> Observed<Exit>;
    pub fn stop(&mut self, cx: &CleanupContext) -> StopReport;
}

pub struct StopReport {
    leader: Observed<Exit>,
    group: Observed<Closure>,
    capture: CaptureState,             // complete, or truncated with reason
    signals: Vec<SignalDelivered>,     // only signals this process actually sent
    residue: Vec<Residue>,
}

impl SignalRouter {
    pub fn install(policy: RouterPolicy) -> Result<RouterHandle, KernelError>; // once, in main
}
impl RouterHandle {
    pub fn root_cancel(&self) -> CancelToken;
}
```

Spawn protocol:

| Step | Action | If the owner dies here |
| --- | --- | --- |
| 1 | `intend(Process { spec digest, label })` | `Intended`, nothing spawned |
| 2 | fork; the child calls `setpgid(0, 0)`, then blocks reading a barrier pipe in `pre_exec` | child reads EOF and `_exit`s before exec: no workload ran |
| 3 | parent records `{pid, pgid, start identity, boot identity}` as `Applied` | child reads EOF and exits; settle observes the group `Absent` |
| 4 | parent writes the go byte; the child execs the workload | settle stops the recorded group |

Every custody spawn sets `EFFIGY_CUSTODY_NONCE=<run>:<effect>` in the child's
environment; descendants inherit it. It is evidence, not a credential.

If step 3 fails (ENOSPC), the parent kills the blocked child and returns
`SpawnError::Unrecorded`. A later spawn failure in `spawn_all` stops every
earlier child and returns both the primary and the cleanup errors.

Owner stop. The owner holds the leader's `Child` and does not reap it until
group closure is verified. The leader's pid is the group id, so neither can
be reused while the owner signals. TERM plus CONT to the group, wait the
grace, KILL the group, verify that no member other than the unreaped leader
remains, then reap. `kill(-pgid, 0)` returning `ESRCH` proves the group
`Absent`. `EPERM` is `Unknown`. Leader exit alone is never closure. Output
EOF and exit are separate events; capture drains under its own deadline
after closure. `src/runner/host_process.rs:3114-3149` and `:3217` already
hold the unreaped root and refuse an unverified group; the regression at
`:4297` is the model.

Recovery stop. A sentinel or successor settles after the owner died; the
leader was reparented and its number is no longer pinned.

| Observation | Signal authority |
| --- | --- |
| Leader alive with the recorded start and boot identity | signal the group. On Linux 6.9 or later: open a pidfd, check identity, then call `pidfd_send_signal` with `PIDFD_SIGNAL_PROCESS_GROUP`, which targets the original group's kernel identity, not its number. Elsewhere (older Linux, macOS): a numeric group signal follows the identity check, with the window below. |
| Leader gone or identity differs, and `kill(-pgid, 0)` returns `ESRCH` | none needed: `Absent` |
| Leader gone or identity differs, and the group has members | signal each member individually, only if its environment carries the run's custody nonce and it started no earlier than the recorded spawn. On Linux, through a per-process pidfd opened before the check. Other members: `Unknown`, residue, never signalled. |
| Process table or member environment unreadable | `Unknown`, residue |

Where no descriptor operation exists (macOS for any signal; Linux before
6.9 for group signals), a window between the identity check and the numeric
signal remains. It is documented, as contract 049 documents macOS identity
limits, and closed nowhere by guessing. Holding a pidfd never reserves a
numeric pid or group id: once the last task using a number is gone, the
number can be reused while the descriptor stays open. An open pidfd
therefore never justifies a numeric `kill(-pgid)`.

Signal handling by placement:

| Signal | Inline owner | Client of a detached run | Detached supervisor | Sentinel |
| --- | --- | --- | --- | --- |
| INT | cancel the root context; forward INT to custody groups; settle | send `stop`; show progress until `Closed` | stop and settle | ignored |
| TERM | cancel; forward TERM; settle | send `stop` | stop and settle | ignored |
| HUP | as TERM | as TERM: client death means stop | ignored (no terminal) | ignored |
| second INT or TERM within 2 s | KILL custody groups now, then settle | forwarded as escalation | escalate to KILL | — |

Exit codes: cancelled is 128 plus the signal number (INT 130, TERM 143,
HUP 129). A `Foreground` child receives the terminal with `tcsetpgrp`; the
kernel restores it on exit.

#### Invariants

- I-P1: the owner exists before the first spawn. `spawn` takes `&mut Run`,
  which exists only after `Admitted` (K6).
- I-P2: no workload executes without a durable custody record. The barrier
  enforces this.
- I-P3: the owner keeps the leader unreaped, and the group retained, until
  closure is verified, not until the leader exits.
- I-P4: signals need positive generation evidence: the owner's unreaped
  leader; in recovery, the leader's verified start and boot identity, or a
  member's custody nonce plus start time. A non-empty group or session is
  never ownership evidence. No pattern kills (`pkill -f`).
- I-P5: capture is bounded (tail ring plus rotating log) and redacted at the
  source, before any sink (K12).
- I-P6: secrets never appear in argv; no `set_var`.
- I-P7: one router per process (K9). Libraries take `CancelToken`. Rhai polls
  the token and never installs a handler.
- I-P8: a process that creates a new session escapes the group, and one that
  clears its environment loses the nonce. Either is outside custody in
  recovery and is reported, never signalled by number. Guest-placement
  Effigy never creates sessions or clears the nonce.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill of owner before the go byte | The child exits on barrier EOF. No workload ran. |
| Crash, Kill of owner after the go byte | The sentinel or the next gate holder stops the recorded group by identity. |
| SIGSTOP of a child | CONT follows TERM; KILL works on stopped processes. |
| ENOSPC on the custody record | Child killed before exec; `Unrecorded`. |
| ENOSPC on logs | Capture continues in the ring; the log is marked truncated. The outcome is unchanged (K10). |
| Perm (`EPERM` on signal, for example a setuid child) | `Unknown` residue; no escalation by other means. |
| Clock | Grace uses `Instant`. |

#### Proof

- `custody_prepare_source_requires_its_class_and_settles_with_a_sentinel`
  qualifies Prepare acquisition through `Custody::spawn_source`: successful,
  failed and deadline-expired children settle through the registered sentinel.
  Prepare cannot use ordinary `spawn`, and Mutate cannot use `spawn_source`.
  Both entry points share the custody fault points and recovery machinery.
- SIGINT, SIGTERM, SIGHUP and SIGKILL to the client, crossed with plain,
  sequence, managed and container runs: settle reports `Complete` and no
  fixture-tagged process survives (O1a).
- A failure spawning child 2 of 3 leaves no survivor and reports both errors.
- A descendant that ignores TERM and keeps the stdout pipe open is killed,
  and capture ends at its drain deadline.
- Reused group: the process-table observer is a kernel trait, so the
  harness can show the recorded pgid occupied by a foreign group without the
  nonce after the leader is gone. Recovery signals nothing and reports
  residue. The same holds for a foreign process sharing a pgid with a
  surviving nonce-carrying member.
- Leader exits while a nonce-carrying descendant survives: owner stop kills
  it through the group, whose number the unreaped leader keeps allocated;
  sentinel recovery kills it through the nonce.
- Recovery verifies a live leader, which then exits, and a foreign group
  takes the number before the signal (injected): with the group capability
  nothing foreign is signalled. Without it, the case is the documented
  window and is reported as such.
- A descendant that clears its environment survives recovery as `Unknown`
  residue and is never signalled by group number.
- No secret bytes appear in `ps -o args` during any run.
- A Rhai script cannot swallow a stop.

### 5.8 Placement, supervisor and sentinel

#### API

```rust
pub enum Placement { Inline, Detached, Guest }

pub enum ClientMode { Owning, Detach }

pub fn start_detached(cx: &EffectContext, inv: &ResolvedInvocation, client: ClientMode)
    -> Result<SupervisorClient, BeginError>;

impl SupervisorClient {
    pub fn events(&self) -> EventStream;      // journal events and log frames
    pub fn stop(&self) -> Result<(), RequestError>;
    pub fn detach(self) -> Result<RunId, RequestError>;
}
```

| Placement | Owner process | Used by | Owning-client death | Owner SIGKILL |
| --- | --- | --- | --- | --- |
| `Inline` | the invoking process | one-shot runs: tasks, sequences, tests, QA groups, delivery runs | not applicable | the sentinel settles |
| `Detached` | one `__kernel-supervise` process per run, in its own session | managed runs; environment-owned runs (sidecars) | stop and settle, unless the client detached | the sentinel settles |
| `Guest` | none: the guest workspace client owns no run; each admitted request is an `Inline` or `Detached` host run (§8.3) | the container-mode workspace client (§8) | its host runs follow their own placement | its host runs follow their own placement |

Detached startup:

| Step | Actor | Durable write | On failure |
| --- | --- | --- | --- |
| 1 | client resolves the invocation (Observe) | none | error; nothing happened |
| 2 | client spawns the supervisor through the kernel and hands it the invocation on a pipe | none | spawn error; nothing happened |
| 3 | supervisor runs `Run::begin` | run `Admitted` | `Busy` or `PredecessorUnsettled` sent to the client; supervisor exits |
| 4 | supervisor commits socket intent, binds `sock/<run-id>.sock` (0600, peer-uid checked), records its receipt and sends `admitted` | supervisor identity | pipe EOF before `admitted`: settle with nothing applied, `Closed(NotStarted)` |
| 5 | supervisor performs effects | events | failure: `Stopping`, settle |
| 6 | the owning client renders (TUI or text). `--detach` sends `detach` and exits once `Active`. | request `detach` | owning-client EOF without detach: `stop` request, settle |
| 7 | another process stops the run: request row, then a socket notice | request | socket unreachable and gate free: the requester settles as gate holder |

Secondary clients (a later attach) never carry stop authority by dying.
Tom's ruling 5: an owning client's death, including terminal close and an
agent's SIGKILL, means stop and settle; `dev` attaching to an already-running
stack borrows it and leaves it running (§5.9).

Sentinel: a per-run `__kernel-sentinel` process, in its own session, holding
the read end of a pipe whose write end only the owner holds. It is spawned
after `Admitted` whenever a run's custody is non-empty, in either placement.

| Event | Sentinel action |
| --- | --- |
| Owner commits `Closed`, closes the pipe | EOF; reads `Closed`; exits |
| Owner dies | EOF; `Gate::acquire(owner, Until(30 s))`; if acquired and the run is not closed: settle, release, exit; if `Busy`, exit (the holder settles) |
| Sentinel dies first | The owner is unaffected. On later owner death the next gate holder settles. Status names the gap. |
| Owner stopped (SIGSTOP) | No EOF; nothing happens; the gate stays held. |

#### Invariants

- I-SV1: one supervisor per detached run. It is the run's only writer.
- I-SV2: per-run processes live exactly as long as their run plus its settle;
  a probe guard lives as long as its observer's open probes. There is no
  resident daemon other than the user gateway.
- I-SV3: front ends (TUI, attached text, detached) read the same events and
  logs. A front end never changes topology or semantics (K11).
- I-SV4: the supervisor and sentinel re-exec the binary identified at
  startup; a replaced binary on disk is detected and refused.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Supervisor Crash or Kill | The sentinel settles. |
| Supervisor and sentinel both Crash or Kill | Status shows `interrupted`; the next Mutate on the owner, or `stop`, settles. |
| ENOSPC on the socket path | The admitted run launches no workload and settles with nothing applied: `Closed(NotStarted)`. If closing also fails, the run reads Interrupted under §5.6 recovery. |
| Perm on the socket | The peer-uid check refuses. The request row remains; the supervisor reads requests when it polls the store. |
| Clock | No time authority. |

Planner ruling (effigy-a6, kernel lane owner, 2026-10-19): admission precedes socket bind, because K3 requires intent before effect.

#### Proof

- The full supervisor matrix in §12.3, including owning-client SIGKILL, owner
  SIGKILL, terminal close, and detach then stop.
- A sentinel on a SIGKILLed inline run settles within its deadline.
- A secondary client's death stops nothing.

### 5.9 Attachments and claims

#### API

```rust
pub enum AttachMode { Borrow, Exclusive { stop_on_release: bool } }

impl Attachments {
    pub fn attach(run: &mut Run, env: &ResourceId, mode: AttachMode, cx: &EffectContext)
        -> Result<AttachmentTicket, AttachError>;  // takes Gate(Env) for one transaction;
                                                   // an effect with Reconcile settlement
    pub fn release(run: &mut Run, ticket: AttachmentTicket)
        -> Result<(), StoreError>;                 // no Gate(Env); only removes protection
    pub fn holders(cx: &ObserveContext, env: &ResourceId)
        -> Vec<(RunId, Observed<Liveness>)>;
    pub fn permit<'g>(env_run: &'g Run /* owner: Env, holds Gate(Env) */,
                      exempt: Option<RunId>, cx: &EffectContext)
        -> Result<TransitionPermit<'g>, HeldBy>;   // R4; cannot outlive env_run's gate
}

pub enum AttachError {
    EnvBusy { transition: Observed<RunId> },       // Gate(Env) not acquired by the deadline
    EnvNotActive { state: EnvState },              // ask the environment engine to ensure
    StaleGeneration { expected: Generation, current: Generation },
    HostAuthorityRequired,
    Store(StoreError),
}

pub enum ClaimState { Pending, Committed, Releasing, Quarantined { reusable_at: BootInstant } }

impl Claims {
    pub fn reserve(txn: &mut Txn, holder: &ResourceId, run: RunId, wanted: &[ResourceId])
        -> Result<ReservationToken, Conflict>;             // all or none, one transaction
    pub fn commit(txn: &mut Txn, token: ReservationToken, revision: u64) -> Result<(), StoreError>;
    pub fn release(txn: &mut Txn, token: ReservationToken, quarantine: Duration)
        -> Result<(), StoreError>;
}
```

Attach and transition serialize on `Gate(Env)`:

| Step | Attaching run | Environment run (down, restart, recreate, reset, handover) |
| --- | --- | --- |
| 1 | Acquire `Gate(Env)` under its deadline, while holding its own owner gate. | Holds `Gate(Env)` as its owner gate for the whole run. |
| 2 | One transaction: the environment is `Active` at generation g; insert the attachment `{run, g}` as an `Applied` effect. | Request every losing holder to settle; wait for each to resolve, still holding `Gate(Env)`. |
| 3 | Release `Gate(Env)`. | `permit`: issued only when every holder is `Absent`, or the only holder is the exempt run settling itself. |
| 4 | — | Each destructive effect's intent transaction re-checks holders and the generation, then the adapter runs. |
| Refusal | `EnvBusy` at the deadline. `EnvNotActive`: request an ensure; the environment's apply run inserts the attachment in its final transaction. `StaleGeneration`: retry from step 1. | `HeldBy` lists live and unknown holders. |

No attachment can appear between permit and mutation, because inserting one
needs the gate the environment run holds. Releasing an attachment needs no
gate: it only removes protection. Any policy a release triggers, such as
stop on release, is a request to the environment engine, processed by an
environment run. A holder therefore settles while a transition holds
`Gate(Env)`, and the two never wait on each other.

Holder liveness:

| Holder run | Owner gate | Liveness | R4 |
| --- | --- | --- | --- |
| non-closed | held | `Verified(live)` | protects |
| non-closed | free | `Interrupted` | protects until settled; the transition settles it first, as gate holder |
| `Closed`, residue unacknowledged | any | `Unknown` | protects |
| resolved, attachment released | any | `Absent` | permits |
| row or gate unreadable | any | `Unknown` | protects |

#### Invariants

- I-A1: `TransitionPermit` is the only key to an environment-mutating
  adapter call. It exists only when every holder is `Absent`, or the only
  holder is the exempt run settling itself (R4). Attach and permit serialize
  on `Gate(Env)`: an attachment is inserted only under that gate, a permit
  borrows the environment run that holds it, and each destructive effect's
  intent transaction re-checks holders and generation.
- I-A2: a down or handover keeps `Gate(Env)` while it asks each losing
  holder to settle and waits for it to resolve. Holders settle without
  `Gate(Env)`. An interrupted holder is settled by taking its owner gate with
  `FailFast`. `Busy` means a successor holds that gate and settles the holder
  first (K5), so the transition keeps waiting within its deadline. An
  environment run never waits on a holder's owner gate.
- I-A3: claims reserve the complete conflict set (data, domains, addresses,
  ports) in one transaction before any effect. A pending claim is protected
  while its holder's gate is held; reclaim needs that holder `Absent`.
- I-A4: a released endpoint claim is quarantined for the published DNS time
  to live plus the connection drain budget, measured on the boot clock. A
  reboot ends quarantine.
- I-A5: the release policy (`stop_on_release`, idle stop, persistence) is the
  environment engine's. The kernel only guarantees the holder protection.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill of a holder | Its attachment stays; liveness becomes `Interrupted`; R4 holds until settle. |
| Crash, Kill mid-reservation | The transaction rolls back; nothing reserved. |
| ENOSPC | Reservation or attach fails before the effect. |
| Perm | Reservation refuses before any effect. Holder observations become `Unknown`, which protects (R4). |
| Clock | Quarantine uses the boot clock, so wall jumps cannot shorten it. |

#### Proof

- Explicit `container down`, failed-run recovery, idle stop and handover
  under a live holder all refuse. The strict fake records zero down calls.
- A holder whose liveness is injected `Unknown` blocks every transition.
- Two concurrent stack and hybrid starts sharing data: exactly one commits
  the data claim.
- The strict fake stalls an environment run between permit and adapter
  apply. A concurrent attach waits, then returns `EnvBusy` or attaches after
  the transition. No destructive call ever runs with an attachment present.
- A holder settles and releases its attachment while a transition holds
  `Gate(Env)`; neither waits on the other.
- An address released and immediately requested is refused until
  quarantine ends.

### 5.10 Remote custody

#### API

```rust
pub trait RemoteExecAdapter {
    fn identity(&self, target: &ContainerTarget, cx: &OperationContext)
        -> Observed<ContainerIdentity>;         // full id, created, VM boot id
    fn launch(&self, id: &ContainerIdentity, cmd: RemoteCommand, cx: &EffectContext)
        -> Result<ProcessSpec, AdapterError>;   // the local exec client to spawn
    fn signal_session(&self, reg: &RemoteRegistration, sig: Signal, cx: &CleanupContext)
        -> Observed<SessionState>;
    fn observe_session(&self, reg: &RemoteRegistration, cx: &OperationContext)
        -> Observed<SessionState>;
    fn find_by_nonce(&self, id: &ContainerIdentity, nonce: &Nonce, cx: &OperationContext)
        -> Observed<Option<RemoteRegistration>>;
}

pub struct RemoteRegistration {
    env: (ResourceId, Generation),       // fencing (K7)
    container_id: FullContainerId, vm_boot_id: String, pid_ns: u64,
    sid: u32, leader_pid: u32, leader_start_ticks: u64, nonce: Nonce,
    session_isolated: bool,
}
```

The remote wrapper is fixed POSIX `sh`, with no Python. It runs the workload
in a new session with `EFFIGY_CUSTODY_NONCE` set, prints one framed
registration line on stderr, then execs the workload:

```text
\x1eEFFIGY-CUSTODY v1 <nonce> <pid> <sid> <start-ticks> <vm-boot-id> <pid-ns>\n
```

| Step | Action | Durable state | Recovery if the owner dies |
| --- | --- | --- | --- |
| 1 | `identity()` must be `Verified`; `Unknown` refuses the launch | none | — |
| 2 | `intend(RemoteProcess { container id, nonce })` | `Intended` | settle runs `find_by_nonce` |
| 3 | spawn the local exec client under `OwnedProcess` | client recorded | settle stops the client, then `find_by_nonce` |
| 4 | read the registration line within the ack deadline; strip it from output | `Applied` with registration | settle signals the session |
| 5 | the workload counts as running only now | — | — |

Stop: `signal_session(TERM)`, CONT, grace, `KILL`, then `observe_session`.
`signal_session` signals the whole session only while the leader is alive
with the registered start ticks. Otherwise it signals only session members
whose environment carries the nonce and whose start ticks are not earlier
than the leader's (I-P4). Other members are `Unknown`.
`Absent` proofs: the container id is missing from a `Verified` inventory; the
VM boot id changed; or no process in the session remains and the leader pid
is gone or shows different start ticks. Exec failure is `Unknown`, never
`Absent`.

#### Invariants

- I-RC1: no bind-mount marker files. Registration travels in band, and the
  nonce makes an unacknowledged launch findable (R1, D9).
- I-RC2: a registration binds one container id and one VM boot. Cleanup never
  transplants a pid or session number into a replacement container, and a
  session number in use is never ownership evidence.
- I-RC3: local exec-client exit is never remote closure. The session is
  observed separately (K6).
- I-RC4: without `setsid` in the image, `session_isolated = false` and closure
  covers the leader's group and verified descendants. Anything else is
  `Unknown`.
- I-RC5: adapters run the stop helper inside the registered container only,
  with a deadline per call.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Owner Crash or Kill before the ack | `Intended` with the nonce; settle finds the session by nonce, or proves it `Absent`. |
| Owner Crash or Kill after the ack | settle signals the registered session. |
| VM restart (Colima, Docker Desktop) | VM boot id differs: `Absent`, `Released`. |
| Container recreated | old id missing from a `Verified` inventory: `Absent`. |
| Backend hung | `Unknown` within the deadline; residue. Nothing is restarted. |
| ENOSPC on host | The ack record fails: stop the client, `find_by_nonce`, then settle. |
| Perm in the guest | `Unknown`; residue. |
| Clock drift between host and VM | No cross-boundary time comparison. |

#### Proof

- A conformance suite every backend adapter must pass: kill at each step,
  container replaced, VM restarted, image without `setsid`, exec client
  killed with the remote still running, and a recorded session number reused
  by a foreign session without the nonce, which is never signalled.
- Real-engine qualification on Colima (docker and containerd), Docker Desktop
  and Linux Docker. Apple Containers must pass before it is supported.

### 5.11 `Observed<T>` and `Probe`

#### API

```rust
pub enum Observed<T> {
    Verified(T, Evidence),
    Absent(AbsenceProof),
    Unknown(UnknownReason),
}

pub enum AbsenceProof {
    NoGateInode, GateFreeRunClosed, BootChanged { recorded: String, current: String },
    NoSuchProcess { pid: u32 }, StartIdentityDiffers { pid: u32 }, GroupEmpty { pgid: u32 },
    NotInInventory { inventory: InventoryRef }, ContainerGone { id: FullContainerId },
}

pub enum UnknownReason {
    TimedOut { phase: String }, PermissionDenied, ToolUnavailable(String),
    Unparsable(String), Inconsistent(String), GuardUnavailable, Injected,
}

pub trait Probe {
    fn observe(&self, cx: &ObserveContext) -> Observed<Readiness>;
}

pub enum Readiness {
    Created, ExecReady, TcpOpen, SocketOwned { pid: u32 },
    HttpReady { status: u16 }, LogMatched, Healthy, EndpointApplied { revision: u64 },
}

pub fn wait_all(probes: &[&dyn Probe], cx: &ObserveContext) -> Vec<Observed<Readiness>>;

impl ObserveContext {
    pub fn probe_process(&self, cmd: ProbeCommand) -> Observed<ProbeOutput>;  // guarded
    pub fn connect(&self, target: ObservationTarget) -> Observed<ProbeIo>;    // non-mutating
}

pub struct ProbeCommand {
    argv: Vec<OsString>, cwd: PathBuf, env: EnvSpec,
    deadline: Duration, max_output: usize,    // stdin is null; no terminal
}

pub enum ObservationTarget {
    Tcp(SocketAddr),
    Http { url: Url, method: HttpReadMethod, expect: StatusSet },   // GET or HEAD, no body
    UnixSocket(PathBuf),                                            // e.g. a control socket
    Dns { name: String, server: SocketAddr },
}
```

Observation capability. Observe, RefreshDerived, Prepare and Mutate contexts
all produce an `ObserveContext`; Observe commands get nothing else.

| Capability | Allowed | Not allowed |
| --- | --- | --- |
| Processes | `probe_process`: an adapter-declared, non-mutating command, under a probe guard | `Custody::spawn`, a `Run`, a journal entry, a terminal |
| Network | `connect`: local TCP connect, HTTP `GET` or `HEAD` without a body, Unix control sockets, DNS to the gateway; targets come from the resolved invocation, the store or endpoint claims; bounded response and per-attempt deadline | source acquisition (fetch, clone, pull), any other method, any target not derived from Effigy state |
| Files and state | read-only opens; gate probes | any write, gate acquisition, store transaction |

Probe custody. The observer is the probe's parent and primary owner. One
`__kernel-probe-guard` per observing process (the same binary, in its own
process group, connected by a pipe) is the backup owner. Neither relies on a
parent-death signal, which covers only the immediate child and is cleared on
fork.

| Step | Observer | Guard | If one dies at this step |
| --- | --- | --- | --- |
| 1 | On its first probe, spawn the guard. | Wait for registrations. | — |
| 2 | Fork the probe into a new process group, blocked on a spawn barrier. Its environment carries `EFFIGY_PROBE_NONCE`, which names the observer's and the guard's pid and start identity, plus a sequence number. | — | Observer dies: the probe reads barrier EOF and exits without executing. |
| 3 | Send the probe's pid, group, start identity, nonce and deadline to the guard. | Record them in memory. On Linux, open a pidfd for the blocked leader and test `pidfd_send_signal(pidfd, 0, NULL, PIDFD_SIGNAL_PROCESS_GROUP)`: success records the group capability; `EINVAL` or `ENOSYS` records it as unavailable. Acknowledge, stating the capability. | No acknowledgement: the observer kills the unexecuted probe and returns `Unknown(GuardUnavailable)`. |
| 4 | On the acknowledgement, release the barrier. The probe executes. | — | — |
| 5 | Keep the leader unreaped: a zombie still holds its pid, so the group number stays allocated. Relay bounded output, kill the group at the deadline, verify closure, reap, then tell the guard the probe closed. | At the deadline plus grace, stop any registered probe still open: the backup for a hung or stopped observer. | Guard dies: the observer still owns and stops the probe; the outcome carries a warning. Observer dies: the guard reads EOF and stops every registered open probe. |

The guard is not the probe's parent, so it stops a probe by the recovery
rule of I-P4, limited to the generation evidence it holds:

| Process | Guard's evidence | Guard may signal |
| --- | --- | --- |
| Group member, Linux with the group capability | the registration pidfd: `PIDFD_SIGNAL_PROCESS_GROUP` reaches exactly the original group's members, even after the leader exits, and returns `ESRCH` once none remain | the original group, by descriptor only |
| Group member, macOS or Linux without the capability, leader alive with the registered start identity | verified leader (on Linux, the registration pidfd also shows whether it has exited) | the whole group by number, right after the check, with the documented window |
| Group member, macOS or Linux without the capability, leader gone | the probe nonce and a start no earlier than the registration | that member only (on Linux, through a per-process pidfd) |
| Process that left the group (new session) | the probe nonce and start time | that process only |
| Anything else | none | nothing |

If the observer and the guard both die, the probe is outside custody. Its
nonce names two owners whose absence the kernel can prove, so the kernel's
maintenance step (Mutate class or the explicit maintenance command, never
Observe) stops nonce-carrying processes by nonce and start time.

| Who lives | Stopped within the bound | Outside the bound |
| --- | --- | --- |
| Observer | every group member, at the deadline (the unreaped leader keeps the group number allocated); nonce-carrying processes that left the group | a process that left the group and cleared its environment |
| Guard only | Linux with the group capability: every member of the original group, plus nonce-carrying processes outside it, at the deadline plus grace. macOS and Linux without it: the whole group while the leader lives; after that, nonce-carrying processes. | Without the group capability: a group member without the nonce after the leader exits. Everywhere: a process that left the group and cleared its environment. |
| Neither | nonce-carrying processes, at the next maintenance step | any process without the nonce |

Processes outside the bound are never signalled (I-P4, R3). Observe writes
nothing, so it cannot record them as residue; the limit is stated here, not
hidden. Probe commands are adapter-declared, non-mutating, short-lived tools,
so a probe that escapes its group and clears its environment is a defect in
that declaration.

Reconciliation with K3 and R2. A probe is not an effect: it changes nothing
Effigy owns and uses only non-mutating commands and requests, so it needs no
intent (K3) and leaves Observe pure (R2), with no file, store, gate or
journal write. The registration lives only in the guard's memory. Contexts
of every other class use the same two calls for their probes.

#### Invariants

- I-O1: destructive APIs take `Absent` or `Verified<Owned<_>>` witnesses.
  There is no constructor from `Unknown` (R3).
- I-O2: a probe observes only. It has no write capability and never repairs,
  restarts or allocates. Its processes and I/O go only through
  `probe_process` and `connect`.
- I-O6: a probe executes only after the guard acknowledges its
  registration. Every probe process inside the bound for whichever owner
  lives (table above) is stopped within that bound. A process without
  generation evidence is never signalled and is outside the bound; Observe
  cannot record it. If both owners die, maintenance stops nonce-carrying
  processes. Probe commands are declared by adapters as non-mutating and
  verified by the read-only fixture.
- I-O3: `wait_all` is one waiter with one deadline over an ordered list.
  Dependencies are typed facts, never sleeps.
- I-O4: HTTP probes declare expected statuses (default 200–299), with a
  per-attempt connect and total timeout.
- I-O5: ready means every required predicate holds for the exact generation.
  A completed command exit is not readiness.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill of the observer | The guard reads EOF and stops every registered open probe within its evidence (group descriptor, verified leader or nonce). Processes without evidence are left alone. Nothing was written, so nothing needs recovery. |
| Crash, Kill of the guard | The observer still owns each probe through its unreaped leader and kills the group at the deadline. New probes return `Unknown(GuardUnavailable)` until a new guard acknowledges. |
| Crash, Kill of both | The probe is outside custody. Maintenance stops nonce-carrying processes; any others stay untouched. |
| SIGSTOP of the observer | The guard stops open probes at the deadline plus grace. |
| ENOSPC | Probes write nothing. |
| Probe timeout | `Unknown(TimedOut)`; other findings are still reported. |
| Tool missing | `Unknown(ToolUnavailable)`. |
| Perm | `Unknown(PermissionDenied)`. |
| Clock | Probes use deadlines only. |

#### Proof

- `Unknown` injected into every observation (§12.1): nothing is killed,
  downed, reclaimed, displaced, reused or restarted.
- An HTTP 401, 404 or 500 is not ready by default.
- On a permission-denied state root and checkout, a
  `git --no-optional-locks status` probe and an HTTP `GET` probe both
  complete with an empty filesystem diff.
- On macOS and on Linux, each with a probe that hangs and one that forks a
  descendant which outlives its leader:
  - guard SIGKILLed: the observer stops the probe and the descendant at the
    deadline;
  - observer SIGKILLed: the guard stops both within the deadline plus grace;
  - observer SIGSTOPped: the guard stops both at the deadline plus grace;
  - guard dead before acknowledging: the probe never executes, and the
    result is `Unknown`;
  - both SIGKILLed: the probe survives until maintenance, which stops it by
    nonce, while a foreign process with another or no nonce survives.
- Observer SIGKILLed after the leader exits, leaving a descendant in the
  group that cleared its environment: on Linux with the group capability the
  guard stops it through the group descriptor; on macOS, and on Linux
  without the capability, it is left untouched and is outside O14's bound.
- Group capability unavailable (Linux before 6.9, or `EINVAL` injected at
  registration): the guard uses only the fallback rules and never sends a
  numeric group signal after the leader exits.
- The original probe group empties and a foreign group takes its number
  while the guard still holds the old pidfd: the descriptor signal returns
  `ESRCH`, and the foreign group survives.
- A descendant that leaves the group and clears its environment is never
  signalled, on either platform, by any owner.
- Every probe case leaves an empty filesystem diff.

### 5.12 `Outcome`

#### API

```rust
pub struct Outcome<T> {
    result: Result<T, ErrorInfo>,
    workload: WorkloadOutcome,
    cleanup: CleanupOutcome,
    effects: Vec<EffectReport>,
    residue: Vec<Residue>,              // kind, id, observation, next command
    next: Vec<Invocation>,              // parser-valid commands
    warnings: Vec<Warning>,             // observability failures (K10)
}

pub enum WorkloadOutcome {
    NotStarted, Succeeded, Failed { code: i32 },
    Cancelled { cause: StopCause, signal: Option<i32> },
    TimedOut { phase: String }, Interrupted, Unknown(UnknownReason),
}

pub enum CleanupOutcome { NotNeeded, Complete, Incomplete }

pub struct ErrorInfo { code: ErrorCode, message: String, details: serde_json::Value }
```

| Classification | Exit code |
| --- | --- |
| `Succeeded`, cleanup complete | 0 |
| `Failed { code }` | `code` |
| `Cancelled` by signal n | 128 + n |
| `TimedOut` | 124 |
| `Busy` at a gate | 75 |
| successful workload, cleanup `Incomplete` | 1 |

Engines keep their current exit-code contracts until their lane changes
them; the table governs new kernel-backed surfaces.

#### Invariants

- I-OC1: `Succeeded` only on natural completion of every required obligation:
  exit 0 for each required process and `Applied` for each required effect
  (R5).
- I-OC2: `Cancelled` and `Unknown` are first-class; neither becomes a pass.
- I-OC3: one output owner renders the outcome. JSON carries the same facts as
  text: the report in `error.details`, never a string inside
  `error.message` (K11).
- I-OC4: every `next` entry parses under the current grammar.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Report write ENOSPC or Perm | A warning; the outcome and exit code are unchanged (K10). |
| Crash or Kill during rendering | The run is already `Closed`; status re-renders it. |
| Clock | Outcomes carry durations from `Instant`; wall times are display only. |

#### Proof

- Every `next` invocation in fixtures parses.
- ENOSPC on every report write leaves the exit code unchanged.
- Text and JSON for one outcome carry identical facts.

## 6. Failure model summary

| Fault | Kernel guarantee | Not claimed |
| --- | --- | --- |
| Crash or Kill anywhere | Intent before effect means every effect is findable; one settle resolves it later. | That the effect did not happen. |
| ENOSPC | Every write fails before its effect or preserves the previous generation. | That work in flight completes. |
| Perm | Refuse before effects; observations become `Unknown`. | That an `Unknown` resource is safe to act on. |
| Clock jump, suspend | No authority from wall time; gates have no time; quarantine uses the boot clock. | Accurate displayed timestamps. |
| SIGSTOP of a holder | Never stolen; waiters time out naming it. | Progress. |
| External boundaries | At-least-once attempts with idempotent undo, inspect-and-reconcile, or a recorded `Irreversible` or `Unknown`. | Exactly-once delivery across filesystem, engine or network. |

## 7. Store layout

### 7.1 Location, trust and clocks

- Root: `~/.effigy/state/`, from the existing Effigy home (`$HOME/.effigy`).
  It is outside every checkout, so it survives worktree deletion and is
  never inside a guest's bind mount.
- Trust: the root and every component are real directories owned by the
  operator, mode 0700; files 0600. A symlinked or group- or other-writable
  component refuses Mutate (K13).
- Filesystem: local only. `statfs` rejects NFS, SMB, WebDAV, FUSE and
  sshfs; Mutate is refused, Observe allowed.
- Clocks: wall time is display only. Persisted deadlines and quarantine use
  `{boot identity, boot-monotonic nanoseconds}` (`CLOCK_BOOTTIME` on Linux,
  `mach_continuous_time` on macOS). A different boot ends both.
- Test isolation uses a fresh `HOME`, as the fresh-HOME proofs require; no
  new environment variable.

```text
~/.effigy/state/
  kernel.db                         runtime store (+ kernel.db-journal during commits)
  gates/<scope>.scope               scope gate
  gates/<scope>/<rid>.gate          resource gates
  sock/<run-id>.sock                per-run supervisor control socket
  runs/<run-id>/<stream>.log        redacted, rotating logs
  objects/<sha256>/                 immutable closures (renders); read-only once published
  staging/<run-id>/                 staging owned by one run; removed by its settle
```

### 7.2 Tables

| Table | Key | Columns | Writer |
| --- | --- | --- | --- |
| `meta` | key | schema version, created by, last written by | kernel |
| `checkouts` | token | kind, path hint, Git dir hint, state (live, tombstoned), first seen, last seen, tombstoned at | kernel, Mutate |
| `resources` | resource name | canonical bytes, kind, scope, label, authority engine, generation, desired, last applied, state, updated at | authority, under the resource gate |
| `runs` (operations) | run id | owner, generation, kind, placement, phase, workload outcome, cleanup outcome, invocation and digest, owner, supervisor and sentinel identities, parent run, stop cause, residue acknowledgement, timestamps | owner gate holder |
| `events` (journal) | run id, sequence | effect id, kind, target, settlement, state, receipt, error, deadline, wall time | owner gate holder |
| `attachments` | environment, run | environment generation, mode, state | the attaching run; settle |
| `claims` | claimed resource | holder, run, reservation token, state, generation, revision, quarantine end, details | owning engine, one transaction |
| `requests` | request id | run, expected generation, action, cause, requester identity, state, created, resolved | any Mutate inserts; the owner resolves |
| `objects` | digest | kind, path, size, referencing resources | engines through the object API |
| `logs` | run, stream | path, bytes, truncated, redaction policy | owner |

### 7.3 Path families

The audit counts about 111 persistent families today. The target is 16.
Kernel-owned families are 1–6; the kernel supplies the persistence contract
for the rest.

| # | Family | Location | Owner | Primitive | Retention | Guest-visible |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Runtime store | `state/kernel.db` | kernel | `Store` | migrated, never deleted | no |
| 2 | Gates | `state/gates/` | kernel | `Gate` | scope removal only (§7.4) | no |
| 3 | Control sockets | `state/sock/`; gateway socket in `~/.effigy/gateway/` | supervisor; gateway | supervisor; gateway lane | removed at run close | no |
| 4 | Immutable objects | `state/objects/` | environment engine | `Store` objects | referenced, plus the last 3 superseded per environment | read-only through a backend mount only |
| 5 | Operation staging | `state/staging/` | kernel | `Run` | removed by the owning settle | no |
| 6 | Run logs | `state/runs/` | owner run | `OwnedProcess` capture | 8 MiB per stream, 2 rotations; 2 GiB total, oldest settled first | no |
| 7 | Leaf certificates | `~/.effigy/gateway/certs/` | gateway | `atomic_replace` | reissued on expiry or CA change | no |
| 8 | Preserved CA | mkcert `CAROOT` | external | read only | never touched | no |
| 9 | Pinned sources | config lane decides | config | `Gate(Store bundles)` + `atomic_replace` | while pinned | no |
| 10 | Private vault | `<checkout>/.effigy/secrets/local.vault` (+ `.prev`) | secrets | `Gate(Store vault)` + pointer commit | previous generation kept | no: masked from the workspace mount (§8.2) |
| 11 | Key wraps | secrets lane decides | secrets | pointer commit with the vault | with the vault generation | no: masked from the workspace mount when beside the vault (§8.2) |
| 12 | Machine preferences | `~/.effigy/config.toml` | config | `atomic_replace` | user-owned | no |
| 13 | Graph store | `<checkout>/.effigy/graph/` | graph | RefreshDerived, `Gate(Store graph)` | per worktree | read |
| 14 | Other derived caches | per owner | owners | RefreshDerived; corrupt is a miss | owner-defined | — |
| 15 | Reports and exports | explicit paths | engines | `atomic_replace` | user-owned | — |
| 16 | Platform setup outputs | `/etc/resolver/`, launchd, completion | setup adapters | ownership verified before change (R3) | until removed by setup | no |

### 7.4 Retention

| Item | Kept | Removed |
| --- | --- | --- |
| Runs | every unsettled run; the last 20 settled per owner | older settled runs, with their events and requests |
| Logs | per §7.3 caps | oldest settled runs first |
| Objects | referenced by a resource's desired or last-applied generation, plus 3 superseded per environment | the rest |
| Claims | live and quarantined | released after quarantine |
| Checkouts | live; tombstoned while any resource, claim or unsettled run references them | tombstones 30 days after their last reference ends |
| Gate files | while their scope lives | a tombstoned scope's directory, removed under the scope gate `LOCK_EX or LOCK_NB` |

A checkout is tombstoned when a Mutate observes `Verified` absence of its
path and token. Retention runs only in Mutate, bounded per invocation, and in
an explicit maintenance command. Never in Static or Observe (K15).

## 8. Host and guest model

A guest exists only in container mode: an environment that declares an
operator workspace, such as Underlay `dev:container` or the DecodeLabs
bundles, where the operator works inside the container. There the guest is a
workspace client. It reads a materialised context locally and sends
everything with authority to the host over one channel. The host resolves,
gates, journals and runs the work. Hybrid mode (`dev:hybrid`), where only
services run in containers, has no guest.

### 8.1 Placements

| | Host | Guest |
| --- | --- | --- |
| Exists in | every Effigy | container mode only: a workspace container of an environment that declares an operator workspace |
| Detected by | no materialised context | a materialised context (§8.2) whose container identity matches the running container |
| State root | `~/.effigy/state` on the host | none. The guest has no store, no gates and no runs. |
| Gates | all resources | none; `Gate::acquire` returns `HostAuthorityRequired` |
| Store writes | yes | none |
| Runs | yes | none of its own. Each admitted request is a host run (§8.3). |
| Authority | the owning engine | requests over the host authority channel (§8.3) |

The mode boundary is the environment's declared operator workspace, never a
mode or environment name. Hybrid mode provisions no guest binary, no context,
no channel listener and no enrolment credential. Its service lifecycle uses
the same host engines.

Container heuristics alone never mean guest placement, and no user-exported
variable confers placement or authority. An unmanaged container, such as a CI
job, has no context: it is a Linux host with its own state root. The
materialised context replaces the run-token and handoff variables.

### 8.2 Materialised context

Workspace Prepare writes an immutable context into the container before the
workspace is admitted:

| Field | Content |
| --- | --- |
| Identity | host user, `CheckoutId`, environment `(ResourceId, Generation)`, full container id and VM boot id, the workspace session `RunId` (§8.7) |
| Path map | host checkout path ↔ guest mount path, workdir, configured workspace user and `HOME` |
| Sources | pinned bundle and catalog digests, manifest digests, and the rendered bundle and catalog snapshot they name |
| Backend | backend descriptor: kind, profile or context, runtime, VM type |
| Routes | declared and effective routes, service addresses and ports, gateway domains |
| Channel | the host endpoint locator and the pinned server identity (§8.3); neither is a credential |
| Binary | the expected Effigy version, digest and protocol version (§8.9) |

- Location: `/run/effigy/context/<generation>/` inside the container,
  outside the bind-mounted checkout, owned by root and read-only to the
  workspace user. `/run/effigy/context/current` names the published
  generation.
- Refresh: the host publishes a new complete generation on every Prepare and
  whenever a fact it carries changes on the host, then switches `current`.
  It never edits a published generation. A guest invocation reads one
  generation for its whole run.
- Staleness: the context is input, not liveness proof. Every request carries
  the context generation; the host refuses a stale one (K7) and the client
  rereads `current`. A missing context means the workspace is unprepared:
  commands that need it report unavailable. Nothing fetches, renders or
  allocates as a fallback (R2).
- Secret grants: an admitted run that needs secrets in the container gets a
  grant: only its declared keys, written 0600 for the workspace user under
  `/run/effigy/secrets/<run-id>/` as a journalled file effect, or passed in
  the remote spawn spec's environment (K12). The run's settle removes the
  grant. No shell gets every vault key.
- The guest never opens the host vault. The workspace mount masks the vault,
  its previous generation, key wraps and unlock credentials (§7.3 families
  10–11) with an empty read-only overlay. Vault reads and writes are channel
  requests to the secrets engine.

Ordinary source edits and task outputs in the mounted checkout are workspace
files, not host records. The context's authoritative inputs are host
records; the guest reads them and never regenerates them.

### 8.3 Host authority channel

One authenticated, per-session, typed protocol carries every guest
operation that needs authority or live host facts.

- Server: the workspace session run's supervisor (§8.7) listens for its
  session. The listener lives exactly as long as that run, so it adds no
  daemon (I-SV2, §14).
- Capability: an unguessable credential, issued by the host for one client
  session and held in a 0600 file in guest-private storage
  (`/run/effigy/client/`), never in the checkout, argv or a global
  environment variable (K12). The host keeps the grant and binds it to the
  host user, `CheckoutId`, session `RunId` and generation, environment
  generation, full container id and VM boot id, and the operations and
  targets it allows. A hostname, source address, marker file or claimed run
  id grants nothing.
- Capability issue: a Mutate request to the session run. The supervisor
  holds the grant in memory as state of that run; it is never persisted, and
  the run's settle ends every grant it issued. The first terminal opened
  through Effigy gets its capability as part of the Prepare or workspace entry that
  opens it.
- Transport: TLS with the server identity pinned in the context, and the
  capability presented inside the authenticated stream. A bearer token over
  plain HTTP is not enough. Unix sockets do not cross Colima or Docker
  Desktop file sharing, and the Docker API socket is never a channel.
- Requests: a selector or typed operation, checkout-relative cwd, intended
  target, expected context and environment generations, a request id and
  bounded arguments. The host never accepts a host executable path, an
  internal entry point, a guest-resolved plan or a caller-chosen state root.
- Host side: the host re-resolves the request against its own records into
  a `ResolvedInvocation` (§5.1), takes the canonical gates, journals effects
  and returns the same typed plans, events, statuses and outcomes a host
  client gets. The resource identity is the same for host and guest origins;
  the origin is metadata only.
- Placement: an admitted one-shot run gets a host owner process spawned by
  the session supervisor, `Inline` with its sentinel; that owner is the run's
  invoking process. A managed run is `Detached`. Container targets run in the
  container through remote custody (§5.10); host targets run on the host.
- Idempotence: the host records each request id before admission. A retried
  id returns the existing run or outcome and never admits a second run.
- Streams: events, log frames, terminal input and resize travel on the
  channel with bounded flow control. The guest client is a front end
  (I-SV3). For an `Inline` run it started, its stop request or stream EOF
  makes the session supervisor cancel the host owner, which settles (§5.2).
  For a `Detached` run it started, it is the owning client (§5.8): stop
  request or EOF without detach means stop and settle.
- Disconnect: on disconnect, capability loss, generation mismatch or a host
  probe timeout, the client reports `Unknown` for every unfinished run and
  refuses further Mutate. It never falls back to local execution. A
  reconnect with the same request id reads the host's recorded outcome.
- Reads: Observe requests run under `ObserveContext` on the host. They create
  no run, token or journal entry and append no access log. Authenticating
  with an existing capability writes nothing.
- Authorisation: the same operator authorisation and elevation policy as the
  host CLI. Container access is no new administrative or release authority.
- Version: client and server negotiate the protocol version before any
  request. A mismatch refuses before effects (§8.9).

Transport candidates by backend. None is supported until qualified:

| Backend | Candidate | Qualification |
| --- | --- | --- |
| Docker Desktop (macOS) | `host.docker.internal:<port>`; the host listener binds only an address the Desktop host proxy reaches | supported Desktop versions, default and custom bridge networks, firewall and VPN, IPv4 and IPv6; container `127.0.0.1` is never the Mac |
| Colima (docker or containerd; VZ or QEMU) | `host.lima.internal`, or the profile's `host.docker.internal` alias; the host prepares the profile's reachable endpoint, with an explicit forwarder adapter if needed | both runtimes and VM types; custom DNS disables the alias mappings; custom `gatewayAddress`; host-loopback reachability settings. `host-gateway` inside the VM is not the Mac. |
| Linux Docker Engine | `extra_hosts: ["effigy-host:host-gateway"]`, or a verified host address; the listener binds a restricted bridge-reachable interface | user-defined bridges, configured gateway addresses, rootless Docker, firewall policy, IPv6; never silently host networking |
| Apple Containers (later) | a verified vmnet endpoint or a reviewed native stream | no claim before qualification, including address changes on restart |

### 8.4 Backends

| Backend | Topology | The kernel relies on | Remote custody identity | Notes |
| --- | --- | --- | --- | --- |
| Colima (macOS; VZ or QEMU; docker or containerd) | one Lima VM per profile | `flock` never crosses virtiofs, sshfs or 9p, so gates are host-only; the state root is never mounted into containers; checkouts sit under the home mount | full container id, VM boot id from the guest's `/proc`, pid namespace, session, leader start ticks | `colima restart` changes the VM boot id, proving prior sessions `Absent`. Profile repair acts on retained identities, never `pkill -f` patterns. |
| Docker Desktop (macOS) | LinuxKit VM; host CLI | the same | the same | default file sharing covers `/Users` |
| Docker Engine (Linux) | shared kernel | the same rule, although `flock` would cross; container pids live in their own namespace and are never signalled from the host by number | the same; VM boot id is the host boot id | membership of the `docker` group is root-equivalent; the kernel never escalates |
| Apple Containers (planned) | one lightweight VM per container | the same rules; `container exec` | container id plus its VM boot id | the Compose relationship is open (Q-004); the adapter must pass the §5.10 conformance suite first |
| No backend | host-only projects | — | — | the kernel works fully without containers |

### 8.5 Platform facts

| Fact | macOS | Linux |
| --- | --- | --- |
| Boot identity | `kern.bootsessionuuid` | `/proc/sys/kernel/random/boot_id` |
| Process start identity | `proc_pidinfo` start seconds and microseconds, replacing whole-second `ps -o lstart=` | `/proc/<pid>/stat` field 22 |
| Gate | `flock(2)` on APFS or HFS+ | `flock(2)` on a local filesystem |
| Parent-death signal | none, so the sentinel | `PR_SET_PDEATHSIG` exists; not relied on, for one model |
| Signal by descriptor | none; numeric signals after an identity check, with a documented window | per process: `pidfd_send_signal`; per group: `PIDFD_SIGNAL_PROCESS_GROUP` on Linux 6.9 or later, detected per pidfd, with the macOS rules as fallback. A pidfd never reserves a number. |
| Group closure | `kill(-pgid, 0)` and process states | the same; cgroup v2 custody is a change trigger |
| Terminal | `tcsetpgrp` | `tcsetpgrp` |
| In-process clock | `Instant` (pauses in sleep) | `Instant` (pauses in suspend) |
| Boot clock | `mach_continuous_time` | `CLOCK_BOOTTIME` |

### 8.6 Coverage

Container mode gives full coverage: each command either runs locally
against the context, or reaches the correct host authority and gets the
result or refusal a host client would get. Rows follow the investigation's
coverage matrix (`synthesis/GUEST-EFFIGY.md` §2).

| Matrix rows | In the guest | Mechanism |
| --- | --- | --- |
| Help, groups, version; `--plan` and `test --plan`; `rhai surface`; `config` inspect, schema, reference, completion | local | Static; binary metadata and the context |
| `tasks` listing and resolve; implicit bundle load; `bundle` inspect; `service` list and extract | local | Static against the pinned snapshot (§8.2). No fetch, lock or cache write. |
| `graph` queries and status; `docs` checks and context; `scan`; `contracts` and `changelog` static checks; `skill tasks`; `state` plan; `artifact` inspect | local | Static and Observe reads of the mounted checkout and graph store. A stale index reports stale; a query never refreshes it (R2). |
| App tasks with `run_in` `host`, `container` or `either`; shell commands; task references, sequences and setup steps; Rhai tasks and host APIs; `test` and suites; drafts; QA groups and members; heavy tasks; `skill run`; `watch` | channel | Admission request. The host resolves placement, gates `Task(TaskId)` (K5 excludes host and guest runs alike), journals and runs: container targets in the target container through remote custody, host targets on the host. Rhai effects are classified on the host (§5.3). |
| `defer` | local inside an admitted workload; otherwise channel | Inside a workload it is that run's command, under its custody. From a shell it is an admission request targeting the current container. |
| Workspace-only tasks, `workspace`, `system` | channel | Attach to the live workspace session run (§8.7); never a second shell or session. Lifecycle operations are requests. |
| Managed TUI and headless (`dev`, profiles) | channel | Admission of a `Detached` managed run, child of the session run. The guest renders events; the host supervisor owns workloads and attachments. |
| Managed `status`, `logs`, `stop`; `tasks status` | channel | Observe requests for status and logs; `stop` is a generation-fenced request (K7). Host and guest pids are never mixed. |
| `tasks cache`, task-lock `unlock` | channel; `unlock` refused | Cache reads are host Observe requests; clears are RefreshDerived requests to the owning store engine. `unlock` refuses in both placements: there is no unlock command (I-G7). `migrate` and QA groups are in the Mutate and admission rows. |
| `doctor` | local and channel | Manifest and tool checks local; runtime checks (backend, gateway, routes, catalog pack, ownership, volumes) are host Observe requests. |
| `container shell`, bounded shell, `exec`, aliases | channel | Admission request; the host launches the command in the named service, including the current container, through remote custody. |
| `container up`, `down`, `reset`, `retire` | channel | Mutate requests to the environment engine under R4. A guest `down` of its own environment follows §8.7. |
| `container status`, `stats`, `logs` | channel | Host Observe requests on the backend inventory. |
| `container data`, `cache`, `volume`, `profile`, `eject`, `hosts`, `scope`, `ports` | local and channel | Declared configuration is local from the context; host facts are Observe requests; mutations, including `eject` source writes, are requests. |
| `secrets` list, doctor, get, export | channel | Observe requests to the secrets engine; plaintext follows the host CLI's policy. |
| `secrets` init, set, unset, import, unlock, lock, passphrase change; required task secrets | channel | Mutate requests to the secrets engine; task secrets arrive as per-run grants (§8.2). |
| `deps` status, link, unlink, pin, unpin | channel | Status is Observe; changes are requests to the deps engine. |
| `graph` index and watch; `bundle` sync; `service` pack install, rollback and remove; `config` user settings | channel | RefreshDerived, Prepare or Mutate requests to the owning store engine. The guest has no separate preference file. |
| `state` apply, capture; `artifact` stage, capture, push; `init`; `docs` index writes; `migrate`; `release`, `deploy`, `bootstrap`, `demo`, `uninstall` | channel | Requests under the same authorisation as the host CLI. |
| `gateway` subcommands | channel | Requests to the gateway engine; never a guest gateway. |
| Hidden internal entry points | refused | Never exposed on the channel; a guest invocation refuses. |

Exclusion covers workloads, data and endpoints, not only selectors: `dev`,
`dev:container`, `dev:hybrid` and aliases resolve to their resources on the
host, so overlapping app roles and shared data claims conflict there (§5.9).

### 8.7 Workspace session run

- The workspace session is a `Detached` run owned by `Task(TaskId)` of the
  environment's workspace entry. It holds a `Borrow` attachment to the
  environment and serves the channel (§8.3). Activating an environment that
  declares an operator workspace admits it; its starter detaches, so no
  terminal is its owning client.
- It outlives `dev`. A `dev` invoked from the workspace terminal is a child
  run (`runs.parent run`) with the guest client as owning client. Exiting it
  settles its own workloads and attachment, then returns to the terminal;
  invoking `dev` again admits another child run.
- Exiting `dev` asks whether to keep the environment up (Tom's ruling,
  2026-10-18).
  Keep: the session run stays. Stop: the client requests `container down`,
  which follows the down path below.
- Attach: the host `effigy workspace` and every guest terminal attach to the
  same session run as secondary clients (§5.8), and a host UI and a guest UI
  can attach to one managed child run. A secondary client's death stops
  nothing.
- Duplicates are impossible. An environment has one workspace entry, and
  K5 admits one unresolved run per owner, so a second `workspace` invocation
  attaches instead of starting a session. Every `dev` or task request takes
  its own owner gate on the host, whatever its origin.
- Down from inside (Tom's ruling, 2026-10-18) uses only existing
  primitives; no run settles another by cascade:
  1. The session supervisor observes the environment's holders. A live or
     `Unknown` holder other than the session run and its own child runs
     refuses with `HeldBy` before anything stops.
  2. It sends an ordinary stop request to each of its child runs and waits,
     under the request's deadline, for each to resolve and release its own
     attachment. An unresolved child refuses the down and is reported.
  3. It returns an operation id, then admits the down as its own `Detached`
     environment run with `ClientMode::Detach`, outside the session's
     custody, naming the session run as exempt (§5.9).
  4. The down run holds `Gate(Env)` and rechecks holders; any holder other
     than the session run refuses with `HeldBy`. It then requests the
     session run to settle. The session supervisor tells every attached
     terminal that the container is stopping, then settles itself and
     releases its attachment.
  5. The permit is issued once the only holder is the exempt run settling
     itself, or none (R4, I-A1), and the down applies. Its outcome stays in
     the host journal for host status.
- The session ends when the environment goes down or the host closes it.
  Closing the last terminal does not end it; retention after that is the
  environment engine's idle policy (§3).

### 8.8 Terminal enrolment

Direct `docker exec` and editor terminals in the workspace container are
enrolled automatically. The ruling record is the `v015-workspace-client`
lane's, not this contract's.

- Workspace Prepare installs a narrow enrolment credential, 0600 for the
  workspace user in guest-private storage (`/run/effigy/enrol/`), as a
  journalled file effect of the Prepare run. It is bound to the environment
  generation and the container identity.
- It authorises session enrolment only, never an effect. Enrolment is a
  Mutate request to the session run (§8.3): the host verifies the
  environment generation is current and the session run is live, then
  issues a per-session capability. A Static or Observe command never
  enrols; without a capability it runs locally only.
- A new environment generation revokes it. A missing, revoked or stale
  credential leaves local Static and Observe working and reports host access
  unavailable.
- Every process running as the workspace user is inside this trust
  boundary; the workspace is not a multi-tenant boundary.

This subsection is separable. Without it, a direct terminal gets local reads
only, and host access needs a terminal opened through Effigy.

### 8.9 Binary provisioning

- Each release publishes version-locked Linux x86_64 and aarch64 artifacts
  with a checksum and provenance manifest naming the version, protocol
  version, target and libc baseline, and catalog schema.
- Workspace Prepare acquires the artifact on the host, verifies its digest,
  and installs it into a guest-private executable store, selected per
  workspace generation by an atomic pointer. The architecture comes from the
  container image's platform, not the host's `uname`. An image whose libc
  has no matching artifact refuses Prepare.
- No run-time build or download: task, exec and terminal entry never fetch
  or compile Effigy or look for a sibling checkout. A contributor's explicit
  Prepare may build the artifact and record the same digest receipt.
- A version or protocol mismatch between client, context and server refuses
  before any effect, naming the host re-prepare command. A live executable is
  never replaced under an active run (I-SV4).

### 8.10 Rules under this design

| Rule | Holds because |
| --- | --- |
| R1 Host authority | The guest has no state root, gates or runs. Every effect is a host run under the owning engine's gate. Capabilities are host-kept grants; a request carries no authority beyond its capability, and the host re-resolves it. The state root and vault are never in the guest's mounts. |
| R2 Pure reads | Local Static and Observe read an immutable context and the mounted checkout and write nothing. Host Observe requests create no run, token or log entry. `--plan` is Static and admits nothing. Context publication and the enrolment credential are Prepare effects; capability issue and enrolment are Mutate requests to the session run. |
| R3 Unknown grants nothing | Disconnect, stale generation or an unreachable host is `Unknown` for the client and refuses Mutate with no local fallback. The host acts only on its own `Verified` observations and fences every request by generation (K7). |

## 9. Authority map

| Resource kind | Authority | Gate | Run placement | Journals | Settle undoes |
| --- | --- | --- | --- | --- | --- |
| Task | task engine | `Task(TaskId)` | inline | env schema, setup, steps, process groups | stop groups; `Cancelled` or `Interrupted` |
| Task with profile (managed) | run supervisor | `Task(TaskId with profile)` | detached | setup, attachments, gateway ensure, workloads (local, remote) | workloads (remote first), then release attachments |
| Env | environment reconciler | `Env` | detached for sidecars; inline for `up` and `down` | applying stages, claims, sidecar workloads | per environment policy, under a permit |
| Data | environment reconciler | through `Env` claims | — | claim reserve, commit, release | release claim |
| Endpoint | gateway engine | through claims; `Gateway` for lifecycle | inline; daemon | reserve, commit revision, applied acknowledgement | release to quarantine |
| Gateway | gateway engine | `Gateway(instance)` | daemon lifecycle run | bind, publish | stop |
| Store (vault, bundles, graph, deps) | owning lane | `Store(name)` | inline | replace commits | restore previous generation where declared |
| Delivery (release, state, deploy) | owning lane | `Task` or `Store` | inline | checkpoints | `Irreversible` reported; reconcile, never replay |

## 10. Adoption map

Current mechanisms and their kernel replacements. Evidence is at main
`bcb5bf40e`. Engine lanes own the migration.

### 10.1 Task engine

| Current mechanism | Evidence | Kernel replacement |
| --- | --- | --- |
| Lock key is the typed selector | `src/runner/manifest.rs:25-32` | `TaskId` from `ResolvedInvocation`; `Gate(Task)` (K1) |
| Lease lock: hard-link publish, stale reclaim, heartbeat thread | `src/runner/locking/io.rs:118-164`, `:251-339`, `:466`; `src/runner/locking/io/stale.rs:17-18`, `:173-204` | `Gate` (K2) |
| `tasks unlock` | `src/runner/locking/io.rs:202-249` | removed (I-G7) |
| Hostname as PID namespace | `src/runner/locking/io/stale.rs:194-218`; `src/runner/managed_settle.rs:526-540` | host-only gates (R1) |
| Status active, latest, history as three renames; 20 s staleness; FNV key over the typed selector | `src/runner/execute/task_status.rs:103-126`; `crates/effigy-runtime/src/task_status.rs:15`, `:241`; `crates/effigy-execution/src/task_status.rs:62-97`, `:245` | `runs` rows plus gate probe (§5.6 status) |
| Status write failure replaces the result | `src/runner/execute/pipeline/standard.rs:94-102` | `Outcome` warnings (K10) |
| Process group only inside a heavy scope | `src/runner/execute/process.rs:23-29` | `OwnedProcess` always (K6) |
| Owned-children TERM, grace, KILL | `src/runner/owned_children.rs:503-545`, `:685` | absorbed into `OwnedProcess` and `SignalRouter` |
| Generated sequence script with Python timeouts under `--json` | `src/runner/execute/sequence_run.rs:39`; `crates/effigy-managed/src/scheduler/script.rs:93` | one executor on `OwnedProcess` (K11) |
| Env-schema commands before the lock; unbounded pipe drain | `crates/effigy-env/src/exec.rs:25-77` | after `Admitted`; bounded capture (K3, K8) |
| `unsafe set_var` with threads alive | `src/runner/secret_session.rs:33`, `:52`, `:70`; `src/runner/execute/pipeline/managed.rs:229`; `src/runner/execute/sequence_run.rs:1034`, `:1044`; `crates/effigy-context/src/lib.rs:29`, `:45` | explicit context; spawn-spec env (K12) |
| Task cache rewritten in place | `src/runner/cache/store.rs:58` | `atomic_replace`, or the cut-list removal |
| Git exclude registration | Manifest composition reads only; source Prepare records registration before acting | Prepare and Mutate only (R2) |
| Bundle and catalog source preparation | Checkout pins and immutable source generations; no read-time refresh or cache lock | Prepare run; `Gate(Store bundles)` with deadline; atomic checkout lock publication |
| Scheduler client signal handler | `src/runner/host_scheduler/submit.rs:205-212` | subscribes to `SignalRouter` cancel; admission shape is Q-006 |
| QA group records: 20 s staleness, truncating writes | `crates/effigy-runtime/src/qa_group_status.rs:19`, `:275-291` | `Run` rows; `stop` and `hard_timeout_ms` become possible |

### 10.2 Run supervisor

| Current mechanism | Evidence | Kernel replacement |
| --- | --- | --- |
| `session.json` written by the headless owner | `crates/effigy-managed/src/runtime/headless.rs:156-167`, `:1060` | `runs` row from `Run::begin` |
| Setup steps before the task lock | `src/runner/execute/pipeline/managed.rs:256` before `:340` | setup after `Admitted` (K3) |
| `stop.requested` marker | `crates/effigy-managed/src/runtime/headless.rs:157`, `:687` | `requests` table (I-R4) |
| Exec group marker files in the bind mount | `crates/effigy-managed/src/runtime/headless.rs:764`; `crates/effigy-containers/src/session.rs:441-444` | in-band registration (§5.10) |
| Lossy `safe_component` paths | `crates/effigy-managed/src/runtime/headless.rs:1047` | encoding v1 (K1) |
| Settle budget as a sum of constants | `crates/effigy-managed/src/runtime/headless.rs:31-38` | settle observed from the journal; per-effect deadlines |
| Signal handler for INT and TERM only | `crates/effigy-managed/src/runtime/headless.rs:1218-1231` | `SignalRouter` with HUP (K9) |
| Owner discovery through lock files and run tokens | `src/runner/managed_settle.rs:42`, `:167-207`; `src/runner/locking/io.rs:349`, `:429-454` | attachments plus gate probe |
| Directory scan for live sessions before handover | `crates/effigy-managed/src/runtime/headless.rs:416-423`; `src/runner/container_command/handover.rs:440`, `:468` | `Attachments::holders` (I-A2) |
| Generated lifecycle shell, state file, EXIT trap, curl readiness, `starting` deadline bypass | `crates/effigy-containers/src/session.rs:221`, `:260`, `:317`, `:324`, `:405`, `:412-420` | attachment effect, `Probe`, deadlines |
| Progressive spawn without rollback | `crates/effigy-process/src/lib.rs:211-238` | `spawn_all` with barrier and rollback |
| Supervisor shutdown grace | `crates/effigy-process/src/supervisor_shutdown.rs:7-11` | `OwnedProcess::stop` |
| Host sidecar supervisors: setsid, PID and spec files, blocking lifecycle lock, best-effort stop | `src/runner/host_process.rs:104-128`, `:430-458`, `:745`; `src/runner/container_command/closeout.rs:33-36` | environment-owned detached run; `StopReport` |
| Transition evidence receipts | `src/runner/host_transition_evidence.rs` | `events` table, or the cut-list removal |
| Remote stop of a bare PGID in the current primary | `crates/effigy-containers/src/exec/managed_stop.rs:36` | `signal_session` on the registration (I-RC2) |

### 10.3 Environment reconciler

| Current mechanism | Evidence | Kernel replacement |
| --- | --- | --- |
| Compose up before the admission record | `src/runner/container_command/lifecycle.rs:204` before `:210`; `src/runner/host_container_lease.rs:250-280` | intent first (K3); `resources` row |
| `.complete`, `.partial.json`, `.handover.json`, `.persistent` and lease markers | `src/runner/host_container_lease.rs:19-23`, `:79-110`, `:489-626` | desired and applied generation; attachments |
| Lease reaper sleeper | `src/runner/host_container_lease.rs:631-720` | holder-based release under a permit (R4); idle policy is environment-lane |
| Admission hashing | `crates/effigy-containers/src/policy/admission.rs:26-106` | immutable objects; generation digest |
| Force-recreate, and the #328 `--no-recreate` guard | `crates/effigy-containers/src/compose.rs:134-137`; `src/runner/container_command/lifecycle.rs:499` | adapter call under a permit; recreation policy is Q-004 |
| Restart after a failed exec probe | `src/runner/container_runtime_prep/mod.rs:574`, `:587`, `:815` | probes never repair (R3, I-O2) |
| `reset` deletes every environment's renders | `crates/effigy-runtime/src/write/planning.rs:31-44` | objects per environment generation |
| Backend metadata read-modify-write | `crates/effigy-containers/src/lib.rs:219-235` | `resources` field; backend in `ResourceId` (I-ID4) |
| Allocators holding a global lock across runtime inventory | `crates/effigy-containers/src/policy_support/generated_compose.rs:1134`, `:1362`, `:1417` | claim transaction; no inventory under a gate (K14) |
| Runtime-scope records | `crates/effigy-containers/src/runtime/scope.rs:221-264` | `checkouts` table, token preserved |
| Colima repair by `pkill -f` patterns | `crates/effigy-containers/src/exec/colima_runtime.rs:924-944` | retained identities only (I-P4, R3) |
| Failed-run recovery downs inside `up` | `src/runner/container_command/lifecycle.rs:766` | predecessor settle (K5); permit (R4) |

### 10.4 Gateway

| Current mechanism | Evidence | Kernel replacement |
| --- | --- | --- |
| PID, identity and version files published before standard binds | `crates/effigy-gateway/src/identity.rs:299`, `:436`; `crates/effigy-gateway/src/server.rs:1264-1265` before `:1388` | gateway lifecycle run: bind is `Applied` before publish (K3); liveness design is gateway-lane |
| Record and transition locks | `crates/effigy-gateway/src/identity.rs:870`; `crates/effigy-gateway/src/legacy.rs:730-732` | `Gate(Gateway)` |
| Routes, ports and loopback files under `RouteTableLock` | `crates/effigy-gateway/src/routes.rs:188`, `:262`; `crates/effigy-gateway/src/ports.rs:137-155`; `crates/effigy-gateway/src/ports/reclaim.rs:109-113`; `crates/effigy-gateway/src/loopback.rs:81` | endpoint claims with revisions (I-A3, I-A4) |
| Listener claim check turns unknown into false | `crates/effigy-gateway/src/registration.rs:81` | `Observed` (R3) |
| Reuse of an unverified gateway | `src/runner/gateway_command/mod.rs:168` | R3 |

### 10.5 Other runs and stores

| Current mechanism | Evidence | Kernel replacement |
| --- | --- | --- |
| Release prepared state written after mutations; branch and tag pushed separately | `crates/effigy-release/src/lib.rs:1456`; `crates/effigy-release/src/git.rs:241` | `Run` with checkpoints; pushes `Irreversible`, reconciled |
| Deploy reports success for unexecuted work | `src/runner/deploy_command/transaction.rs:109`, `:287` | R5 and I-OC1; ownership is Q-003 |
| Demo spawn and group signal | `src/runner/demo_command/execute.rs:48` | `OwnedProcess` |
| Vault written in place, unlocked | `src/runner/secret_vault.rs:177-194`; `crates/effigy-rhai/src/rhai_secrets.rs:665` | `Gate(Store vault)` + pointer commit (§5.5) |
| Deps state: try-only lock, local atomic helper | `crates/effigy-deps/src/state.rs:497`, `:573` | gate with deadline; `atomic_replace` |
| Graph refresh: try-lock, snapshot copy, detached worker | `crates/effigy-codegraph/src/refresh.rs:107`; `crates/effigy-codegraph/src/storage.rs:116`; `src/runner/graph_time_budget.rs:77` | RefreshDerived; cooperative deadline (I-CX3); corpus semantics are graph-lane |
| Catalog pack store lock | `crates/effigy-catalog/src/pack/store.rs:343` | `Gate`, or removal under Q-005 |
| Pending-facts journal with file and directory `fsync` | `crates/effigy-host-run/src/secure_fs.rs:299-308` | the pattern `atomic_replace` absorbs |
| Doctor publication marker | `crates/effigy-scan/src/execution/doctor_inventory.rs:586` | `Gate` |
| Rhai, runtime and doctor signal flags | `crates/effigy-rhai/src/lib.rs:219-221`; `crates/effigy-runtime/src/signals.rs:184-186`; `crates/effigy-scan/src/execution/doctor_inventory.rs:2064` | `SignalRouter` and `CancelToken` (K9) |
| `init` writes in place | `crates/effigy-builtin/src/init/agent.rs:814` | `atomic_replace` |
| Boot and start identity helpers | `crates/effigy-process/src/identity.rs:13-65`, `:214-245` | absorbed; macOS start identity via `proc_pidinfo` |

## 11. Migration

A v0.14.1 user updates and everything keeps working with no manual steps
(vision). Importers accept both v0.14.1 and main `bcb5bf40e` shapes.

| # | Principle |
| --- | --- |
| M1 | Never dual-write. Per scope: quiesce the old writer, import, cut over. Old and new code never write the same fact. |
| M2 | One store transaction per scope, under `Gate(Store migrate:<scope>)`, recording the legacy path and digest of each import. |
| M3 | Import never modifies or deletes legacy files, and the new binary never writes them. Removal comes after one minor, by explicit maintenance, once the scope's import is committed. |
| M4 | Unknown grants nothing. Unparsable, PID-only or foreign legacy state imports as `Unknown` and blocks destructive action until an explicit, label-verified `down` or `retire`. |
| M5 | A legacy version without an importer is ignored, never guessed at. Doctor lists it. |
| M6 | Observe never imports. Until a scope is imported, status reads legacy files read-only for one minor. The first Mutate per scope imports. |
| M7 | Running stacks survive. Containers keep running, labels prove ownership, nothing is recreated because of an import. |
| M8 | Downgrade after a scope's cut-over is unsupported. The store records the writing build, and a binary that predates the schema refuses Mutate. |

| Legacy family | Location | Import | Quiesce condition | `Unknown` handling |
| --- | --- | --- | --- | --- |
| Task lease locks | `<root>/.effigy/locks/` | not imported | a live legacy holder (same host, PID alive, Effigy command) refuses Mutate on that task with the next command | foreign hostname or unreadable record: refuse, never delete |
| Task status v1 | `<root>/.effigy/runtime/tasks/`, `<root>/.effigy/reports/tasks/` | not imported; read-only legacy view for one minor | — | — |
| Managed sessions, stop markers, exec group markers, lifecycle state | `<root>/.effigy/runtime/managed*/` | a run with phase from its status, owner identity PID-only | a live legacy owner refuses `begin` for that owner and names the legacy stop control, kept for one minor | PID-only owners and bare PGIDs are `Unknown`; never signalled by number |
| Environment markers, lease, persistent owner, handover, admission | `~/.effigy/runtime/host-container-leases/`, `<root>/.effigy/runtime/` | `resources` row: desired from persistent or lease, applied from complete, failed-unknown from partial without complete | a live reaper sleeper is waited for or refused | containers adopted only by `Verified` labels |
| Generated renders | `<root>/.effigy/runtime/compose/` | not imported; re-rendered into objects on the next apply | — | — |
| Backend override metadata | `crates/effigy-containers/src/lib.rs:219-235` | the environment's backend field | — | unparsable: `Unknown` backend; destructive actions refuse until the backend is named |
| Host sidecars: PID, spec, listener records | `<root>/.effigy/runtime/host-processes/` | stopped through the legacy stop protocol after identity verification, then started under an environment run | legacy supervisor alive and verified: stopped first | unverifiable identity: `Unknown`, residue |
| Runtime-scope records | `~/.effigy/runtime-scopes/` | `checkouts`, tokens preserved | — | — |
| Gateway PID, identity, version; routes, loopback, ports | `~/.effigy/gateway/`, `~/.effigy/ports.json` | gateway-lane importer: claims byte for byte, manual routes kept | legacy daemon quiesced by the gateway lane's setup | — |
| Vault | `<checkout>/.effigy/secrets/local.vault` | not moved; the next write uses the pointer commit | — | unreadable vault: never overwritten |
| Host-run pending facts; retired admission store | `~/.local/state/host-run/`; `~/.cache/effigy/admission/` | untouched (contract 049) | — | — |

## 12. Proof plan

### 12.1 Fault-injection harness

- A `fault-injection` feature, compiled only into test builds, names every
  injection point: store commit (before, after); each `atomic_replace` step;
  gate acquire; each `begin` step; `intend`, `applied`, `failed` (before,
  after); each adapter effect (before, after); spawn (fork, before record,
  before go, after go); remote launch (before ack, after ack before record);
  each settle step; phase and request writes; sentinel actions; log writes;
  every observation.
- Fault kinds: `Kill` (SIGKILL self), `Enospc`, `Eperm`, `Unknown` (the
  observation returns `Unknown(Injected)`), `Stall` (SIGSTOP self until the
  harness continues it, or a sleep past the deadline) and `ClockJump`
  (injected wall clock).
- Driver: run a scenario with tracing to list the points it crosses; then,
  for each point and fault, run it in a fresh `mktemp -d` HOME and checkout,
  run the recovery step (status, stop, rerun or settle), and check the
  oracles.
- Scenarios: plain task; sequence; multi-child test; detached managed run
  with an environment attachment; environment up, down and handover on the
  strict fake backend, with a concurrent attach; a `Closed` predecessor
  with residue followed by `begin`; owner loss followed by recovery stop over
  a reused or foreign process group; Observe with guarded subprocess and HTTP
  probes; endpoint reserve and commit; vault write; legacy import.

| Oracle | Must hold after recovery |
| --- | --- |
| O1a | A run whose cleanup is `Complete` has no owned process alive: nothing in its recorded groups or sessions, and nothing carrying its custody nonce. |
| O1b | A run whose cleanup is `Incomplete` lists every surviving owned or unidentifiable process as residue, keeps its protections (K5 blocks a successor unless acknowledged; R4 keeps its environment holds), and claims neither `Settled` nor `Succeeded`. A process survives only as listed residue. |
| O2 | Every run is `Closed`, or reported `interrupted` with a parser-valid next command. |
| O3 | At most one non-closed run per owner. |
| O4 | Every gate is free unless a live process holds it. |
| O5 | Strict-fake call logs show no non-idempotent effect applied twice, and exactly one release per attachment. |
| O6 | `kernel.db` passes integrity and foreign-key checks. |
| O7 | Every `atomic_replace` target parses as its old or new generation. |
| O8 | No destructive call targeted anything whose observation was `Unknown`. |
| O9 | Observability faults never changed a workload outcome or exit code. |
| O10 | Static and Observe commands produced an empty filesystem diff. |
| O11 | No destructive environment call ran while an attachment existed. |
| O12 | No signal reached a process without generation evidence (I-P4); foreign fixture processes all survive. |
| O13 | No run was admitted beside an unresolved predecessor (K5). |
| O14 | While its observer or its guard lives, no probe process inside that owner's evidence bound (§5.11) outlives the deadline plus grace (I-O6). Processes without generation evidence are never signalled. |

### 12.2 Strict fakes

Every adapter (backend, remote exec, platform, probe transports) has a fake
that fails on any unmodelled invocation, records its calls, and supports
injected `Unknown`, timeouts and partial effects. Foreign resources carry
sentinels that must stay untouched. The unselected backend's fake must see
zero calls. A test may not be loosened to match observed behavior without a
recorded contract change.

### 12.3 Matrices

- Supervisor matrix, full cross product as cheap model tests: front end
  (TUI, attached text, detached) × prior state (fresh, interrupted
  predecessor, closed predecessor with residue, stopped) × stop (request, INT, TERM, HUP, owning-client
  SIGKILL, owner SIGKILL, handover, explicit `down`) × attachment (none,
  borrowed, exclusive). Every cell asserts exactly one release per
  attachment. Named end-to-end representatives follow.
- Platform qualification on macOS and Linux for gates, identity and custody.
- Real-engine remote-custody qualification on Colima, Docker Desktop and
  Linux Docker (§5.10).

### 12.4 QA ratchet

| Item | Specification |
| --- | --- |
| Selector | `qa:kernel-ratchet`, a member of `qa:architecture` |
| Scanned | `src/` and every `crates/*/src/`, excluding `crates/effigy-kernel/` |
| Banned outside the kernel | `std::process::Command` and `tokio::process::Command` construction or `spawn`, `output`, `status`; `libc::kill`, `libc::killpg`, `nix::sys::signal::kill`, `killpg`; `libc::flock`, `fs2::FileExt` locks, `std::fs::File` lock methods; `signal_hook` registration; `std::env::set_var` and `remove_var`; `fs::write`, `File::create`, writing `OpenOptions`, `fs::rename` and `fs::remove_file` in modules that build Effigy state paths (`.effigy` or the Effigy home); `pkill` and `killall` literals |
| Counting | per file and construct, by a syntax-aware scanner, not a text grep; `#[cfg(test)]` items and `tests/` trees are reported separately and not gated |
| Baseline | `config/kernel-ratchet.json`, created when the gate activates |
| Gate | fails when a count rises or a new file and construct pair appears; also fails when a count falls without the baseline being lowered in the same change |
| Done | every product count is zero (lane "done when") |

### 12.5 Per-primitive obligations

Each primitive's proof list in §5 is mandatory before that primitive merges.
The fault suite (§12.1), with every oracle green, is mandatory before any
engine migrates onto the kernel.

## 13. Fix and defect mapping

Every fix in #192–#202 and every kernel-owned class in SYNTHESIS §6, mapped
to the rule and primitive that makes it structurally impossible.

| Fix | Symptom the proof found | Rule or invariant | Primitive | Why it cannot recur |
| --- | --- | --- | --- | --- |
| #192 (PR #318) | container teardown deleted a listener-owned route | K1, K7 | endpoint `Claim` with holder and generation | teardown releases only claims its run holds, fenced by generation |
| #193 (PR #320) | shared data needed an exclusive writer | R4, I-A3 | `Data` claim reserved in one transaction | two environments cannot both commit a claim on one data resource |
| #194 (PR #319) | volume and project names changed with the Rust toolchain | K1, I-ID5 | encoding v1 | identity is fixed canonical bytes, never `std::hash` |
| #195 (PR #321) | a partial peer stop left an incoherent environment | K3, K4, R4 | handover as a `Run`; permit | every step is journalled; a failure resumes through settle |
| #196 (PR #322) | peer resolution rewrote the admitted payload | R2, K3 | `EffectClass`; immutable objects | resolution is Observe; published renders are never rewritten |
| #197 (PR #323) | 1 Hz probes tore renders; reuse on unknown identity | R2, R3, I-O2 | `Probe`; `atomic_replace`; `Observed` | probes cannot write; `Unknown` cannot authorize reuse |
| #198 (PR #324) | handover did not know a headless owner existed | K3, K5, I-A1, I-A2 | `Run::begin`; attachments serialized with permits on `Gate(Env)` | the owner and attachment rows exist from `Admitted`; handover reads holders, not directories; no attachment can appear between permit and mutation |
| #199 (PR #325) | in-container readiness; `starting` bypassed its deadline | K8, I-O3 | `Probe`; `OperationContext` | every wait takes a deadline; readiness is a typed in-process predicate |
| #200 (PR #326) | settle wait too short; remote groups survived | K6, R5, I-RC3, I-P4 | remote custody; journal settle | remote closure is observed per session, signalled only with registration or nonce evidence; settled is a journal fact, not a budget |
| #201 (PR #327) | a failed child left a partial stack | K4, K3 | one settle on every exit path | startup failure enters `Stopping` and settles; no success-only cleanup |
| #202 (PR #328) | session record absent; mass exit 137 from an explicit `container stack down` under a live run | K3, R4, R3, K5, I-A1 | `Admitted` first; `TransitionPermit` serialized with attach on `Gate(Env)`; `Observed` | the record exists before any effect; no permit exists while a holder is live, unknown or still holding residue, and none can be outflanked by a late attach |

| SYNTHESIS §6 # | Defect | Kernel-owned | Rule or invariant | Primitive |
| --- | --- | --- | --- | --- |
| 1 | Elevated log opens follow symlinks | yes, file safety; root removal is gateway-lane | K13 | descriptor-relative no-follow opens |
| 2 | Resolver files Effigy doesn't own overwritten or removed | yes, the rule; the managed-header check is the setup adapter's | R3 | `Verified` ownership before external mutation |
| 3 | Non-atomic, unlocked vault write | yes | I-S4, I-G4 | `Gate(Store vault)` + pointer commit |
| 4 | Installed incompatible pack blocks workspace exec | no (Q-005, catalog lane) | — | — |
| 5 | Stale-lock reclaim race | yes | K2 | `Gate` |
| 6 | Reaper stops a stack mid-task | yes | R4 | attachments, `TransitionPermit` |
| 7 | Four unserialised `down` authorities under a live run | yes | K4, R4, I-A1 | one settle; `Gate(Env)` serializing attach and permit |
| 8 | Deploy reports success for work it never ran | yes, the rule; deploy ownership is Q-003 | R5, I-OC1 | `Outcome` |
| 9 | `qa`, `ci`, `ci:fresh` hard-wired to the scheduler | no (Q-006) | — | — |
| 10 | Underlay bundle floor below the keys it emits | no (bundle and config lane) | — | — |
| 11 | Wrong-backend volume actions; unserialised data writers; global reset | yes for identity and claims; reset scope is environment-lane | I-ID4, I-A3 | backend in `ResourceId`; `Data` claims; objects per generation |
| 12 | Host stop drops failures | yes | R5, I-R8 | `StopReport`; `Outcome` residue |
| 13 | Spawn before record and handler; SIGHUP unhandled; partial spawn leaks | yes | K3, K6, K9, I-P4 | `Run::begin`; spawn barrier; `spawn_all`; `SignalRouter`; generation-evidenced signals |
| 14 | Unbounded waits | yes | K8 | `OperationContext` |
| 15 | Gateway status prompts and lies; `up` reports before binding | yes for the rules; gateway shape is gateway-lane | R2, K3, I-O5 | Observe class; bind `Applied` before publish; `EndpointApplied` |
| 16 | Reads mutate | yes | R2 | `EffectClass` |
| 17 | Cancelled tests can pass | yes | I-OC1, I-OC2 | `Outcome` |
| 18 | `--json` changes sequence execution | yes | K11 | one executor; one output owner |
| 19 | Rhai swallows INT and TERM | yes | K9 | `SignalRouter`; `CancelToken` |
| 20 | Graph index torn on timeout, then reported fresh | yes for the transaction and deadline; corpus semantics are graph-lane | I-CX3, I-S7 | RefreshDerived transactions; cooperative deadline |
| 21 | Passphrase in child env; secrets in argv | yes | K12 | `ProcessSpec` env; redaction |
| 22 | Release commit, tag and push partial failure | yes | R5, I-R7 | `Run` checkpoints; `Irreversible` |

## 14. Non-goals

- No host-wide scheduler (vision). Heavy-task shape stays open under Q-006.
- No separate binaries. Supervisor, sentinel and probe guard re-exec the one
  `effigy`.
- No daemon beyond the user gateway, per-run processes (supervisor,
  sentinel) whose lifetime is one run and its settle, and probe guards whose
  lifetime is their observer's open probes.
- No exactly-once claims across filesystem, engine or network boundaries.
- No Compose replacement. The environment boundary stays open under Q-004.
- No remote execution, plugins, marketplace or Windows support.
- No cgroup resource limits, and no OS-level enforcement beyond process
  groups and sessions.
- No public command. Engines own every surface.

## 15. Change triggers

Revisit this contract before: adding a daemon; coordinating across hosts;
adding a second guest-to-host channel or a guest outside container mode;
supporting network filesystems; adopting cgroup custody; adding a resource
kind or encoding version; bumping the store's major schema; supporting Apple
Containers; or relaxing any of R1–R5.
