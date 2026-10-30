# 052 - Runtime Kernel Contract

Q-010 storage model (Tom, 2026-10-22: "Approve"): foreground runs keep
runtime state in memory; runs requiring retained state use their owning
checkout journal; only gateway endpoints/routes, ports and admission facts use
the machine store. §3.1, §7 and §11 govern classification, storage and migration.
Engine adoption limits are recorded per engine in that engine's own contract.

Local shell tasks, statically resolved local sequences and local test suites,
including the interactive test TUI, use `ForegroundRun`. Ancestor gate custody,
verified parent grants and positive process closure preserve nested admission
and cleanup without durable records. `--run-id`, managed runs, QA groups,
environments and remote clients select checkout durability before their first
effect. Status combines live foreground evidence with
explicitly historical durable results; a missing record means `not-recorded`.

A checkout journal opens by its location, `<checkout>/.effigy/state/`, and
records no checkout, Git or home generation (Q-011, §7.1). A replaced `.git`
directory or pointer, a `git worktree repair`, a moved checkout and a reboot are
ordinary: the journal keeps its history, and a checkout's gate names follow a
digest of its canonical path. Linked worktrees have separate journals and
imported key aliases stay local. Foreground runs create no token and consult no
machine state. `runs --all` and doctor use bounded filesystem/Git and
explicit-root discovery with visible omissions, without a central index.

v0.15 imports nothing (§11). A v0.14.1 install upgrades with no manual step
and no command: everything that release left on disk is ignored, never changed
and never deleted, and a stack it left running is reconciled by the first `up`
through the ordinary apply. Named volumes and Compose project names did not
change, so the data stays and the upgrade costs one container recreate per
stack. No legacy layer remains: no importer, no migration report and no
PID-only refusal.

Runtime ownership is only the backend's own labels; there is no scope inventory
to keep. After deletion, `container retire --dead` groups the labelled
containers, volumes and networks by checkout path and retires the groups whose
path is missing. The private backend proof deletes a linked worktree, retires
its labelled stack, and verifies that live, unknown and foreign owners, the
unselected engine and persistent data survive.

Machine fact edits publish a complete conflict set in one transaction; unchanged
edits/read/import write zero. Cold schema initialization is a separate commit.
Gateway startup commits intents and receipts before go, then detach; positive
close compacts history and retains one bounded current diagnostic. Machine
process output stays in memory. Release checkpoints, certified candidates and
native process custody belong to the checkout; the vault uses a machine gate
without opening a runtime database. §7.5 owns the exact machine write budget.

Status: foundation types are implemented in `crates/effigy-kernel/`: identity
encoding v1, operation contexts and class capabilities, observation and Probe
API types, outcomes, and macOS/Linux platform facts. Gates (§5.4), the store,
records and `atomic_replace` (§5.5), the state root, its kernel-owned path
families and retention (§7) are implemented with their proof lists and
fault points. Immutable object closures, run-owned staging settlement and fenced
object reference transactions (§7.1–§7.4) are implemented in `objects`, with
`test:kernel:objects` concurrency and fault proofs. `qa:kernel-ratchet` enforces the committed production baseline
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
Fault points inside helper processes (sentinel settle, guard acknowledgement)
are not yet armed; architecture 032 records the limit. The managed-run
supervisor is armed at every kernel crossing (`test:supervisor:faults`). That
proof traces one immutable crossing list, assigns each entry once across six
owned fixture workers, and reaps every worker before aggregation; per-run
deadlines remain separate from the whole-matrix timeout.
The task engine has adopted the kernel (effigy#217),
and bundle-source preparation uses Prepare runs, source custody, store gating
and atomic lock publication. The environment reconciler has adopted it
(effigy#232; contract [053](053-environment-reconciler-contract.md)).
Managed runs have one detached supervisor per run (effigy#235). QA-group runs are kernel runs,
but `stop` and `hard_timeout_ms` stay unavailable: contract
[051](051-bounded-qa-groups-contract.md) keeps refusing them until the task
engine exposes them.

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

Before the kernel, Effigy had no shared runtime foundation. Identity, exclusion, durable intent,
process custody, deadlines and "unknown" observations were re-implemented per
domain, each unsoundly, and cross-domain operations rebuilt another
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
| Host | The machine kernel that owns the checkout and machine state roots (§7). macOS or Linux. |
| Guest | Effigy inside a container-mode workspace container managed by a host Effigy: a workspace client with no state of its own (§8). |
| Host authority channel | The one authenticated, per-session protocol that carries a guest's requests to host engines (§8.3). |
| Workspace session | The durable host run a container-mode workspace's terminals attach to (§8.7). |
| Resource | Anything with one authority: task, run, environment, data, endpoint, store, gateway, checkout. |
| Authority | The engine that alone may mutate a resource, acting while it holds the resource's gate. |
| Gate | A host `flock` on a stable inode in the resource's owning scope (§7). Never unlinked while that scope lives. Holding it is liveness. |
| Run | One operation by one owner resource: an in-memory foreground run or a durable run (§3.1). |
| Effect | One external change a run makes: process, remote process, attachment, claim, file, adapter call, checkpoint. |
| Intent | An effect record committed before it starts: in memory for foreground runs, durable for durable runs. |
| Settle | The one idempotent path that resolves a run's recorded effects. |
| Holder | A run attached to an environment. |
| Claim | An exclusive hold on a data, endpoint or address resource. |
| Generation | A durable per-resource counter; foreground fences additionally bind the fresh RunId (§3.1). |
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
| D1 | Runtime store | per-checkout JSON records, revision field | one SQLite store: operations, resources, attachments, claims, requests, events | Q-010: memory for foreground runs; SQLite journals per checkout; a small machine store for shared facts only (§7). Vault and user-owned files stay separate. | Tom, 2026-10-22; Q-010 |
| D2 | Store and gate location | `<checkout>/.effigy/kernel/` | outside disappearing worktrees | Q-010: `<checkout>/.effigy/state/`, independently per linked worktree; machine resources alone under `~/.effigy/state/`. Guest mounts mask local state. | Tom, 2026-10-22; deletion limits in §7.1 |
| D3 | Settle | replay recorded undo in reverse | per-adapter semantics, positive settlement | Reverse order, with a declared `Settlement` per effect. Irreversible effects are reported, never replayed. | Both; X-sol's objection to blind undo of borrowed or irreversible effects is absorbed |
| D4 | Meaning of a free gate | free gate + live record = interrupted | release permits reconciliation, not successor activation | Both. A free gate with a non-closed run reads `Interrupted`. A successor must settle it before its own run is admitted (K5). | X-sol §3: lock release does not prove grouped or remote work ended |
| D5 | Effect classes | static, read, refresh, mutate | Static, Observe, RefreshDerived, PrepareSource, Mutate | Static, Observe, RefreshDerived, Prepare, Mutate | Brief |
| D6 | Managed supervisor | detached | foreground stays foreground | One detached supervisor per managed run. Owning-client death means stop and settle. Attaching to a running stack leaves it running. | Tom ruling 5 |
| D7 | Guest Effigy | isolate, opt-in, runs under the host run's token | cut auto-provisioning, host owner protocol | Workspace client in container mode only: local reads against a materialised context; every authority request over one authenticated host channel, re-resolved, gated and journalled on the host; a durable workspace session run; a version-locked Linux binary installed at Prepare. Hybrid mode has no guest (§8). | Lane rule 1; Tom 2026-10-18 (container mode needs full coverage); `synthesis/GUEST-EFFIGY.md` option (c) |
| D8 | Endpoint store | `state.json` + lock | SQLite + effect outbox | Endpoint claims are kernel store rows; the gateway engine is their only writer; a released endpoint is immediately reusable | D1; X-sol gap 6 |
| D9 | Remote registration | ack `{container_id, nonce, pgid}` | ack with namespace and start identity | In-band ack on the exec stream, plus the nonce in the remote environment for recovery; no bind-mount marker files | R1: guests never write host state |
| D10 | Owner SIGKILL of a one-shot run | "no custody process outlives its run" incl. KILL | a small supervisor may outlive the front end | Foreground custody has an in-memory death guard; durable custody keeps the journal-backed sentinel (§3.1). Loss of foreground evidence leaves residue, never numeric recovery authority. | Q-010; X-opus K5 for durable runs |

Not decided here: Q-003 deployment ownership, Q-004 the Compose boundary,
Q-005 service catalogs, Q-006 heavy tasks, environment idle-stop policy,
gateway activation mode, the ports-registry fate and graph corpus
semantics. Kernel rules constrain those engines; they do not choose for them.

### 3.1 Q-010: run classes and the durability boundary

The classifier runs after resolution, before admission, setup or any spawn.
Duration is not a classifier: a ten-hour foreground build can be in memory,
and a one-second environment transition is durable. `--json`, pipes, CI and
the absence of a TTY do not select durability. A foreground invocation's
lifetime belongs to its invoking process/session even when it has no terminal.

| Invocation or obligation | Class and owner |
| --- | --- |
| Local foreground task, sequence or test; all descendants local and terminal-bound | `Foreground`: record, intents, receipts and outcome in memory; checkout resource lock files |
| `tasks qa-group run` (maintained or temporary group, with or without heavy members) | `CheckoutDurable`; contract [051](051-bounded-qa-groups-contract.md) requires a retained definition, member ledger, result and run-scoped logs after completion; member runs use the group's checkout journal |
| `--run-id <key>` on a supported one-shot command | `CheckoutDurable`, even without `--detach`; idempotency, requests and logs need a journal |
| `--detach`; managed session in TUI, attached text or `--headless`; environment-owned host sidecar | `CheckoutDurable`; detached supervisor and journal-backed sentinel. Existing managed-task refusal of `--run-id` is unchanged |
| `container up` (attached or detached), down, handover, repair, reset, reap, retire, data mutation or environment import | `CheckoutDurable`; environment authority in its owning checkout |
| A task that attaches to/ensures an environment, runs a container/remote workload, owns a workspace session, or publishes a route | `CheckoutDurable`; durable holder and remote-custody records before effects |
| A workflow needing resumable checkpoints or persistent source preparation after the invocation | `CheckoutDurable`; the feature's authority owns the journal |
| Gateway daemon lifecycle, endpoint/route publication, port allocation or release | `MachineDurable`; gateway authority, only the bounded records allowed in §7.5 |
| Static/Observe, including `runs`, `run status`, doctor and gateway status | No run, admission, import or write (R2) |

A run tree has one durability requirement: a durable descendant makes its
foreground ancestors checkout-durable before the first effect. Local children
of a durable run use that checkout journal. Late-bound scripts and adapters
must declare durability before execution with `durable = true` on their task
table. This includes test harnesses which exercise durable kernel operations or
launch named/detached Effigy children from shell code. The default `false`
permits foreground admission only when the remaining tree is local. A
referenced durable task makes its ancestors durable too. A declared durable
task selects the journal for the whole tree before workload go; there is no
promotion after workload go and no silent fallback to the machine database. A
container-free foreground build never opens either database for writing.
Machine-store failure is not a foreground dependency; existing local
predecessor evidence stays read-only.

The durability class belongs to the invocation that declares it. A nested
`effigy` is an ordinary invocation: it classifies its own tree, and a durable
nested run journals in its own checkout. No process inherits or checks a
"parent capability", so closing a descriptor, killing or sleeping the parent, a
dead admission hook or a leaked environment variable never makes a nested run
refuse. Tom, 2026-10-25 (Q-011): "Parent-capability inheritance
(`EFFIGY_PARENT_CAPABILITY` plus fd 9) is removed. This reverses that part of
the Q-010 kernel design."

Foreground admission reads no journal: the owner flock is the only exclusion
that matters, so durable residue a dead run left behind is not a reason to refuse
(K1, Q-011, F11). Settling a durable predecessor is a separate durable
operation, and only a proved, live predecessor holding that owner gate refuses.
Local gate identity is a digest of the root's canonical path, with a domain
separate from persisted `CheckoutId`s, so a reboot, a renumbering or a replaced
Git directory cannot rename a checkout's gates and locks (effigy#337). First
durable initialization serializes on this local identity gate. A short run never mints a Git token, registers a
checkout, appends an ignore block, creates a database, writes a run directory,
or fsyncs run metadata. Lock-file creation and best-effort diagnostic bytes
are transient exclusion machinery, not durable run records. Task outputs and
explicit user-requested exports are outside runtime-state accounting.

Admission nesting is one advisory environment fact. `EFFIGY_ADMITTED=<admitting
run id>` is set on everything an admitted heavy run spawns; a heavy invocation
that sees it does not call the admission hook again (contract
[049](049-heavy-validation-admission-contract.md)). There is no liveness check
and no other authority in it: no hook identity is retained, written down or
re-verified, and a forged marker only lets a run skip the local orchestrator's
queue, which is the orchestrator's risk on a single-user machine. A task that
is not `admission = "heavy"` never consults the hook at all (F10, Q-012).

Custody of a run's own descendants is unaffected by that: process groups, the
memory guard and the sentinel settle the tree the run started. Foreground
descendant custody uses an anonymous cleanup pipe whose writing end the run
keeps. Custody exports only a write-only copy in reserved descriptor 10. A
descendant guard holds a pin through its own settlement, and ancestor guards
keep their gates until all pins close and EOF is observed. Pin bytes carry no
protocol or heartbeat and confer no gate, journal-write or signalling
authority; a pipe cannot be shut down to forge EOF while another guard still
holds a pin. Unknown closure keeps the ancestor gated. The guard checks
descriptor type and access before adopting it, and group anchors receive no
pin. A durable run with no memory guard keeps its checkout custody backup in
the same way.

Bound checkout admission takes the checkout's one gate spelling: the digest of
its canonical path. A run's persisted-token scope and that spelling are the same
lock, so a token-free foreground invocation and a token-carrying durable run of
the same checkout exclude each other, and first durable initialization cannot
split exclusion from an already running foreground invocation. A helper reopens
the journal by path and marks it as that checkout, so its recovery meets its
owner's locks. Durable recovery checks predecessors under the held owner gate
before admitting a successor. The bounded admission retention sweep runs after
those gates; a refused gate does not create a database for maintenance.

**Foreground custody (I-P2/I-P3).** Before workload go, the owner and its
in-memory guard acknowledge the complete custody receipt, nonce and ready
group anchor. The existing spawn barrier still applies. The guard uses the
existing sentinel entry point in a memory-only mode, with a private pipe; no
control socket, log file or journal. Only that guard may inherit the owner's
gate description; workloads never do. Each later spawn repeats the receipt/ack
barrier before go. A free flock, not dead-looking diagnostic bytes, proves
there is no holder. A held lock with a dead/unverifiable named owner remains
held; the guard may refresh its own identity in the diagnostic body without
creating durable authority. No stale-file unlink or PID-only reclaim exists. Owner and guard retain the group and
unreaped-child/verified-anchor evidence through descendant and stream closure,
even after leader exit. Normal settle closes the in-memory effects before
unlocking. Owner EOF or terminal HUP/INT/TERM makes the guard stop the retained
group within the cleanup budget, using the same positive evidence as the
owner, then exit. While cleanup runs it keeps the owner gate held; it never
unlocks on owner EOF alone. The owner similarly keeps custody if its guard dies.

Process-group membership alone does not guarantee OS termination on owner
SIGKILL. The terminal's HUP and the memory guard are the normal crash path,
not proof that every descendant died. A process escaping the group, surviving
terminal close, or losing verifiable custody evidence is **residue**. It is
reported to the invoking stream while a custodian remains; subsequent doctor
and runs observations report only survivors whose checkout/run tags or fresh
identity evidence can actually be read. They never recover authority from a
PID, PGID, session number or old lock body. If owner and guard both die, there
is no replay, durable completion or durable residue acknowledgement. A process
that also discarded its tags may be unidentifiable; views state that limit and
never claim a complete inventory or successful cleanup. Discovered residue
blocks the affected owner until positively absent or explicitly acknowledged;
acknowledgement for a foreground run lasts only in the current custodian.
Guaranteed restartable recovery requires a durable class.

**The narrow terminal-record exception (effigy#325).** One deliberate,
bounded exception to "a foreground run leaves no durable record" (Tom,
2026-10-24; Queue decision b3538091, chosen option verbatim: "the admission
hook reports the workload's real process group and start identity, and Effigy
records a terminal result for foreground runs. Then Queue can settle dead runs
as lost and free slots on its own."). An orchestrator that opts in to
`effigy.admission-request.v2` (contract [049](049-heavy-validation-admission-contract.md))
makes Effigy write exactly one row to the owning checkout journal at settle,
in an additive schema-1 `foreground_terminals` table. An older reader selects
the columns it knows, so the addition needs no schema-version bump.

The row holds the run id, the `CheckoutId`, the boot identity, the task, the
outcome, the exit code or signal, `cleanup`, the acknowledged workload-group
count and the closed time. It is **evidence, never authority**: it carries no
PID, PGID or start identity, so no run, provider or built-in may signal,
reclaim, recover or settle from it. `run status <id>` reports it with
`evidence_source: "foreground-terminal-record.v1"`; `run logs` reports no
retained log and `run stop` refuses. The only closure Effigy asserts from it is
a terminal result with `cleanup: "complete"` (R5/I-R7). A run whose custodians
all died before settle writes nothing and stays `not-recorded`; there is no
replay. Every other foreground run keeps Q-010 unchanged, including no journal
write at all. The provider-side release rule this evidence supports is
contract 049's, and it is not an Effigy authority.

**Views and fencing.** Foreground runs have random `RunId`s and an in-memory
incarnation fence containing that id; persistent resource generations are not
incremented. No persisted request can address a foreground incarnation.
`runs` may show a live foreground row from a held gate only after verifying
its holder's boot/start identity and checkout binding. Without an owner query,
phase/outcome/log fields are explicitly unavailable. `run status <id>` checks
the current checkout's journal and verified live gate diagnostics; after the
foreground run exits it says `not recorded (foreground or expired)`, never
`succeeded` or `interrupted` from stale bytes. Discovered survivors are residue
observations, not reconstructed journal entries. `run logs` has no foreground
history; `run stop` may signal only a freshly verified live owner via
`Gate::signal_holder`, otherwise refuses with the residue diagnostic. Repeated
id or task spelling never attaches to a vanished foreground run. Use
`--run-id` when durable status, logs, idempotency or recovery is required.

`effigy tasks status <task>` and `tasks status --all` use the same verified
live-gate evidence even when no checkout token or run row exists. Contract
[018](018-task-status-query-surface-and-read-model-contract.md) owns their
projection: a verified live foreground invocation is `running`, unavailable
phase/outcome fields stay unavailable, and `current` is separate from durable-only
`latest`/`history`. A held gate without verifiable diagnostics is `unknown`,
never `never-run`. With no live run or unresolved durable predecessor the view
reports `not-recorded`; it cannot establish the last invocation's outcome from
an older durable result, which remains labelled historical. No token, free
gate or missing row proves the task never ran. Contract
[017](017-task-status-record-and-active-run-model-contract.md) owns the amended
identity/token, custody and storage rules. The versioned task-status read model
in 018 supplies the implemented v3 envelopes.

**Invariant amendments.** K3, I-R1 and I-P2 mean committed in memory before
go for foreground runs, and durably committed before effect for durable runs.
I-P3 and I-P4's retention and signal-evidence requirements apply to both.
K4/I-R5 resume only durable journals; foreground settle is idempotent within
its living custodians. K5/I-R2 and O13 prohibit unresolved *recorded* durable
predecessors and positively discovered foreground residue; they cannot promise
a persisted predecessor check after all foreground evidence is lost. K6 and
O1/O2 promise restartable closure only for durable runs; foreground loss is
reported with the custody limits above. K7's foreground fence includes RunId.
I-G4 permits a foreground owner gate across its workload; I-G5 requires memory
closure before unlock for it. §5.6's store transactions,
requests, `Interrupted` projection and §5.8's journal-backed sentinel apply
only to durable runs; an inline durable `--run-id` run still uses them. These
are deliberate Q-010 changes, not claims of the former durability without I/O.

## 4. Rules and invariants

Rules R1–R5 are the lane rules. Invariants K1–K15 make them enforceable.
Each lists the states it forbids. A brief or review that touches an engine
names the rule or invariant it relies on.

| # | Rule | Forbidden states |
| --- | --- | --- |
| R1 | **Host authority.** Every mutable resource has exactly one authority and one gate. Gates and the store live on the host. Guests never lock host gates or write host records. Effigy in a guest keeps working by requesting host authority (§8). | A guest holding or opening a host gate; a guest writing the host store; a host acting on a guest-resolved plan; authority inferred from a hostname, heartbeat age or a file in a bind mount. |
| R2 | **Pure reads.** Static and Observe commands write nothing anywhere. Observe may run guarded, non-mutating probes (§5.11). The narrowly scoped deep-doctor derived-cache exception is defined in contract 047: only a changed generation may be published; unchanged deep inventory writes nothing. | `help`, `status`, `tasks`, `--plan`, `graph status` or `doctor` (outside the contract 047 exception) creating, renaming or truncating a file, appending a Git exclude, allocating a port or address, rendering Compose, minting a scope token, fetching a bundle, writing the store or a journal, or acquiring a gate exclusively. |
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
| K7 | **Fencing.** Every effect, request and callback carries `(ResourceId, Generation)`. A stale generation is refused. A remote workload binds its environment's generation at intent; its acknowledgement and completion check the owner fence and the environment's identity, because every bring-up advances that generation, while container identity and VM boot (I-RC2) and holder protection (R4) keep it off a replacement (planner ruling, 2026-10-20). A stale-target intent writes nothing and leaves the run usable, so the caller re-reads and retries, as an attach does. | A stale stop stopping a successor; a late callback deleting a successor's route; a concurrent reuse bring-up failing a running remote step. |
| K8 | **Bounded.** Every kernel wait takes a deadline. Expiry cancels at a safe boundary and never detaches a writer. Only declared workload lifetimes are open-ended. | A `starting` loop that skips its deadline; curl without a time limit; a detached writer killed mid-transaction. |
| K9 | **One signal router.** One router per process, installed in `main` before any spawn. Libraries take a cancel token and never install handlers. HUP, INT and TERM are handled. | A library swallowing INT or TERM; HUP killing an owner with no cascade. |
| K10 | **Observability is not outcome.** A failed status, log or report write never changes a workload's outcome or exit code. | A succeeded task turned into an error by a status write; ENOSPC on a log failing the workload. |
| K11 | **Output independence.** Output format never changes execution. One output owner per invocation. | `--json` selecting a different executor, scheduler, timeout or cleanup. |
| K12 | **Secrets stay out of argv and global env.** Secrets reach a child only through its spawn spec's environment or a 0600 file or descriptor. Secret-bearing output is reduced to well-formed text whose only control is `LF` (plus the `CR` of a line ending) and then redacted, in merged arrival order, before every sink; the canonical captured tail is that same merged result, and a stream log or queue is redacted as its own byte stream. A redaction replacement is a byte readers keep and no secret contains, so no reader can join across it. | Secret bytes in `ps`-visible argv; `set_var` of secrets or run tokens; unredacted TUI transcripts; a secret split by a CSI or OSC sequence; a secret split by a C1 control or an invalid byte a renderer drops; a secret split by a lone carriage return or a TAB, which a display moves the next bytes with; a redaction replacement a reader drops (a NUL or other control); a stream sink assembled by filtering the merged result without redacting that projection; terminal-byte normalization after redaction. |
| K13 | **Safe files.** Every kernel open is descriptor-relative, no-follow, owner- and mode-checked and size-bounded. Mutation on a network filesystem is refused. | Following a symlink in the state root; reading an unbounded or non-regular file; flock on NFS or SMB. |
| K14 | **Acquisition order.** Multi-gate acquisition follows canonical order. No store transaction or allocator/resource gate other than an owner gate is held across a subprocess, prompt or network call. Native custody leases (§7.1) protect cleanup lifetime and never serialize allocation. | A global allocator lock held during `colima list`; an admin prompt under `routes.lock`. |
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
pub enum CheckoutKind { Primary, LinkedWorktree, EphemeralClone, Machine, Filesystem }

pub struct TaskId {
    checkout: CheckoutId,
    catalog: CatalogPath,        // root-relative, '/'-separated, "" for the root
    surface: Surface,            // published, or draft while drafts exist
    task: TaskName,
    profile: Option<ProfileName>,
}

pub struct RunId([u8; 16]);      // 48-bit ms time + 80-bit random; text: 26-char base32
pub struct Generation(u64);      // per resource, strictly increasing

pub enum ResourceKind { Checkout, Task, Run, Env, Data, Endpoint, Store, Gateway, HostProcess }
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
| HostProcess | checkout | environment name, profile, host process name |

`CheckoutId` tokens live in Git's per-checkout state. Linked worktrees and
ephemeral clones reuse the existing runtime-scope token
(`crates/effigy-core/src/worktree_scope.rs`), so Compose project and volume
names do not change. Primary checkouts get a token beside it. Only Prepare
and Mutate classes mint a token; Observe reports "no runtime state yet". The
`Filesystem` kind is never persisted as a checkout token or durable run owner.
Its domain-separated digest uses the canonical checkout path and its resolved
per-checkout Git directory path, never a mount-time device number or inode; its
scope names are separate even if a persisted token has the same raw bytes.
The existing kind tags 0–3 remain unchanged; tag 4 denotes this local-only kind,
tags 0–7 keep their meanings, and tag 8 is the `HostProcess` resource an
environment's host process run owns (effigy#320).

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
  cancellation never cancels cleanup. A second signal escalates it (§5.7). A
  cleanup step takes its window from the moment it begins: a context minted
  when a long-lived owner started bounds nothing about a stop or settle that
  follows hours later.
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
- I-EC3: executable sources (bundles, provider packages, catalog sources) are
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

Layout under the owning checkout or machine state root (§7):

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
        -> Result<T, StoreError>;      // BEGIN IMMEDIATE; the store's own wait window
    pub fn transaction_within<T>(cx: &EffectContext, wait: BusyWait,
        f: impl FnOnce(&mut Txn) -> Result<T, StoreError>) -> Result<T, StoreError>;
    pub fn maintain(cx: &EffectContext, pages: u32) -> Result<MaintainReport, StoreError>;
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

Regular backend output and interpolation-cache retirement use the same record
boundary: `open_output(cx, guard, dir, name)` checks an operator-owned private,
single-link regular file before truncating it, and `remove_record` verifies a
regular entry before unlinking it. Both require host effects and a store gate;
the caller retains the gate until child output descriptors close. Replacement
of a retained output tail or interpolation set uses `atomic_replace`. These
file operations open no database and create no run-history index. A private
interpolation cache uses `Gate::files` in its own directory: the same stable
inode locks and identity diagnostics, with only store-file authority and no
machine journal initialization. Checkout backend logs use their checkout gate.

SQLite settings: `journal_mode=WAL` (persistent), `synchronous=FULL`,
`foreign_keys=ON`, `journal_size_limit` and `wal_autocheckpoint` so the log
stays small, `auto_vacuum=INCREMENTAL` chosen while the store is empty, and a
`busy_timeout` from the store's own window. Write-ahead logging is what lets a
reader and a writer share the store; it replaced the rollback journal in
effigy#275 after concurrent runs refused each other with `Busy`.

Reads create no files and copy nothing: `Store::read` is one read-only SQLite
connection with `PRAGMA query_only` and a short busy timeout (K1, Q-011). A
store with write-ahead side files is read through SQLite's own WAL with a plain
read-only connection (`mode=ro`), so a writer beside the read is
no obstacle: WAL already gives one consistent committed state, and a crashed
writer's uncommitted frames are invisible to the reader and left untouched. The
read never asks for a read-only shared-memory index: that left the index
unwritable and the next write in this process failed. A
checkpointed store without side files is an immutable open, which needs no lock
and no index. A read that fails is `Unknown`: the foreground path warns once and
proceeds, and `runs` reports the journal as unavailable rather than refusing.

Every write takes the store's own writer gate (`Store(kernel)`), then one short
`BEGIN IMMEDIATE` transaction, and commits it. The gate serializes Effigy's
writers so a refused wait names a holder exactly as a resource gate does: its
diagnostic body carries the pid and, when it writes for a run, the run id.
A write waits for its own bounded window, never for the caller's remaining
deadline: at least 30 s for admission and effects, and up to two minutes for
settle and outcome writes (K4). Opening a current store reads it and takes no
write lock, so an ordinary run's admission is one short insert transaction.
Trust refusal rules are unchanged. SQLite opens files by
path, so checkout `kernel.db` and machine `machine.db` are the K13 exceptions to
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

- I-S1: related facts within one store commit in one transaction: a run's
  phase with its outcome; a claim with its reservation token; an attachment
  with its run. Cross-store endpoint effects use §7.5's fenced protocol; no
  transaction or foreign key spans checkout and machine stores.
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
- I-S8: a store write's wait is bounded by the store's own window, not by the
  workload's remaining deadline. A `Busy` refusal is an environmental
  condition that names its holder (pid and run) and never fails a task;
  settle retries long enough that a concurrent writer cannot cost a run its
  outcome, and a run whose settle still cannot land stays unsettled for its
  sentinel or the next holder of its gate to reconcile (K4).

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill mid-transaction | SQLite rolls back. The intent was not committed, so its effect never started (K3). |
| Crash, Kill mid-replace | The target is the old or new file, never torn. A leftover temporary is removed by the next writer under the gate. |
| ENOSPC | The commit fails and the operation stops before its effect. A replace leaves the previous file intact. |
| Busy or locked (another writer) | Wait the store's own window (30 s admission and effects, 120 s settle), then report `Unavailable` with the holder's pid and run (§6.1). Settle never reports a task failure for it. |
| Cannot open, or permissions deny the file | `Unavailable`, naming the path it could not open. Never `Corrupt`: the file is not wrong, its environment is. |
| Killed writer mid-transaction | SQLite rolls back the uncommitted write-ahead frames. A reader keeps the last committed snapshot without repairing anything; the next writer recovers. |
| Perm | Mutate refuses before any effect. Observe returns `Unknown(PermissionDenied)`. |
| Clock | Records carry wall time for display and journal sequence for order. Nothing compares wall times for authority. |
| Verified corruption of `kernel.db` | Observe reports `Unknown(store unreadable)`. Mutate reports `Corrupt` and names the explicit repair, which moves the store aside with its `-journal`, `-wal` and `-shm` files, so a leftover log never attaches to the next `kernel.db`. Resources it described become `Unknown`, so R3 protects them. A foreground run warns once and still runs. An invariant failure that is not corruption is `Broken` and never suggests the repair (§6.1). |
| Hot rollback journal seen by a read-only reader | `Unknown(store unreadable)`; the next Mutate recovers it. A foreground run warns once and still runs. |

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
        -> Result<Run, RunError>;   // §6.1: Refused, Unavailable or Broken
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

A spawn may hand its child one short-lived reader at `HANDOFF_FD` (11), above
the cleanup pin (10). The child reads one framed
payload and closes it; the parent keeps no copy once the spawn returns, and the
anchor process never holds it. That is how a session secret reaches an Effigy
child without argv or a persistent environment (contract 032; effigy#338), and
a workload never receives one. An engine helper receives the same slot through
`EngineLaunch::handoff`.

Spawn protocol:

| Step | Action | If the owner dies here |
| --- | --- | --- |
| 1 | `intend(Process { spec digest, label })` | `Intended`, nothing spawned |
| 2 | fork; child creates its group (`setsid` for a PTY), forks and waits for its anchor, then blocks on the barrier | barrier EOF exits child and pre-go anchor; no workload ran |
| 3 | parent records `{pid, pgid, start identity, boot identity, anchor}` as `Applied` | child and pre-go anchor exit; settle observes closure |
| 4 | parent writes go; child releases the anchor, then execs the workload | settle stops the workload and anchor by identity |

Every custody spawn sets `EFFIGY_CUSTODY_NONCE=<run>:<effect>` in the child's
environment; descendants inherit it. It is evidence, not a credential.
Before acknowledging the barrier, the blocked child forks an anchor inside
its already-created process group (and its session for a PTY). A transient
intermediate child reparents the anchor and is reaped before workload exec,
so the workload never inherits a hidden child to wait for. The anchor
re-execs the run's binary as `__kernel-anchor`, then acknowledges its pid. Its
environment contains only the custody nonce. The parent records the full
`{pid, start, boot}` identity in the optional `anchor` field alongside the existing receipt. Old receipts
omit that field and deserialize unchanged; imports infer no anchor.

The anchor reads a separate go channel until launch. Barrier EOF closes that
channel when the blocked child exits, so an unrecorded anchor cannot persist.
After the durable receipt and owner go byte, the child releases the anchor
before execing the workload. The anchor closes its last descriptors and
waits for custody settlement; owner or leader death alone cannot retire it.
Tom's 2026-10-27 ruling (Q-014) makes SIGTERM a cooperative exit and leaves
settlement as the other one. An idle anchor therefore ends within a bounded
grace when a harness stops the tree it started, and a run that completes
retires its anchor during settlement; no anchor outlives a settled run.
SIGINT and SIGHUP stay ignored, because a terminal delivers them to the whole
foreground group where the anchor is only the group's evidence.
Settlement stops the workload and verifies group closure before retiring the
anchor. If SIGTERM removes the anchor while workload members remain, its
positive group-identity evidence is gone and the existing `Unknown` rule below
applies; no numeric group signal or new settlement state is inferred. It
inherits no workload streams, gate descriptors or sentinel writer, and its output and
exit never become workload output or status. This costs one persistent
process and a transient reparenting fork per custody group. A protected OS
shell is unsuitable on macOS because its environment may be unavailable to
recovery; the native helper keeps the full nonce verification requirement.
`PR_SET_CHILD_SUBREAPER` helps reap Linux orphans but does not pin a group;
cgroups add Linux-only infrastructure and delegation policy. Neither replaces the
portable anchor needed on macOS.

A fork-only anchor in the blocked leader cannot obtain the new effect's nonce
through macOS `KERN_PROCARGS2`: it exposes the exec-time environment, even when
the libc environment was changed before fork. The existing single-threaded
sentinel is in another session and cannot join a PTY workload's group.
`fork_only_anchor_cannot_obtain_the_effect_nonce_or_join_a_pty_session` proves
both constraints. A fork-only anchor would require an execed custody leader
helper, or delegating workload creation and descriptor transfer to a sentinel
launched with its own recorded nonce. This implementation execs only the
anchor and preserves the existing workload barrier protocol. The run's own
binary is the only shipped image with the kernel entry point, so using it
avoids adding a separately distributed helper. After the leader's descriptor
closure, anchor setup closes only its newly created descriptors; the helper
closes the protocol's known inherited descriptors at startup.

Planner ruling (2026-10-21, effigy-bd): about 500 net production lines plus
proofs are authorized; proof and benchmark lines do not count toward the
original approximate 600-line tripwire. Spawn cost is the acceptance gate:
measure a release image with at least 20 paired samples on quiet and loaded
hosts, reporting p50 and p90 added latency. Target p50 is at most 25 ms; if
p50 cannot reach 50 ms, stop and report alternatives rather than ship.

If step 3 fails (ENOSPC), the parent kills the blocked child and returns
`SpawnError::Unrecorded`. A later spawn failure in `spawn_all` stops every
earlier child and returns both the primary and the cleanup errors.

Owner stop. The owner holds the leader's `Child` and does not reap it until
workload closure and capture drain are verified. With an anchor, TERM plus
CONT and then KILL target individually inventoried workload identities,
checking the anchor and membership before every signal. KILL inventories
repeat until closure, covering children forked during an earlier inventory.
The anchor itself is excluded from workload closure and killed last, after
no live workload member remains. Group closure is then verified, and only
then is the leader reaped. Output EOF and exit are separate events; capture
drains under its own deadline. Unanchored custody retains the unreaped-leader
group-signal path. `ESRCH` proves absence; unreadable observations remain
`Unknown`. Leader exit alone is never closure. A stop counts its window from
the recorded grace plus the drain, reap and anchor margin, and it never reads
the caller's cleanup budget as evidence about the group: a supervisor that
mints one budget when its run begins must not report that spent budget as
unverified closure while the group was closing (effigy#357).

Recovery stop. A sentinel or successor settles after the owner died; the
leader was reparented and its number is no longer pinned.

| Observation | Signal authority |
| --- | --- |
| Recorded anchor has its start and boot identity, custody nonce and recorded group membership | Its continuous presence pins the original group. Signal workload members individually after fresh identity and membership checks; Linux uses per-process pidfds, macOS has the documented check/signal window. Exclude the anchor from workload closure and stop it last. |
| Recorded anchor missing, killed, replaced or unreadable; workload members remain | `Unknown`; no anchor-derived signal, never a numeric group signal. An empty group still proves closure. |
| No anchor in the receipt; leader alive with the recorded start and boot identity | signal the group. On Linux 6.9 or later: open a pidfd, check identity, then call `pidfd_send_signal` with `PIDFD_SIGNAL_PROCESS_GROUP`, which targets the original group's kernel identity, not its number. Elsewhere (older Linux, macOS): a numeric group signal follows the identity check, with the window below. |
| Leader gone or identity differs, and `kill(-pgid, 0)` returns `ESRCH` | none needed: `Absent` |
| Leader gone or identity differs, and the group has members | signal each member individually, only if its environment carries the run's custody nonce and it started no earlier than the recorded spawn. On Linux, through a per-process pidfd opened before the check. A member that has provably left the group since the inventory (its pid is gone or a zombie) is re-inventoried, never residue. Other members: `Unknown`, residue, never signalled. |
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

Terminal handover (effigy#272): one owner runs any number of `Foreground`
children one after another — `effigy gateway setup` runs `mkcert -install` and
then `/usr/bin/sudo -H -- effigy gateway setup`. The rule has four parts.

- The workload's handover names the workload's own process group, read from
  the child after it became a group leader and before it ran. The group anchor
  joins that group; it never leads it and never holds the terminal.
- The owner blocks `SIGTTOU` on the calling thread around every `tcsetpgrp`,
  taking and restoring alike. A handover from the background — the previous
  child's lease, a restored owner group — is routine, never a stop.
- The lease ends when that child's exit is observed, which is where a wait or
  a poll first sees it, not when the anchor retires and not at settle. The
  owner's group is the terminal's foreground group again before the next
  sequential spawn, so no second take starts from the background.
- Only the owner's own group, or a group this owner already leases to a
  foreground child, is displaced. A terminal another program holds, or stdin
  that is not a terminal, takes no lease; the child keeps its inherited stdin
  and the run is unaffected. Restoration prefers a still-leased group and
  otherwise the owner, so no reaped group is ever named (`SIGTTOU`, `TOSTOP`
  and a group anchor are all exercised on a real pty).

#### Invariants

- I-P1: the owner exists before the first spawn. `spawn` takes `&mut Run`,
  which exists only after `Admitted` (K6).
- I-P2: no workload executes without a custody record including its ready
  anchor: durable for durable runs, acknowledged in memory by both custodians
  for foreground runs (§3.1). Both share the existing barrier and go byte; the anchor holds
  the group before either can release workload execution. No parent-side
  `setpgid` races an exec, including on macOS or for a PTY session.
- I-P3: the owner keeps the leader unreaped, and the group retained, until
  closure is verified, not until the leader exits.
- I-P4: signals need positive generation evidence: the owner's unreaped
  leader; in recovery, a verified anchor's continuous group membership plus
  the target's fresh identity and membership; or, for an unanchored receipt,
  the leader's verified start and boot identity or a member's custody nonce
  plus start time. Explicit Colima profile repair uses the selected Lima
  pidfile, exact instance arguments, pidfile write time and freshly rechecked
  process start and boot identity as its external-process authority. A
  non-empty group or session is never ownership evidence. No pattern kills
  (`pkill -f`). Missing evidence
  is residue only while the member is still present: a member that exited
  between the group inventory and its evidence read is re-inventoried, so a
  clean settle cannot report residue for a process that is already gone.
- I-P5: capture is bounded (tail ring plus rotating log) and, for a
  secret-bearing process, sanitized of terminal controls and then redacted at
  the source, in merged arrival order, before any sink (K12; output capture
  below).
- I-P6: secrets never appear in argv; no `set_var`.
- I-P7: one router per process (K9). Libraries take `CancelToken`. Rhai polls
  the token and never installs a handler.
- I-P8: a process that creates a new session escapes the group and remains
  outside custody. Clearing the environment loses the nonce but no longer
  escapes recovery when the recorded anchor is verified: the anchor proves
  uninterrupted group continuity after the leader is reaped. If the anchor
  was killed externally, or the receipt predates anchors (including v0.14.1
  imports), nonce-free survivors are still `Unknown` residue, never signalled
  by number. PGID, boot identity and a start after launch alone cannot prove
  continuity across an emptied and reused group. Guest-placement Effigy
  never creates sessions or clears the nonce.

#### Output capture

Capture is one pipeline over merged arrival order (K12, effigy#486). For a
secret-bearing process — one whose spawn spec carries a `SecretValue` — the
merged stdout/stderr bytes carry each byte's origin stream and pass through a
bounded streaming terminal-control sanitizer *before* redaction. CSI, OSC,
DCS, SOS, PM and APC strings, escape intermediates, two-byte escapes, the
other C0 controls, TAB and DEL are dropped; `LF` is the only control that
passes, because it is a line break no reader joins across. A carriage return
survives only as the CR of a `CR LF` line ending. The two horizontal controls
are dropped for the same reason: a display applies a lone CR by erasing what it
covers and expands a TAB to the next tab stop, so the bytes after either are
moved rather than printed where they stand — and a reader that strips the
control, or a soft wrap a TAB forces, is a join no separator breaks. A value
split by one would never be one run to the redactor; dropping them joins the
halves before redaction, which is the frame a display could show.

What reaches a sink is well-formed text: a multi-byte character is validated
before any of it is emitted, and a C1 control (`U+0080`..=`U+009F`, the UTF-8
class a terminal renderer drops, CSI and OSC among them) or an invalid byte —
a stray continuation, an overlong lead, a surrogate, a scalar past
`U+10FFFF` — is dropped. A renderer drops those, so retaining them would hand
it a join; dropping them here, before redaction, joins the halves in the
frame the redactor sees. A sequence's bytes are emitted with the tag of the
stream that began it, so a per-stream projection stays well-formed too. The
sanitizer holds at most one partial character, which EOF drops. Sanitizing first is what makes the redactor see the bytes a front
end shows: a value a child split with `ESC [ 1 m` is one value on screen and
two non-matching runs to a redactor over raw bytes, and a reader that
interprets controls reassembles it.

The sanitizer's state carries across reads and across streams and is O(1): a
sequence's bytes are discarded as they arrive, so no read or stream boundary
can split a decision and nothing is buffered. It fails closed — a malformed
or unterminated control drops its bytes, an open sequence never completes at
EOF, and no byte the machine cannot classify is emitted. Only the visible
bytes it emits keep their origin tags. Keeping the streams apart is itself a
byte removal, and a removal can join two halves of a secret that arrival order
kept apart — `super` on stdout, `noise` on stderr, `secret` on stdout leaves
the merged stream safe and would leave the stdout projection unsafe — so each
stream's stored log and queue is redacted again over its own byte stream
before it is written. The merged output queue and the merged tail stay
canonical: they are the only capture that keeps arrival order, and no consumer
concatenates two projections into one. A process whose spec carries no secret
skips both the sanitizer and the projection pass and keeps its colour. A
secret whose own bytes hold a control is also matched in its visible form, so
stripping the control from it cannot leave an unredacted remainder.

Redaction's own replacement has the same rule (effigy#486). A secret can itself
contain the display marker, or a marker can join adjacent bytes into another
secret, so a second streaming pass removes that occurrence. What it removes it
*with* must survive a reader: a control byte (a NUL, say) is dropped by the
front end's `sanitize_log_text`, which joins the bytes either side of it and
rebuilds the secret the pass took out. The replacement is therefore one byte
value that every reader keeps and no secret contains — printable ASCII, in a
fixed preference order — so it can neither hold a secret nor be spanned by one,
and every control, TAB included, is excluded because a reader drops it or a
renderer moves with it. When the secret set covers every such byte, no replacement is
both safe and readable: the capture then stops and reports `Truncated` with the
reason, emitting nothing, rather than write a marker a reader would join across.
The workload itself is unaffected; this is capture state only.

Every reader draws from that one result: the per-stream rotating logs, the
per-stream queues, the merged output queue, the bounded merged arrival-order
tail (`tail()`, the canonical captured tail), the bounded diagnostic excerpt a
settle or doctor report quotes, and any JSON envelope carrying their bytes.
Nothing normalizes terminal bytes after redaction; a post-redaction
normalization path would recreate the split. A consumer that needs both
streams reads the merged capture, never a concatenation of the projections,
and a consumer that wants a bounded tail takes the merged tail.

Host producers use `Custody::spawn_logged_auxiliary` with captured pipes and
one `HostOutput` shared across child generations and supervisor messages. The
existing custody sanitizer records style data and feeds the existing two-pass
redactor in merged observed arrival order. Child EOF resets terminal framing;
only supervisor completion finishes the shared redactor. CR is removed from
CRLF before redaction. Complete UTF-8 text is appended and synced before any
live/tail fanout. A failed append poisons capture; no unwritten output is
acknowledged. A partial failed append can leave an unacknowledged plain suffix.
Managed Standard workloads opt into `Terminal::Styled`, which persists their
merged plain output in the stdout log (the stderr log is empty). Ordinary
custody callers retain their existing separate-stream contract.

Tom's 2026-10-30 one-time cutover ruling accepts that safe appends cannot make
an untrusted historical prefix safe. Under the newly admitted host owner gate,
with predecessors settled, an owned regular no-follow legacy log moves to the
same runtime directory's private `.legacy-host-logs` quarantine. Its name ends
in `.legacy`, not `.log`; discovery never includes it. A new 0600 canonical
file is created exclusively at the same public path. Intent, file and parent
directory syncs precede producer admission. The durable private ready record
`.<sha256(log basename)>.host-log-ready` is version 1, with an `owner` array of
32 digest bytes and `canonical: {device, inode}`. A matching ready identity
allows subsequent generations to append without another cutover. Changed,
foreign, symlinked or uncertain identities fail closed, preserving operator
data. Crash retries retain the original intent and quarantined identity.

Effigy readers require matching durable ready provenance before opening
payload bytes and recheck identity before exposure. Bounded followers reset
both offset and partial text on inode replacement or shrink, reporting a gap;
replacement is detected even when the new file grew beyond the old offset.
The bundled Underlay presentation follower tracks device/inode and offset,
resets on replacement/shrink and reports one gap. It does not enforce Effigy
ready provenance. Before cutover its behavior remains the pre-existing owner
file contract. An already-open pre-upgrade POSIX descriptor cannot be revoked
by rename/chmod: stop and reopen older attached followers after upgrading.
Continuity across that transition is not guaranteed, and numeric offsets are
never preserved by padding or replay of raw bytes.

Style primitives and the existing ANSI line/parser/SGR implementation live in
`effigy-core::terminal_text`; the TUI facade reuses them. The writer records SGR
as bounded style data. Redaction clears style on masked ranges. A bounded
private `.styled-tap` sidecar publishes a window of typed, already-redacted
spans only after successful plain append. It is optional presentation data,
not a log checkpoint: it grants no durability acknowledgement, changes no
canonical offsets and never replays an untrusted prefix. A pane uses styles
only where identity, byte offset and text exactly match its canonical read;
missing, stale or expired style data renders plain. TUI typed ingest renders
those spans directly, without ANSI interpretation or a second redactor.

The initiating TUI capability is non-secret and fixed before detached host
handoff. Cargo `always` and `CLICOLOR_FORCE=1` are selected only for that ANSI
destination in the absence of NO_COLOR, CI, a dumb terminal or an explicit
colour preference. Text/headless/detached starts do not force colour.

**Known qualification limit:** the migration, complete fault/security matrix,
combined bundled follower proof and independent exact-head reviews must pass
before the provisional host-log limit and manual merge hold can be released.
A writer primitive alone does not qualify the delivered path.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill of owner before the go byte | The child exits on barrier EOF. No workload ran. |
| Crash, Kill of owner after the go byte | The sentinel or the next gate holder stops the recorded group by identity. |
| Anchor killed externally | Live workload members become `Unknown` in anchored recovery; no signal is authorized by the dead anchor. |
| Crash at anchor spawn or record | Pre-go barrier EOF retires the blocked child and anchor; after record the sentinel can observe both. |
| Crash at anchor stop | Workload closure was already verified. Recovery stops a surviving verified anchor or observes the already closed group. |
| SIGSTOP of a child | CONT follows TERM; KILL works on stopped processes. |
| ENOSPC on the custody record | Child killed before exec; `Unrecorded`. |
| ENOSPC on logs | Capture continues in the ring; the log is marked truncated. The outcome is unchanged (K10). |
| Perm (`EPERM` on signal, for example a setuid child) | `Unknown` residue; no escalation by other means. |
| Clock | Grace uses `Instant`. |

#### Proof

- `anchor_recovery_reaped_nonce_free_descendant_and_lost_anchor`: real
  nonce-free orphan recovery after reaping the leader (ordinary and PTY),
  externally killed anchor refusal and old-receipt refusal.
- `anchor_spawn_overhead_is_measured`: matched before/after measurements
  with 24 alternating release-build pairs per acceptance condition, p50 and
  p90 added latency, host load, and a workload with no hidden child to reap.
- `fault_matrix_registration_spawn_and_socket_kill_enospc` and the custody
  fault scenarios cross `AnchorSpawn`, `AnchorRecord`, `AnchorStop` and
  `AnchorStopped`, including owner SIGKILL. The driver waits for writer
  helpers before recovery and requires all recorded nonwriting anchors to
  retire after recovery, including anchors that never reached registration.
- `custody_prepare_source_requires_its_class_and_settles_with_a_sentinel`
  qualifies Prepare acquisition through `Custody::spawn_source`: successful,
  failed and deadline-expired children settle through the registered sentinel.
  Prepare cannot use ordinary `spawn`, and Mutate cannot use `spawn_source`.
  Both entry points share the custody fault points and recovery machinery.
- SIGINT, SIGTERM, SIGHUP and SIGKILL to the client, crossed with plain,
  sequence, managed and container runs: settle reports `Complete` and no
  fixture-tagged process survives (O1a). The signal matrix runs on Linux
  through the private disposable-VM route (`test:task-engine:kill-matrix:linux`,
  `crates/effigy-kernel/tests/scripts/linux-custody.sh` mode
  `task-engine-fixtures`), never the operator's Colima profile, contexts or
  another VM. A statically resolved plain foreground tree keeps no runtime
  record, so its proof is the released foreground gate plus settled
  descendants and no later step, not a journal row; a declared `durable = true`
  tree keeps the closed row this contract reads.
- A failure spawning child 2 of 3 leaves no survivor and reports both errors.
- A descendant that ignores TERM and keeps the stdout pipe open is killed,
  and capture ends at its drain deadline.
- Reused group: the process-table observer is a kernel trait, so the
  harness can show the recorded pgid occupied by a foreign group without the
  nonce after the leader is gone. Recovery signals nothing and reports
  residue. The same holds for a foreign process sharing a pgid with a
  surviving nonce-carrying member.
- A member that exits between the group inventory and its evidence read: the
  observer re-inventories a vanished member instead of reporting residue, and
  a present one without the nonce still reports residue
  (`recovery_foreign_reused_group_unknown_and_nonce_evidence_oracle`, case 6
  and cases 0/2/3).
- Leader exits while a nonce-carrying descendant survives: owner stop kills
  it through the group, whose number the unreaped leader keeps allocated;
  sentinel recovery kills it through the nonce.
- Recovery verifies a live leader, which then exits, and a foreign group
  takes the number before the signal (injected): with the group capability
  nothing foreign is signalled. Without it, the case is the documented
  window and is reported as such.
- A descendant that clears its environment survives recovery as `Unknown`
  residue and is never signalled by group number.
- `custody_sequential_foreground_spawns_keep_the_owner_in_the_foreground` and
  `custody_sequential_foreground_spawns_survive_tostop`: on a real pty, one
  owner runs two `Terminal::Foreground` children in sequence, the first
  writing to the terminal and the second reading a line from it, with
  `TOSTOP` set in the second case. The owner never enters state `T`, each
  child reads and writes the terminal, and the owner's own group is the
  terminal's foreground group after each child exits — before the run
  settles (effigy#272).
- `nononce_foreground_takes_the_terminal_from_the_background_and_hands_it_back`:
  the same handover without the group anchor (`Blocked::fork_with_nonce`),
  where the second child is given the terminal while the first still holds
  it, so the take happens from the background. Each handover names the
  child's own group, and the lease released first returns the terminal to the
  group still leased, then to the owner.
- `proof:gateway:setup:terminal` runs
  `setup::tests::setup_sequence_runs_two_foreground_commands_in_one_run`: the
  gateway setup sequence (`mkcert -install`, then `/usr/bin/sudo -H -- effigy
  gateway setup`) through `command(…, terminal = true)` twice in one run
  against fake absolute programs, settling with `Complete` cleanup.
- No secret bytes appear in `ps -o args` during any run.
- `redaction_sanitizes_controls_split_into_known_secrets_before_every_sink`
  drives the shipping capture pipeline and capture logs with no live child:
  a known secret split by CSI, OSC (BEL and ST), a CSI or an OSC split across
  reads, a CSI or an OSC split across streams, interleaved streams, a
  malformed CSI and a two-byte escape. Every sink — both per-stream logs,
  both queues, the merged output, the merged tail, the settle excerpt and a
  JSON envelope — is free of the value and of any terminal control, and the
  merged tail keeps the redaction marker.
- `redaction_drops_an_unterminated_control_and_its_tail_at_eof` shows the
  fail-closed rule: an unterminated OSC, CSI or DCS word drops everything
  after its introducer, and a later read never resumes inside it.
- `redaction_redacts_a_projection_that_joins_a_secret_across_an_interleaved_stream`
  pins the projection rule: with the halves on one stream and the other
  stream's visible bytes between them, the merged capture never held the
  value, and the stream's own log and queue show the marker instead of the
  joined halves.
- `redaction_drops_utf8_c1_controls_and_invalid_bytes_before_redaction` splits
  a value with a C1 CSI, a C1 OSC/ST pair, C1 padding, an invalid byte, a
  stray continuation, an overlong lead, a surrogate and a scalar past
  `U+10FFFF`, across reads and across streams: every sink is well-formed,
  control-free and free of the value, ordinary non-ASCII text survives, and a
  secret that itself carries a C1 control is matched in its C1-free form.
- `redaction_drops_a_tab_that_a_renderer_would_shift_the_next_bytes_with` splits
  a value with a TAB in one read, across reads and across streams, and with
  padding that wraps a row: every sink is free of the value and of the TAB, and
  an ordinary process keeps its own TABs.
- `redaction_drops_a_lone_carriage_return_and_keeps_a_line_ending` drops a CR
  inside a value, at a read boundary, at a stream boundary and at EOF, so the
  halves reach redaction as one value and no sink holds a lone CR; a `CR LF`
  line ending keeps its CR, and a secret that itself carries a CR is matched
  in the CR-free form a display shows.
- `redaction_replaces_a_reformed_secret_with_a_byte_no_reader_drops` puts a
  secret that is a substring of the display marker, and a marker that joins
  adjacent bytes into another secret, through the shipping pipeline: every sink
  is free of the value, holds no byte the front end drops, and the reader's own
  filter of the sink is the sink.
- `redaction_stops_capture_when_no_reader_safe_replacement_exists` covers the
  corner where the secrets cover every kept replacement byte: every stored sink
  is empty and the capture reports `Truncated`.
- `redaction_keeps_terminal_colour_for_a_process_without_secrets` and
  `redaction_covers_the_visible_form_of_a_secret_that_carries_a_control`
  keep ordinary colour intact and redact a secret that itself holds a
  control, in its visible form.
- `managed_delivery_redaction_sanitizes_terminal_controls_before_the_tui_reads_them`
  is the end-to-end counterexample on the managed path: a live secret-bearing
  child writes the split value, and the managed TUI's own reader
  (`ingest_log_payload` then `ansi_line`, the existing main path) shows the
  redaction marker and never the value, and its last case is a real
  pseudo-terminal whose display goes through the session's terminal emulator
  (`LiveTerminalBuffer`), which applies carriage returns, and a third split by
  a UTF-8 C1 control, whose break probe shows the emulator dropping that
  character and joining its neighbours. A fourth proof feeds a TAB-split value
  to the same emulator and shows the pane dropping the cursor-forward escape a
  TAB renders as, which is the join this task removes. Its break probe feeds
  the pre-fix arrival shape to that same reader, which reassembles it, so the
  assertion fails if the sanitizer is removed. The companion
  `managed_delivery_redaction_keeps_colour_for_a_child_without_secrets`
  keeps an ordinary child's colour.
- A Rhai script cannot swallow a stop.

### 5.8 Placement, supervisor and sentinel

#### API

```rust
pub enum Placement { Inline, Detached, Guest }

pub enum ClientMode { Owning, Detach }

pub fn start_detached(cx: &EffectContext, inv: &ResolvedInvocation, client: ClientMode)
    -> Result<SupervisorClient, RunError>;   // §6.1: Refused, Unavailable or Broken

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
- I-SV4: the supervisor and sentinel exec whatever binary the startup path
  names, so a rebuild or reinstall during a run is ordinary. The helper
  handshake frame carries a protocol version; a binary that speaks another
  version refuses before running anything, naming both versions.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Supervisor Crash or Kill | The sentinel settles. |
| Supervisor and sentinel both Crash or Kill | Status shows `interrupted`; the next Mutate on the owner, or `stop`, settles. |
| Sentinel or supervisor dies, or stops answering, before it acknowledges | `Unavailable` (§6.1), naming the helper, how it ended, the parent's handshake cause and the helper's stderr tail; never a store error and never a refusal. The end is its code or signal; `Stopped` when it was still running after a 2-second bound (it is the parent's own unreaped, unregistered child, so stopping it is identity-safe); or `NotStarted` when the spawn itself failed. Until it acknowledges, the helper's stderr is an unlinked private file under the state root; the outcome carries its last 4 KiB, and the hidden entry writes its refusal there before exiting. After acknowledgement stderr returns to the null device. An admitted run then settles `Closed(NotStarted)`: nothing ran on the helper's behalf. |
| ENOSPC on the socket path | The admitted run launches no workload and settles with nothing applied: `Closed(NotStarted)`. If closing also fails, the run reads Interrupted under §5.6 recovery. |
| Perm on the socket | The peer-uid check refuses. The request row remains; the supervisor reads requests when it polls the store. |
| Clock | No time authority. |

Planner ruling (effigy-a6, kernel lane owner, 2026-10-19): admission precedes socket bind, because K3 requires intent before effect.

Long checkout paths do not relocate control state. Socket bind and bounded
connection use a verified directory descriptor and one component: Linux uses
its own `/proc/self/fd/` directory reference; Darwin performs the relative
operation on a fresh joined pthread with a thread-local cwd. The invoking
thread and process cwd are unchanged, and no helper process can bind after
owner death. Socket intent, inode receipts and exact-inode settle remain the
same. The Darwin signature and return contract are pinned in Apple's
[libpthread header](https://github.com/apple-oss-distributions/libpthread/blob/42d026df5b07825070f60134b980a1ec2552dfee/private/pthread/private.h#L65-L87).


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

pub enum ClaimState { Pending, Committed, Releasing }

impl Claims {
    pub fn reserve(txn: &mut Txn, holder: &ResourceId, run: RunId, wanted: &[ResourceId])
        -> Result<ReservationToken, Conflict>;             // all or none, one transaction
    pub fn commit(txn: &mut Txn, token: ReservationToken, revision: u64) -> Result<(), StoreError>;
    pub fn release(txn: &mut Txn, token: ReservationToken) -> Result<(), StoreError>;
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
- I-A4: a released claim frees its resource immediately; nothing waits out a
  reuse period.
- I-A5: the release policy (`stop_on_release`, idle stop, persistence) is the
  environment engine's. The kernel only guarantees the holder protection.

#### Failure model

| Fault | Behavior |
| --- | --- |
| Crash, Kill of a holder | Its attachment stays; liveness becomes `Interrupted`; R4 holds until settle. |
| Crash, Kill mid-reservation | The transaction rolls back; nothing reserved. |
| ENOSPC | Reservation or attach fails before the effect. |
| Perm | Reservation refuses before any effect. Holder observations become `Unknown`, which protects (R4). |

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

Implemented beyond the sketch above (effigy#232): `publish_active_for`
inserts each requesting run's attachment in the environment apply run's
final transaction (the `EnvNotActive` row); `permit_exempt` is the sketch's
`exempt`, admitted only once that holder is `Stopping` or `Closed`;
`TransitionPermit::publish_stopped` publishes `stopped` with a fresh holder
check; `Run::set_engine_record` stores the authority engine's own record on
its resource row (bounded, fenced); `StoreReader::object_refs` and
`run::summary` are pure reads. Settle records a `Retain` or `Irreversible`
environment effect without a holder check, because recording calls no adapter.

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
    fn terminal(&self, id: &ContainerIdentity, requested: RemoteTerminal)
        -> RemoteTerminal;                      // effective; never above `requested`
}

pub enum RemoteTerminal { None, Stdin, Tty }  // how the exec client meets the operator

pub struct RemoteCommand { argv, env /* private, never argv */, terminal: RemoteTerminal }

pub struct RemoteRegistration {
    env: (ResourceId, Generation),       // bound at intent (K7)
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

#### Rulings (planner, 2026-10-20, effigy#236)

- **Fence.** Intent binds the environment generation current at launch.
  Acknowledgement and completion check the run's owner fence and the
  environment's identity, not its transition generation (K7). A concurrent
  reuse bring-up therefore leaves a running step's outcome truthful; a
  replaced container or a VM reboot is still refused by I-RC2.
- **Nonce recovery.** `find_by_nonce` never receives the fence: the kernel
  binds the intent's own fence to what it returns. `Verified(None)` is the
  adapter's positive proof that the known, live container on the registered
  boot was inventoried completely and no process carries the nonce, so the
  workload never started (R5): the effect is `Released` and reruns are not
  blocked. A stopped container proves the same. Anything unobservable is
  `Unknown` and stays residue under K5.
- **Terminal.** `Stdin` and `Tty` inherit the owner's stdin with both client
  streams captured; `Tty` asks the backend for a terminal, which merges the
  workload's streams, so its frame arrives on stdout ending `\r\n`. `None`
  keeps EOF stdin. The adapter reports the terminal its backend actually
  gives (`terminal`); the kernel launches and frames with that effective
  terminal and never lets a backend upgrade the request. The wrapper keeps an exec that is already a session
  leader (a backend terminal makes it one) and never lets `setsid(1)` fork a
  group leader, so the exec client waits on the registered leader.

#### Implemented (effigy#236)

- The production adapter is `effigy_containers::environment::ComposeRemote`
  for Colima and Docker (Desktop on macOS, Engine on Linux). A container's
  identity names its exact engine, context and full id
  (`engine:full-id@context`), so settlement during sentinel recovery reaches
  the same backend; Docker's ambient default context is resolved to its name
  at launch.
- Forwarded values never enter argv (K12). A fixed host relay sends them as
  `KEY=value` lines from its own private environment: Docker reads them as an
  env file on a private descriptor, leaving stdin to the operator; Colima's
  VM shell reads them from stdin, exports them and passes them to `nerdctl` by
  name. Values that span lines are refused. Colima cannot carry a terminal
  while its stdin carries the values, so its adapter reports `Tty` as
  `Stdin`: the frame stays on stderr and its launch refuses a terminal.
- A fixed POSIX helper runs as root in the registered container: it lists
  `/proc` with session facts and nonce evidence, and rechecks boot,
  namespace, start ticks and nonce immediately before each signal.
- The task engine's `TaskSettlement` settles remote sessions through the same
  adapter, inline and in sentinel recovery.

#### Proof

- A conformance suite every backend adapter must pass: kill at each step,
  container replaced, VM restarted, image without `setsid`, exec client
  killed with the remote still running, and a recorded session number reused
  by a foreign session without the nonce, which is never signalled. The
  production adapter passes it over a strict container model
  (`test:containers:environment`); the helper itself is proven against a real
  `/proc` on Linux.
- The task engine's remote matrix (`test:task-engine`) runs a container step
  whose workload is a real host process group apart from its client: client
  death (interrupted, then the session stopped), container stop (the
  workload's own end), engine restart (absent by boot) and an unreachable
  engine (residue, never a kill, settled by the next run), each ending with
  no surviving process. Owner death is proven out of process by the kernel's
  `owner_sigkill_at_each_remote_step_recovers_from_the_durable_journal`: an
  in-process owner never dies, so its sentinel would race its successor.
- Real-engine qualification on Colima (docker and containerd), Docker Desktop
  and Linux Docker. Apple Containers must pass before it is supported. This
  is a qualification-lane step (planner ruling, 2026-10-20): the relays,
  helper and identity resolution above have not yet run against a live
  engine. The recorded steps, on each backend with the candidate binary:
  1. cbs: `effigy schema:refresh`, then `effigy seed`; compare the dumped
     schema files and step outcomes with v0.14.1; repeat `seed` under
     `--json` and compare `steps`.
  2. Acowtancy: `effigy workspace:js:prepare` (built-in `exec` step) in a
     terminal and under `--json`.
  3. During a long step: kill the exec client, `container stop` the service,
     restart the engine (`colima restart`, or restart Docker Desktop) and
     SIGKILL Effigy. After each, `effigy run list` shows a settled run and
     `ps` inside the container shows no survivor carrying the nonce.
  4. An interactive container task in a terminal: Docker gets a TTY,
     Colima stdin only.

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
    TimedOut { phase: String }, Busy { phase: String },
    // A stop request ended the work before it could be observed: the detail
    // names what was stopped, so it never reads as contradictory evidence.
    Cancelled(String),
    PermissionDenied,
    ToolUnavailable(String), Unparsable(String), Inconsistent(String),
    GuardUnavailable, Injected, Untrusted(String), HostAuthorityRequired,
    StoreUnreadable(String),
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

### 6.1 Error categories (K4, effigy#360)

Every kernel and run error is exactly one of three categories, and each
category has one `Display` and one next step (`effigy_kernel::human::Category`).
The error's own text names the rule, what was waited on, or the check that
failed, and never contradicts its category.

| Category | What happened | The one next step it carries |
| --- | --- | --- |
| Refused | The request was well formed and a rule said no: another run holds it, a conflict, an inactive environment, a stale generation, a placement without host authority, an effect class the operation may not use. | Nothing ran; stop what holds it or change the request, then retry. |
| Unavailable | Something environmental did not answer in time or could not be reached: a busy store, a timed-out gate, a helper that exited before acknowledging, I/O, a path that cannot be opened or permissions that deny it (both naming the path), a failed step, a run a signal or stop request cancelled. | Nothing is broken, and retrying once it answers is safe. |
| Broken | Effigy's own record or an invariant is wrong. | This is an Effigy bug, not your request; report it with the check it names. |

Only verified corruption — SQLite `NotADatabase` or `DatabaseCorrupt`,
`PRAGMA integrity_check` or a foreign-key violation, or a format Effigy wrote
that it can no longer read — says `Corrupt` and names the one repair:
`Store::repair` moves `kernel.db` and its `-journal`, `-wal` and `-shm` files
aside, so no leftover log attaches to the next `kernel.db`. A journal mode or
binary that no longer matches is `Broken`, never `Corrupt`, so a pin or token
mismatch cannot point a reader at a destructive repair (effigy#360). A journal
written by a *newer* schema is version skew: it says `Effigy was updated`,
reports `Unavailable`, and never suggests moving a healthy journal aside.

A cancellation is not a refusal. A run, a gate wait or a store wait that a
signal, a stop request or an owner exit ended renders as `cancelled by SIGINT`
(and the like) with no category's next step, and reports `Unavailable` where a
category is carried across a process boundary (F7).

The same three kinds hold outside the kernel: `RunnerError::category()` maps
every runner failure to `Refused`, `Unavailable` or `Broken` instead of
reporting everything as a refusal, a supervisor relays the category it read,
and `--json` carries it as `error.kind` with the error's own type in
`error.type` (F16, K4).

A busy store, a locked database, `SQLITE_CANTOPEN` and a permission refusal are
`Unavailable`, naming the path: they are environment conditions, and retrying
after the cause is fixed is safe. A journal mode that is not the write-ahead log
Effigy sets is `Broken`: the file is readable, so it is an invariant failure and
never a repair. A failed step renders as `RunError::unavailable(step, cause)`.

A run that helper processes admit crosses a process boundary, so a refusal does
too: the helper renders its category's sentence, sends it with the category, and
the client prints it verbatim (`RunError::Reported`). A refusal is therefore
framed exactly once, by the process that read it — a busy store at managed start
stays `Unavailable` instead of being wrapped in a refusal. A managed supervisor
that cannot admit its run carries the kernel's own category
(`RunnerError::RunAdmission`), so relaying it never relabels it either.

There is no invalid-request error in the kernel. A name this checkout does not
route is resolved before the kernel reaches admission, and the message that
names `effigy tasks` comes from that resolution
(`RunnerError::TaskNotFound`/`RoutingError::TaskNotFound`), so it appears only
when a user actually named something that is not a task (effigy#360).

| Fault | Kernel guarantee | Not claimed |
| --- | --- | --- |
| Crash or Kill anywhere | Intent before effect means every effect is findable; one settle resolves it later. | That the effect did not happen. |
| ENOSPC | Every write fails before its effect or preserves the previous generation. | That work in flight completes. |
| Perm | Refuse before effects; observations become `Unknown`. | That an `Unknown` resource is safe to act on. |
| Clock jump, suspend | No authority from wall time; gates have no time. | Accurate displayed timestamps. |
| SIGSTOP of a holder | Never stolen; waiters time out naming it. | Progress. |
| Lost group anchor | Unverifiable live groups stay `Unknown`; no numeric group signal. | Cleanup of nonce-free descendants after anchor loss, or session escapes. |
| External boundaries | At-least-once attempts with idempotent undo, inspect-and-reconcile, or a recorded `Irreversible` or `Unknown`. | Exactly-once delivery across filesystem, engine or network. |

## 7. Store layout

### 7.1 Location, trust and clocks

Durable checkout state belongs to the resolved invocation root, not a nested
catalog's cwd: identity, environment journal, render objects and container
labels resolve through the nearest enclosing directory carrying `.git`, so a
nested catalog directory with no Git metadata of its own declares no second
checkout, while a nested repository or submodule keeps its own. Each linked
worktree and each clone owns its own `<checkout>/.effigy/state/`, including a
primary checkout. They never share the
primary's journal: independent deletion, gate contention and recovery must be
independent. Existing Git runtime-scope tokens, Compose names and volume names
are preserved. Resource identity is distinct from storage routing: a globally
unique RunId and its machine-scope canonical encoding do not put a task run's
records or gate in the machine root. They are colocated with its owner checkout;
only §7.5 authorities route to machine storage. Both token-based and initial
filesystem-identity admissions take the local initialization gate first, so
creating a token cannot split a resource across two lock names.
A durable Prepare establishes the existing `.effigy/` ignore
block; foreground admission and all reads never append it.

```text
<checkout>/.effigy/state/
  kernel.db                         checkout journal (+ kernel.db-wal and -shm while open)
  gates/<scope>.scope               scope lock
  gates/<scope>/<rid>.gate          per-resource locks, including foreground owners
  sock/<run-id>.sock                durable supervisor socket
  runs/<run-id>/                    durable redacted rotating logs
  objects/<sha256>/                 immutable render/source closures
  staging/<run-id>/                 durable run-owned staging
  migration-committed.json          old import record; ignored, never read
  migration-prepared.json           old import record; ignored, never read
~/.effigy/state/
  machine.db                        only §7.5 shared facts (+ -wal and -shm while open)
  gates/<scope>.scope               machine scope locks
  gates/<scope>/<rid>.gate          shared-resource locks
  migration/                        old cutover manifest and receipts; ignored
```

The state root and its family directories are real, operator-owned 0700
directories; files are 0600, no-follow, descriptor-verified and bounded (K13).
Only `.effigy/state` itself is owner- and mode-checked: an effect creates it
0700 or tightens an existing operator-owned directory to 0700, and nothing
else. Ancestors, including `.effigy`, are reopened no-follow but their owner
and mode are never consulted, so a umask-002 checkout, a CI container or
`/Users/Shared` is ordinary.
Mutation on NFS, SMB, WebDAV, FUSE or sshfs refuses. SQLite keeps §5.5's
WAL/FULL settings. Private fixtures use a fresh `HOME` **and independent
checkout roots**, never real checkout links. No production state override is
needed. Wall clocks are display only; persisted deadlines retain
boot identity and boot-monotonic nanoseconds. A different boot ends them.
The state subtree, secrets and control sockets are masked from workspace guest
mounts; guests continue through the host authority channel (R1).

`.effigy/state/` is kernel-owned. The `effigy state` surface's own artefacts
(capture/apply hook context and its recommended capture payload paths) live
beside it under `.effigy/state-captures/`, so writing them can never create or
loosen the private root. For one release an effect adopts a `.effigy/state/`
that v0.14 `effigy state` left at an operator-owned, not-group-writable mode by
tightening it to 0700 and keeping its contents. That covers every effect path,
including a foreground task tree and a retained run: neither refuses the
checkout's cutover on the mode alone, and adoption creates nothing, so a fresh
destination is still never created before the migration decision. Observation
never repairs a mode it does not trust, and a mode another user could write is
still refused.
A checkout whose own records cannot be read is recorded against that checkout:
dead-owner and port recovery hold its entries only, never every owner on the
machine.

A durable checkout journal opens by its location: `<checkout>/.effigy/state/`,
and keeps no record of which checkout or Git generation it belongs to (Q-011).
A volume renumbered at mount, a recreated Git directory, a `mv` of the checkout
and a reboot are all ordinary: prior-boot runs are already history through the
boot identity each record carries, and `st_dev` is assigned at mount, so no
durable decision may depend on it. Tom, 2026-10-25: "I thought we had simplified
the kernel.db dependency? This is exactly the sort of overcomplication and
brittleness I wanted to avoid." A legacy central journal requires explicit
migration before any fresh durable destination is created. A checkout remains
usable independently of machine-store health; local authority does not consult
machine state. A durable machine effect likewise writes no layout record: the
machine store is a location, and a moved home is rebound by its next effect
rather than refused.
Fresh task and environment identities share one token; both distinct legacy
primary tokens remain valid when imported. Non-Git durable roots persist a
random token in `.effigy`; path hashes cannot establish ownership of old state.

Deleting a checkout deletes its journal, logs, objects and local locks. It is
not a stop or successful settle. Supported `container retire` first settles
runs, withdraws routes/releases claims and verifies closure under scope locks;
only then can an operator remove the directory. Effigy does not prevent an
external `rm -rf`. Living custodians retain open gate/root descriptors and
refuse new effects if the retained checkout, state directory or Git locator is
replaced under them; they finish cleanup from their verified receipts where
possible. Restoring a copied `.effigy/state` into a recreated directory opens
that journal by its location and rebinds its record: that is a deliberate copy,
and no recorded device or inode is what distinguishes it. Missing state plus
surviving containers or processes is residue, never permission to kill by
number. Machine endpoint/port claims survive deletion and protect their values
until explicit verified retirement; unknown owners are not garbage-collected.
No machine copy of the deleted checkout's journal exists.

Dead-worktree retirement cannot rely on a deleted journal or a central checkout
index, and needs neither. `container retire --dead` reads the backend's Effigy
ownership labels and releases the checkout's gateway, port and loopback state by
checkout path. Dead means the path is missing; a relative or unreadable path
label is reported and never guessed about. Every resource of a dead group is
deleted by the exact name the backend listed, and a deletion that fails is named
and fails the command. A missing checkout on an unavailable volume is reported
under `unknown`, never as dead.

### 7.2 Tables and transaction boundaries

The §5.5/§5.6 record shapes remain in the checkout journal: `meta`, the owning checkout
identity and verified import aliases in `checkouts`, `resources`, `runs`, `events`, `attachments`, local `claims`,
`requests`, `objects`/object references and `logs`. No row represents another
checkout's task history. All environment records, renders, backend metadata,
data ownership, remote receipts and holder runs live here. Foreground runs
create none of these rows. A checkout journal failure affects that checkout;
no common database is opened for task begin, spawn or settle.

An environment holder and its environment authority use the same checkout
journal, preserving atomic attach/activation and R4. An explicit borrow from
another checkout is mediated by a durable holder session in that environment's
owning checkout, with a fenced request/receipt from the caller's durable run;
there is no cross-database attachment or guessed import. Loss of either side
keeps protection until the authority session settles. Shared data keeps its
existing machine-scoped ResourceId and host lock, but its durable ownership
belongs to the environment's checkout; the lock serializes ownership requests,
and references to another owner require that owner's verified authority.
The backend's verified ownership labels locate that checkout; a gate
diagnostic path is only a hint and never proof. An absent or unreadable owning
checkout/label is Unknown, not an unclaimed data resource. Fresh allocation
needs exact backend absence while the shared data gate is held. Neither data
nor holder records are added to the machine database.

### 7.3 Path families

| # | Family | Location and lifetime |
| --- | --- | --- |
| 1 | Checkout runtime store | `<checkout>/.effigy/state/kernel.db`; durable state only |
| 2 | Gates | owning checkout's `state/gates/`; truly shared resources alone in machine `state/gates/`; never unlink a held scope |
| 3 | Control sockets | checkout `state/sock/`; removed at durable close; gateway socket remains in `~/.effigy/gateway/` |
| 4 | Immutable objects | checkout `state/objects/`; referenced closures plus 3 superseded per environment |
| 5 | Staging | checkout `state/staging/`; only the owning durable settle removes it |
| 6 | Run logs | checkout `state/runs/`; 1 MiB segments, newest 4 per stream; 2 GiB per checkout, oldest settled first |
| 7 | Leaf certificates | `~/.effigy/gateway/certs/`; gateway `atomic_replace`, reissued on expiry or CA change |
| 8 | Preserved CA | mkcert `CAROOT`; external, read-only to Effigy, never touched |
| 9 | Pinned sources | `<checkout>/.effigy/sources.lock.json`, `.effigy/cache/sources/`; config gate and `atomic_replace`, while pinned |
| 10 | Private vault | `<checkout>/.effigy/secrets/local.vault` and `.prev`; secrets gate and pointer commit, previous generation kept; masked from guest mounts |
| 11 | Key wraps | secrets-owned location; same pointer commit/lifetime as vault; masked from guest mounts when beside vault |
| 12 | Machine preferences | `~/.effigy/config.toml`; user-owned, `atomic_replace` |
| 13 | Other derived caches | per owner; RefreshDerived only, corrupt is a miss, owner-defined retention |
| 14 | Reports and exports | explicit user-owned paths, engine `atomic_replace` |
| 15 | Platform setup outputs | resolver, launchd, completion; setup verifies ownership before changing (R3), retained until explicit removal |
| 16 | Machine facts | `~/.effigy/state/machine.db`; §7.5 only |

Families 1–6 are host-only; family 4 may be mounted read-only by the
backend. Certificate, source, vault, cache, report and platform families do not
move into either database. Observe creates none of these paths. Certificate
and platform setup remain explicit gateway/setup work, not foreground admission.

### 7.4 Retention

Checkout journals keep every unresolved run and the newest 20 settled runs per
owner; old settled logs/events/requests expire together. All referenced objects,
live data claims, environment records and holders stay. Garbage collection is
local, explicit or a bounded Mutate batch: at most 20 settled runs or 64 object
candidates per invocation, with a 50 ms transaction budget; stop at either bound
and continue next time. No global close-time scan and no checkpoint-wide VACUUM
on settle. No timestamp refresh, history or retention scan in the machine store
on task close. Foreground completion leaves no history to retain.

Scope removal still needs its scope gate exclusively, with no guards alive.
Externally deleted checkouts are reported as missing (§7.1); discovery never
creates tombstones or cleans their external resources. Machine endpoint and
port records expire only through verified release, never because a
checkout disappeared. Machine recovery records are compacted only after their
effects are positively resolved, independently of checkout retention.

### 7.5 Machine store: allowlist and write budget

Machine-scoped resource gates use the single machine `state/gates/machine/`
namespace even when their owning run journals in a checkout. Ordinary checkout
gates never consult that domain. First machine-gate use retains its home and
state-directory inodes; observation and recovery helpers stay in the same domain
and refuse a directory that is no longer that inode (a renumbered device is not
a replacement). Machine-format or migration refusal
blocks that machine resource, while local checkout gates remain usable. These
lock files contain no checkout registry or shared data-claim database. Internal
database and object mutexes use their owning checkout's filesystem scope.

The allowlist is closed: gateway daemon identity/control and endpoint/route
claims (including manual routes and loopback addresses), and host admission
state
(opaque live grants/waiters if an admission provider uses this store). Schema,
writer build and import receipts support those facts. Path hints on a claim are
ownership evidence, not an enumeration of checkouts. No checkout catalogue,
heartbeat, task history, task journal, environment render, holder, data claim,
log or global object registry is permitted here. Gateway control includes one
current generation's positively closed outcome and a bounded redacted diagnostic;
starting a successor invalidates the previous generation. This single current
control fact is replaced on closure and is never an append-only history.
The kernel enforces this boundary at admission and typed resource writes:
machine invocations require machine identity and shared-fact owners, checkout
catalog writes and data claims refuse atomically, and immutable object
publication refuses before an intent or payload directory is created. Both
workload and supervisor output capture use memory on every machine lifecycle
path, including recovery and setup.

Gateway lifecycle/publication still needs durable intent. The machine store
may keep one current operation and unresolved effect receipts per shared owner,
with generation fences and the full §5.6 recovery semantics. It retains no
completed run history; repeated idempotent ensure/read performs no write.
An unresolved operation blocks that shared owner, not checkout-only work. The
daemon writes on lifecycle/configuration change, never per request, access log,
proxy connection, readiness poll or idle tick. Certificates stay separate files.

Budgets below count successful logical database write transactions, including
meta changes, not SQLite's physical page or fsync count. The implementation
proof also reports physical writes/bytes. Initialization/import and explicit
maintenance are measured separately, never hidden in the steady-state counts.

| Operation | Machine write transactions (maximum) |
| --- | --- |
| Short foreground begin/spawn/settle; ordinary durable checkout begin/spawn/settle | **0** |
| Status, runs, doctor, unchanged gateway ensure or existing port pin lookup | **0** |
| Pure endpoint/route edit, all conflicts as one set | **1**: payload and claim revisions commit together |
| Gateway lifecycle or endpoint change with an external bind/reload effect | **3**: intent/reservation; applied receipt plus publication; positive close/settle |
| Admission grant/enqueue, transfer/admit or release, if stored by the provider | **1 per state transition**, no polling/heartbeat writes; Effigy itself creates no admission row for a foreground run |
| Fault recovery | one transaction per unresolved effect disposition, plus one close; no scan or rewrite of completed runs |

Contract 049's external hook remains provider-owned; its own storage is not an
Effigy run journal and is measured separately. No-hook runs touch no admission
state. A provider failure cannot make light, unhooked foreground work open the
machine store.

There is no atomic transaction across checkout and machine databases. Before a
checkout asks for a shared change it commits an effect with exact target,
`(CheckoutId, ResourceId, Generation, RunId)` and an idempotency token. The
machine authority reserves/checks the full conflict set under its gate and
commits the receipt before returning; the checkout then records that receipt.
A lost reply is retried with the same token, not another allocation. Machine
pending claims protect until positive settlement; checkout settle requests a
fenced release, never edits machine rows. Activation follows verified shared
publication, and a failed/unknown cross-store step stays partial with protection.
I-A3 atomic reservation applies to each authority's complete conflict set; a
workflow spanning data and endpoints orders these intents, compensates on
failure and cannot publish Active until both receipts exist. Machine admission
never waits on a checkout gate or probes under a transaction (K14).

### 7.6 Discovery without a central index

`runs` defaults to the current checkout. `runs --all` and cross-checkout doctor
are the only commands that look beyond it. They build a read-only candidate set
from cwd, Git's linked-worktree list, configured
search roots (`[runtime] checkout_roots = ["/path/to/workspaces"]` in user
config), and checkout paths already present in gateway/port claims. This is
bounded filesystem discovery, not registration at run start. Search roots are
operator configuration; they do not grant mutation authority. Traversal never
follows directory symlinks, reads arbitrary source files or creates state.
Default limits are depth 8, 1,000 checkout candidates and a 5-second total
view budget, including journal reads and probes; hitting a limit reports partial results and the omitted root/reason.
A missing configured root is reported, an unreadable one is `Unknown`.

Verify candidate root/token identities, deduplicate by checkout identity and
read each local journal/gate directory independently under bounded Observe
contexts. One corrupt, busy, missing or unsupported checkout appears as a
per-checkout diagnostic; it cannot discard other results. Rows name their
checkout and evidence source (durable, live foreground, residue); output also
states roots searched and incomplete coverage. `--all` means all discoverable
checkouts, not an assertion of host completeness. A checkout outside every
search root and shared claim cannot be found after its owner exits. Supply its
root through configuration or run the query there.

`run status`, `run logs`, `run stop`, attach and `--run-id` key resolution
read the journal of the checkout the query runs in, resolved from cwd, and
nothing else: one cheap read per lookup, never a scan of its peers
(effigy#339, Q-011). An unreadable own journal is `Unknown`; it is never
permission to select another checkout. `runs --all` is the only cross-checkout
view. Every action re-verifies its exact target, and a polling wait does at most
one cheap read per interval with a backoff of at least 250 ms.

Doctor additionally reports imported-but-missing roots, legacy-store cutover
state and verified external resource residue; it performs no import, repair or
numeric signalling. Queue admission does not reconstruct capacity from this
view: the contract 049 hook receives cwd/root and holds a live process/pipe
lease, and a nested invocation sees the advisory `EFFIGY_ADMITTED` marker
instead. No global run index or database scan is needed to decide whether that
lease is live.

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
| State roots | owning checkout `<checkout>/.effigy/state/`; machine `~/.effigy/state/` for §7.5 only | none. The guest has no store, no gates and no runs. |
| Gates | all resources | none; `Gate::acquire` returns `HostAuthorityRequired` |
| Store writes | yes | none |
| Runs | yes | none of its own. Each admitted request is a host run (§8.3). |
| Authority | the owning engine | requests over the host authority channel (§8.3) |

The mode boundary is the environment's declared operator workspace, never a
mode or environment name. Prepare and entry match the selected container against
workspace declarations across all systems, including explicitly selected
non-default systems and workspaces; the default binding is not a mode filter.
Declarations sharing a named container must have identical effective workspace
configuration (including mounts, checkout path and identity). Conflicting
declarations refuse by name even when a task supplies a workspace override;
use distinct container names for distinct workspaces. A declared workspace
without a non-empty resolved user and absolute home refuses Prepare and entry
with configuration and host re-prepare guidance; it never becomes hybrid.
Hybrid mode provisions no guest binary, no context,
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
  generation. The document is `context.json`, schema
  `effigy.workspace-context.v1`; a guest reads the pointer and then one
  generation, and an unknown schema or field is unprepared rather than a
  newer version to guess at.
- Refresh: the host publishes a new complete generation on every Prepare and
  whenever a fact it carries changes on the host, then switches `current`.
  It never edits a published generation. A guest invocation reads one
  generation for its whole run.
- Sources closure: the pins name the root manifest, the source lock, every
  bundle and catalog source the host composed from, each composition fragment,
  and the rendered composed manifest. The rendered document is published once
  at `/run/effigy/snapshots/<digest>/manifest.toml`, so a bundle- or
  catalog-provided task is guest-readable without the guest reading a cache the
  host did not approve. A bundle or catalog pin's snapshot is the rendered
  result it contributed.
- The environment fence is the environment record's activation generation,
  which the activation run advances; `container up`, a task's environment apply
  and the workspace handoff all publish through the same Prepare, so the
  recovery a refusal names prepares the workspace rather than repeating it.
- Entry fences: the host verifies the backend's full container id and targets
  that exact id for context reads, image/boot probes and publication. The
  context's container id must match in full; a hostname, cgroup claim or id
  prefix never proves placement. VM boot id, image platform/libc and environment
  generation must also match. A replaced container, a reboot that retained the
  filesystem, or a re-activation with unchanged files refuses and names the
  host re-prepare. No additional state ledger is needed for this binding.
- Local reads: version, the pinned task list and the pinned source identities
  come from the context, the immutable rendered snapshot and the mounted
  checkout alone. The pins whose snapshot is checkout-visible are
  digest-checked before anything is parsed, so a checkout that changed since
  Prepare refuses instead of reading bytes the host never approved. A local
  reader holds no capability, so a Static or Observe read cannot enrol, and an
  authority-bearing command refuses rather than enrol.
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
- Mount isolation validates the complete effective mount set the primary
  service will have: injected and retained entries, short and long form, with
  absolute guest targets checked before comparison. Parent (`..`) traversal
  refuses; redundant separators and current-directory (`.`) components normalize
  before private-store overlap, mask ownership and duplicate-target checks.
  This also applies to checkout destinations and every Compose overlay.
  Compose interpolation is expanded and a bind-backed named volume is resolved
  through its local driver options. External volumes, explicit volume-name
  aliases, unknown volume names and drivers whose host source cannot be
  established are refused without a backend source inspection receipt.
  Exact aliases from the host catalog's owned-volume inventory remain usable;
  their driver and external options still undergo verification.
  Effigy-generated disposable volumes remain owned by the workspace; a source
  declaration that collides with one still undergoes the same checks. Volume
  mappings merge recursively across inputs, preserving inherited external flags
  and driver options. Mount interpolation accepts `.env` literal assignments,
  including simple single/double quotes and Compose inline comments. Dotenv
  expansion, escapes, multiline or alternate assignments, duplicate names,
  environment-file selection overrides, tilde paths and nested Compose interpolation are
  refused when their meaning cannot be proven; they are never partially read
  as a safe host path. Every Compose input file Compose is handed is
  rewritten and validated, not only the first: an overlay, later or
  intermediate, replaces an entry at the same guest target, so a state root
  bound at another target in any overlay is refused and an overlay entry at a
  masked target is stripped. The
  workspace mounts are injected into the first file that defines the primary
  service and the masks are appended to the last one, so they are applied after
  every source file and own their guest paths. A host state root, an ancestor of
  one mounted anywhere but the masked checkout mount, a second source for one
  guest path, an entry whose source cannot be proven, and anything overlapping
  the guest's `/run/effigy` store are refused by name.

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
| Colima (macOS; VZ or QEMU; docker or containerd) | one Lima VM per profile | `flock` never crosses virtiofs, sshfs or 9p, so gates are host-only; the state root is never mounted into containers; checkouts sit under the home mount | full container id, VM boot id from the guest's `/proc`, pid namespace, session, leader start ticks | `colima restart` changes the VM boot id, proving prior sessions `Absent`. Explicit profile repair checks exact pidfile, process arguments and start identity before signalling; it never uses process patterns. |
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
| App tasks with `run_in` `host`, `container` or `either`; shell commands; task references, sequences and setup steps; Rhai tasks and host APIs; `test` and suites; drafts; QA groups and members; heavy tasks; `skill run` | channel | Admission request. The host resolves placement, gates `Task(TaskId)` (K5 excludes host and guest runs alike), journals and runs: container targets in the target container through remote custody, host targets on the host. Rhai effects are classified on the host (§5.3). |
| `defer` | local inside an admitted workload; otherwise channel | Inside a workload it is that run's command, under its custody. From a shell it is an admission request targeting the current container. |
| Workspace-only tasks, `workspace`, `system` | channel | Attach to the live workspace session run (§8.7); never a second shell or session. Lifecycle operations are requests. |
| Managed TUI and headless (`dev`, profiles) | channel | Admission of a `Detached` managed run, child of the session run. The guest renders events; the host supervisor owns workloads and attachments. |
| Managed `status`, `logs`, `stop`; `tasks status` | channel | Observe requests for status and logs; `stop` is a generation-fenced request (K7). Host and guest pids are never mixed. |
| `tasks cache`, task-lock `unlock` | channel; `unlock` refused | Cache reads are host Observe requests; clears are RefreshDerived requests to the owning store engine. `unlock` refuses in both placements: there is no unlock command (I-G7). QA groups are in the admission row. |
| `doctor` | local and channel | Manifest and tool checks local; runtime checks (backend, gateway, routes, ownership, volumes) are host Observe requests. |
| `container shell`, bounded shell, `exec`, aliases | channel | Admission request; the host launches the command in the named service, including the current container, through remote custody. |
| `container up`, `down`, `reset`, `retire` | channel | Mutate requests to the environment engine under R4. A guest `down` of its own environment follows §8.7. |
| `container status`, `stats`, `logs` | channel | Host Observe requests on the backend inventory. |
| `container data`, `cache`, `volume`, `profile`, `migrate`, `hosts`, `scope`, `ports` | local and channel | Declared configuration is local from the context; host facts are Observe requests; mutations, including `eject` source writes, are requests. |
| `secrets` list, doctor, get, export | channel | Observe requests to the secrets engine; plaintext follows the host CLI's policy. |
| `secrets` init, set, unset, import, unlock, lock, passphrase change; required task secrets | channel | Mutate requests to the secrets engine; task secrets arrive as per-run grants (§8.2). |
| `deps` status, link, unlink, pin, unpin | channel | Status is Observe; changes are requests to the deps engine. |
| `graph` index; `bundle` sync (bundle and catalog source pins); `config` user settings | channel | RefreshDerived, Prepare or Mutate requests to the owning store engine. The guest has no separate preference file. |
| `state` apply, capture; `artifact` stage, capture, push; `init`; `docs` index writes; `release`, `deploy`, `bootstrap`, `uninstall` | channel | Requests under the same authorisation as the host CLI. |
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
- The artifact family is the image's platform **and** its C library, never a
  host `uname` and never a default: a glibc image takes the `*-gnu` artifact and
  a musl image the `*-musl` one. The guest reports the loader it can see, and an
  image whose C library the host cannot prove refuses Prepare rather than
  guessing. An image with no artifact for its own family refuses with the host
  re-prepare command and the missing target named.
- The store is `/run/effigy/bin/<digest>/effigy`, one immutable file per
  digest, selected by the digest in `/run/effigy/bin/current`. An artifact is
  authenticated before it is published: its producer receipt names the target
  and the digest its bytes must have, a release download also matches the
  checksum published beside the artifact, and the installed executable is
  re-hashed in the container. A release that publishes no checksum cannot be
  installed, and an image without a SHA-256 utility refuses Prepare. A local
  producer builds with the requested Rust `--target` and copies from that
  target's release directory; unavailable target libraries or linkers fail
  before publication. Each artifact has its own `<binary>.receipt` containing
  its target and digest, so producing another target cannot replace its
  evidence. An aggregate rehearsal receipt is not artifact evidence. The guest
  authenticates installed executables with bounded digest probes, never by
  reading the file into a bounded command's output. The stable
  entry path `/usr/local/bin/effigy` is an atomically replaced symlink into the
  store, so a running invocation keeps the bytes it resolved and only a new
  invocation sees a new generation. Before handoff, entry verification requires
  that symlink to point directly to the pinned immutable executable and hashes
  the bytes reached through the stable entry; missing or retargeted entries
  refuse with host re-prepare guidance.
- Workspace Prepare consumes the produced artifact on the host, verifies its
  digest, and installs it into a guest-private executable store, selected per
  workspace generation by an atomic pointer. The architecture comes from the
  container image's platform, not the host's `uname`. An image whose libc
  has no matching artifact refuses Prepare.
- No consumer-time build: Prepare consumes bytes a release download or an
  explicit `effigy workspace:linux:artifact` already produced, and otherwise
  refuses naming that command. It never discovers a checkout to build from and
  never writes a host source pointer at consumer runtime.
- No run-time build or download: task, exec and terminal entry never fetch
  or compile Effigy or look for a sibling checkout. A contributor's explicit
  Prepare may build the artifact and record the same digest receipt.
- A version or protocol mismatch between client, context and server refuses
  before any effect, naming the host re-prepare command. Guest artifact
  installation never replaces the live executable under an active run.

### 8.10 Rules under this design

| Rule | Holds because |
| --- | --- |
| R1 Host authority | The guest has no state root, gates or runs. Every effect is a host run under the owning engine's gate. Capabilities are host-kept grants; a request carries no authority beyond its capability, and the host re-resolves it. The state root and vault are never in the guest's mounts. |
| R2 Pure reads | Local Static and Observe read an immutable context and the mounted checkout and write nothing. Host Observe requests create no run, token or log entry. `--plan` is Static and admits nothing. Context publication and the enrolment credential are Prepare effects; capability issue and enrolment are Mutate requests to the session run. |
| R3 Unknown grants nothing | Disconnect, stale generation or an unreachable host is `Unknown` for the client and refuses Mutate with no local fallback. The host acts only on its own `Verified` observations and fences every request by generation (K7). |

### 8.11 Workspace-client interface v1

This is the implementation seam for §8, not a claim that the client or any
transport is implemented or qualified. The channel is separate from the
kernel's remote custody protocol: the former requests host authority; the
latter proves and controls a workload already admitted by the host.

**Prepare publication.** Container-mode activation uses the actual environment
owner run and `Gate(Env)`. A typed kernel-owned additive publication obligation
commits before guest staging, immutable file writes or pointer switches. Its
receipt names the activation fence, full container identity, VM boot identity,
artifact digest and context generation. Only after that receipt is applied and
the required obligation is completed may activation publish active and attach
requesting holders. Container up, task activation and workspace handoff use
this same boundary; publication never follows settlement as a separate effect.

Existing holders remain attached to their original selections. This narrow
operation permits only additive immutable binary, snapshot and context objects
and atomic pointers for future invocations; it grants no stop, reset, runtime
replacement or generic environment-effect permission. Those transitions still
require `TransitionPermit` and its holder checks. Guest identity is re-probed
before publishing the context. An existing object with wrong bytes is refused;
recovery skips retained orphan context documents instead of overwriting them.
Partial publication or a missing receipt leaves activation unsuccessful and
pending. Recovery retains additive bytes without replaying container I/O; a
fresh activation verifies or publishes a complete generation under the gate.
No database transaction spans guest I/O. Hybrid activation emits no guest
publication obligation or artifacts.

**Wire.** TLS pins the server identity from the selected context before the
client sends a credential. A length-prefixed UTF-8 JSON frame has a four-byte
big-endian length, at most 1 MiB; malformed, oversized, duplicate-key or
unknown-version frames close the connection before dispatch. Binary stream
payloads are base64 in frames, at most 32 KiB decoded. Limits apply before
allocation and decoding. Each connection begins with `hello` containing
`schema: "effigy.workspace-channel.v1"`, supported protocol versions,
client Effigy version and binary digest, context generation and session run
id. The server returns `hello_ok` with the selected version and its Effigy
version, or a refusal. V1 selects only protocol 1, and requires the client version and digest to match the context and the
host-pinned guest artifact, and the client, context and host version and
protocol to match §8.9;
there is no downgrade. Authentication follows negotiation inside TLS: an
`authenticate` frame presents the capability, or an `enrol` frame presents
the enrolment credential. Neither credential is echoed or logged. No
request is accepted before authentication; enrolment only issues a
capability and cannot execute another operation. Version, identity or
credential failures refuse before effects and tell the client to re-prepare
where applicable.

An authenticated `request` frame has `id` (128-bit random hex), `method`,
`context_generation`, `environment_generation`, `session_run_id`,
`container_id`, `vm_boot_id`, and `body`. The host binds these to its grant,
not to the client's claims. An invocation body contains a selector or typed
operation, checkout-relative cwd, intended target, argument array and output
mode; it contains no resolved plan, executable path, entry point or state
root. At most 256 arguments of at most 4096 bytes each are allowed, with a
64 KiB total body; cwd and target are bounded to 4096 bytes and must resolve
within the allowed checkout or target. Invalid encoding, traversal, unknown
fields or methods and excess limits refuse before admission. Request IDs are
unique per session generation, including across reconnects.

A `response` carries `id` and exactly one of `observed`, `accepted`,
`completed`, or `refused`. `observed` contains the host's typed Observe
result; `accepted` contains the host `RunId` and placement; `completed`
contains the typed outcome and exit status; `refused` contains a stable code
and safe diagnostic. Codes include `invalid_request`, `unauthenticated`,
`forbidden`, `stale_context`, `identity_mismatch`, `version_mismatch`,
`held_by`, `unavailable`, `unknown` and `id_conflict`. Preserve the host
CLI's result and error types: `--json` is rendered as the existing
`effigy.command.v1` envelope, not as a channel frame. Channel control frames
and credentials never enter stdout or that envelope.

Mutating or admission requests are recorded by the host before admission,
keyed by session generation and request ID with a canonical request digest.
A concurrent or later retry with the same digest returns the recorded
admission, run or outcome; a different digest returns `id_conflict`. The
record survives a disconnect and supervisor restart and is retained for the
session generation and its recoverable outcomes; it is never silently
forgotten and readmitted. If recovery cannot establish the record or outcome,
return `unknown`, not a new run. Observe requests have no admission record or
side effects; retries re-observe and may return newer facts. Neither retry
rule grants authority after capability revocation or generation change.

A `stream` frame names `id`, `stream_id`, monotonically increasing `seq`,
`kind` and payload. `id` is the attach request that opened the connection's
attachment, `stream_id` 0 is that attachment's terminal, and `seq` is one
strictly increasing series per stream id shared by every kind on it — retained
output's series is the absolute log position of each frame, and a source that
keeps no position numbers frames instead. Kinds are `event`, `stdout`,
`stderr`, `terminal_output`, `terminal_input`, `resize`, `ack`, `end` and
`gap`; events carry typed host
status or outcome, resize carries positive rows and columns, and `end`
carries the final sequence and outcome where known. Per stream, the receiver
acknowledges consumed sequence numbers; the sender permits at most 16
unacknowledged data frames and at most 512 KiB of queued decoded data, then
backpressures. Control and cancellation must remain serviceable under backpressure.
A reconnect may request a sequence cursor; retained frames resume in order,
otherwise `gap` reports the missing range and the client obtains current
status or logs from the host. A gap is never presented as complete output.
Terminal input is neither replayed nor buffered across reconnects; resize is
latest-wins. Frames for an unknown stream, out-of-order input, invalid cursor
or exhausted limits are refused or closed without dispatching effects.

**Methods and custody.** `observe` is host `ObserveContext` only; it makes no
run, token, journal or access log. `invoke` admits the host-resolved command;
its actual effect class is determined by host policy, never by the method
label or a guest claim. `attach` observes an authorised session or run and
opens a bounded stream; `cancel` requests stop of an authorised run;
`detach` explicitly releases the owning client without stopping a Detached
run. Enrolment, invocation, cancellation and detach are authorised operations;
the host checks the grant's operation and target scope and the same operator
authorisation and elevation as the host CLI. A client cannot cancel or detach
another client's run merely by naming its ID. A secondary attachment cannot
stop the workspace session by disconnecting. EOF without detach cancels an
Inline owner or stops its owned Detached child; EOF after acknowledged detach
leaves that child running. Disconnect, capability loss, generation mismatch
or host probe timeout yields `Unknown` for unfinished work and refuses further
Mutate; reconnect queries the recorded ID. Host-side cancellation still
settles through the run's custody, and an unproven stop remains Unknown.

The grant is memory-only in the session supervisor and binds host user,
checkout, session run and generation, environment generation, full container
identity and VM boot id, and allowed operations and targets. TLS pinning and
this binding are both mandatory. The enrolment credential is a separate,
versioned, environment-generation and container-bound credential for enrolment
only; it is 0600 in guest-private storage and is revoked on generation
change. A first Effigy-opened terminal receives its capability as part of
entry; direct terminals enrol only for authority-bearing operations. Static
and local Observe do not enrol. All workspace-user processes share this trust
boundary. Capability issuance is a session Mutate; authentication with an
existing capability is read-only.

For every request the host verifies the current context, environment and
container fences and re-resolves selector, cwd, target, placement, privileges
and effect class from host records into `ResolvedInvocation`. It acquires
the canonical engine gates and journals effects before execution (§5 and §9).
Origin does not change resource identity. The guest has no state root,
store writer or gate: guest placement rejects `Gate::acquire` with
`HostAuthorityRequired`, and dispatch cannot construct host effect contexts
locally. State and vault mounts remain masked. No failed channel request falls
back to guest execution. Hybrid provisions no guest binary, context, listener
or enrolment credential.

The kernel seam is `run::{reserve, lookup, Run::begin_admitted,
resume_owner, attach_secondary}`. A `Session` must identify an admitted,
nonclosed session run at its current resource generation. The reservation
records that session's owner, and admission, replay and attachment operations
recheck it. A legacy reservation with no recorded session owner is refused;
it never gains one by inference. `lookup(&ObserveContext, &StateRoot,
AdmissionKey, Digest)` reads a fresh journal snapshot. An unfinished admission
is returned only while its current owner process identity and held owner gate
can be established. An orphan is `Unknown` and cannot acquire handles or be
readmitted; ordinary custody recovery must settle it before its recorded
outcome becomes available. The keyed run and consumed ticket commit before
predecessor cleanup, so interruption during cleanup retains that binding.
Client release revalidates the current session, owner and exact attachment in
its write transaction. A registered liveness watch instead releases through
the live Run's owner-gated custody, so session expiry cannot retain a lost
terminal. Owner EOF releases its row and queues one matching stop in that same
transaction; persistent foreign or resolved request-ID collisions select another
identity without deleting the existing row. Control and release deadlines also
bound schema initialization and additive migration.

During keyed predecessor recovery, the current owner gate and successor fence
keep the predecessor's already-fenced escalation serviceable. New controls use
the admitted successor's current RunId and fence; escalation there also shortens
predecessor cleanup. Stale public requests remain refused. Reading either
mailbox checks its exact serialized run and generation binding, and no journal
transaction spans custody I/O.

**Owners and coverage.** Paths below name the intended owners, including new
modules; they do not imply those modules already exist. The only integration
file allowed to join these lanes is `src/runner/workspace_client.rs`.

| Interface or command class from §8.6 | Owner and boundary |
| --- | --- |
| Wire types, framing, authentication, replay and host dispatcher | New `crates/effigy-kernel/src/workspace_channel.rs`; delegates resolution and effects to owning host engines, never to remote custody. |
| Workspace session, attachments, terminal control and down coordination | New `crates/effigy-kernel/src/workspace_session.rs`; runner entry adapts `src/runner/workspace_command/session.rs`. Existing `crates/effigy-kernel/src/remote/{session,stream}.rs` remain remote workload custody. |
| Context, artifact and enrolment Prepare; placement detection and guest local reads | `src/runner/workspace_command/provisioning.rs` and new `src/runner/workspace_client/context.rs`; `crates/effigy-containers/src/workspace.rs` owns mount isolation. |
| Help, version, plans, config and static task, bundle, service, graph, docs, scan, contracts, changelog, skill, state and artifact reads | Guest local reader in `src/runner/workspace_client/context.rs`, using the pinned snapshot and mounted checkout, with no refresh. |
| Tasks, shell, references, Rhai, tests, drafts, QA, skill run, defer outside a workload, managed runs and workspace/system entry | Host task engine and session via dispatcher; defer inside a workload stays under that workload's custody. |
| Managed status, logs, stop; task status and cache; doctor | Host task/run and store engines for live facts and mutations; local manifest/tool checks stay local. Unlock is refused. |
| Container shell/exec, lifecycle, status, stats, logs, data, cache, volume, profile, migrate, hosts, scope and ports | `crates/effigy-containers/src/environment.rs` and its owning operations; static declarations local, live reads Observe, changes host requests. |
| Secrets, deps, graph index, bundle sync, user config | Their host store engines; read or mutate as §8.6 specifies. Secret grants belong to the admitted run. |
| State apply/capture, artifact stage/capture/push, init, docs index writes, release, deploy, bootstrap, uninstall and gateway | Respective host engines under their canonical policy and gates; gateway never runs in the guest. |
| Hidden internal entry points | Refused at guest CLI and dispatcher; never a channel method. |

Session activation is one Detached run owned by the environment workspace
entry's `Task(TaskId)` and holds a Borrow attachment. Additional terminals
attach, not create a second session. Down from within follows §8.7's holder
check, child settlement, detached down operation and final gate recheck; the
session does not cascade-settle other runs. This boundary requires the
supervisor to expose replay-safe admission and attachment handles to the
dispatcher; if that cannot be provided without a second authority, stop the
integration and resolve the kernel interface before implementing a substitute.

#### Implemented: the session layer (effigy#438)

The kernel records this boundary needs are implemented and proven (the kernel
seam below, and lane A's channel). `crates/effigy-kernel/src/workspace_session.rs`
is the lane B session layer on top of them and owns no second authority:

- `SessionRef` resolves the environment's one session through its own additive
  record (`workspace_sessions`), not by guessing from an attachment: a child run
  or a host `dev` hold on a persistent environment can borrow the same
  environment, so a `Borrow` row names a holder, never the session. Every read
  validates the record against the journal — the run is at the recorded
  generation and not closed, it still borrows the environment at the recorded
  generation, and the environment has not advanced — and an invalid record
  resolves as absent, so a stale session is never mistaken for a live one and a
  successor can be admitted. `SessionRef::admit` refuses any placement other than
  `Detached`, holds the environment `Borrow` attachment for the run's life and
  records the mapping, refusing to replace a live one. The entry itself is guarded
  by the workspace entry's owner gate, and request ids inside the session are the
  kernel seam's `run::reserve` records — one admission authority, not two.
- `SessionHandle` hands the kernel seam's `run::Session` to reservation, attach
  and bounded control, and exposes the down coordination. `OwnerLoss` maps EOF,
  a disconnect and TERM/HUP to the kernel's owner release (`Release::Eof`) and to
  the stop cause the supervisor settles with (`StopCause::OwnerLost` or
  `Signal(n)`); an acknowledged detach is `Release::Detach` and stops nothing.
- Down: `DownCoordination::begin` refuses every holder other than the session and
  its own child runs, live or `Unknown`; `DownChildren` stops only those children
  with an ordinary journalled `run::request` (idempotent by request id) and takes
  its permit through `Attachments::permit_exempt` once the session has entered
  its settle.
- The runner entry `src/runner/workspace_command/session.rs` maps each front end
  to its `TerminalKind` and prints the one resolved identity.

Still owned elsewhere by the lane split: the supervisor's serving loop over its
control listener, the channel frames and dispatcher (lane A), Prepare and context
(lane C) and the client glue (`src/runner/workspace_client.rs`). Proofs are the
fake-owner, fake-runtime cases under `workspace_session::tests::`; the admission,
attachment and stream records they consume are proven by lane A's
`test:kernel:workspace-channel`. Neither exercises an engine, container, gateway
or live supervisor.

**Kernel seam.** `crates/effigy-kernel/src/run/workspace.rs` implements that half.
A session is its run and its generation, so two sessions at one generation stay
distinct. `reserve` records `(session, request ID, canonical request digest)`
with the one run it reserved, and `Run::begin_admitted` commits that binding, the
run row and the owning client's attachment in one transaction. One run identity
belongs to exactly one key: a second key proposing the same run id is refused in
that transaction and by a unique index, and an ordinary run may not take a
reserved id, so a reservation is never swapped for another workload. A key with
a record is classified before any proposed id is judged, so a same-key retry
recovers its admission or reports its digest conflict whatever entropy it
offers. Both replay outcomes first validate the recorded session owner and
current generation; a conflicting digest never bypasses that fence. Only the
binding that actually admitted a run answers for attachment and
records its outcome; a reservation that never committed keeps answering
`Unknown`. `lookup` answers a reconnect from the same records, `Unknown`
included, and never admits again. A
reservation whose admission cannot be established is `Unknown` and names no run:
an id the client offered is not evidence that the request produced it. The run's
close transaction records the keyed request's outcome and ends its client
attachments in the same commit that closes it. The ticket is transferable — a
dispatcher hands it to a detached engine supervisor through
`supervise::EngineLaunch::admission`, and the engine reads it as
`EngineSession::admission` — but admission re-checks the durable reservation, so
the ticket alone binds nothing. `attach_secondary` gives a second terminal its
own attachment row, a bounded stream cursor that reports a rotation gap, and
stop/escalate control bounded at four waiting requests, counted in the same
transaction that records one; every read re-checks the attachment row, and a
control re-checks it in the transaction that records the request, so a release
cannot be overtaken and a released attachment or a closed run stops both. An
owning client's EOF records its run's stop in the same commit as the release,
while an acknowledged detach records none and leaves the run going, including
when its original owning pipe closes. `EngineSession::watch_attachment` binds an
attachment to a separate client liveness pipe; the supervisor's control turn
releases a killed secondary's row without stopping the owner, and reclaims its
slot. If the attaching caller loses the creation response or dies before watch
registration, `resume_secondary` reopens the exact still-attached secondary ID
under the current recorded session and live workload fence. It allocates no
slot and revives no released row; the authorised dispatcher can register the
recovered handle or release it to reclaim the slot without stopping the owner.
Watches are bounded to eight secondaries and one owner. Reservation replay,
attachment opening and attachment status derive finite observation phases even
when the engine's workload lifetime is open, preserving any shorter caller
deadline. Workspace controls respect the caller's remaining budget, capped at
one second, including a busy store; a timeout records no control and the caller may retry. Output reads do
not hold the writer or share the liveness pipe. `resume_owner` reopens
the owning client's handle and no other. Both refuse a run this session did not
admit and grant nothing: the dispatcher's grant check remains the authority.
`test:kernel:workspace` owns these proofs.

**Lane A dispatcher.** `crates/effigy-kernel/src/workspace_channel.rs` owns the
v1 wire, its framing and limits, the server pin check, negotiation, grants,
enrolment and the typed entry points. It reads the key with `run::lookup`
before it calls `run::reserve`, so a retry spends no entropy and reaches no
engine, and an unreadable record is answered `unknown` rather than admitted
again. Resolution, admission and observation stay with the owning host engines
through four small seams — `Resolver`, `Admission`, `Observer` and `RunIds` —
and `Admission` receives the reservation's `AdmissionTicket`, which the engine
hands to `EngineLaunch::admission`, so the run an engine admits is the run the
record names. `observe` reserves nothing and attaches nothing; a capability is
memory-only, checked before every effect, and its operation and target scope is
the host's, never the client's. Every request is checked against the grant's
server-side binding as well as its own claims, and enrolment issues a capability
for the session the host recorded, never the one an `enrol` frame names.
Revoking a generation ends the enrolment credential with the capabilities it
issued, so re-enrolment cannot mint a live capability for a dead generation.
A connection owns at most one attachment: attaching to another run refuses,
replacing this connection's own secondary releases the replaced row, and only
the connection that admitted a run holds its owner handle. A connection that
ends on a refused or malformed frame releases its own secondary row with it, so
a rejected frame can neither exhaust the run's secondary slots nor orphan one,
while an owning client's lifetime is still released only by its own end of
stream: malformed traffic records no owner stop. Captured output is
published through a pump bounded by the acknowledgement window, so a control
request is answered while data is backpressured; the connection's lifetime is
its session's, and a closed connection whose frames were malformed releases no
owner lifetime. Each connection starts and ends with no queued stream state, so
a server that serves a reconnect publishes nothing from the previous connection
and never carries an unpublished chunk or a full window into it.

Stream sequences are numbered once per stream id and shared by every kind on
it, so a stream's stdout and stderr form `0, 1` rather than restarting each
other, and the acknowledgement window frees frames in that one series. A source
that reads a retained log publishes each chunk with its absolute position, and
the sequence is then that position: one frame's sequence is its first byte's
offset, so a cursor, a frame sequence and the retained log are one coordinate
system across reconnects and chunk boundaries with no replay store or second
cursor. `RetainedStreams` is the kernel's own
endpoint over `run::read_log`. Retained streams have independent logs and
independent lengths, so an attach reports the position it consumed on each
stream id it read (`cursors`), with `cursor` as the default for the primary
stream; one position for the whole connection would skip the bytes a client
never read on a shorter stream or replay the bytes it had consumed on a longer
one. Negotiation is per stream and pure: `resume(run, cursors)` answers the
start position of **every** stream the endpoint publishes, and refuses the
attach — opening none — when a stream the client did not name individually
would have to be guessed, when a cursor names a stream the endpoint does not
carry, when a position is past that stream's retained end, or when a position
cannot be established because no retained log exists (a nonzero cursor cannot be
resumed after retention removed the bytes). A stream's position at or before its
retained end continues from there; bytes the log rotated past become one `gap`
frame naming the first missing offset, ahead of the data that resumes there — and
that gap is published even when the surviving segment is empty and no data
follows it, so a run that ends there does not leave the client believing it saw
everything. Every emitted frame on a stream keeps a strictly increasing sequence
and no gap is presented as complete output. Nothing moves until
`start(attachment, positions)` runs for the attachment that actually opened, so a
replacement the host then refuses leaves the connection's working attachment
exactly where it was. The endpoint re-checks the connection's attachment with
`Attachment::attached_observed` on every read, so a released or replaced row
stops publication. `input` delivers one validated terminal input frame exactly
once to the terminal owner — `Terminal::input` — and `resize` hands over the
newest size with no queue behind it. Delivery is fenced on the attachment's own
lifetime twice: the channel re-reads the row immediately before the call, and
`Terminal` receives the live `Attachment` rather than its identifier, so the
owner's final check before writing is `Attachment::attached_observed` and a row
released, replaced or ended by its run's closure in between writes nothing. An
inbound frame is accepted only for the connection's own attach request id, its
own terminal stream and the stream's next sequence, and only as
`terminal_input`, `resize` or `ack`; an acknowledgement must also be shape-only
and name a data frame this connection published and has not freed. An unknown
stream, a foreign request, a repeated, out-of-order, duplicate, stale or
resumed-unsent sequence, a malformed payload or an outbound kind refuses and
closes with no host effect — no input reaches a terminal, no size is recorded
and no frame is freed. A replacement attachment is rebound in one step: the
previous attachment's window, held chunk, numbering and inbound sequence are
dropped before the requested cursor positions its own stream, so a replacement
resumes at its own cursor and its acknowledgements cannot free the previous
attachment's frames. The acknowledgement window is shared by every stream on a
connection while each frame stays its own stream's: an acknowledgement releases
that stream's frames that sit between other streams' frames, so an interleaved
stream's consumption is never blocked by another stream's order. A host with no
terminal owner refuses input and resize (`NoTerminal`) instead of reporting a
delivery it did not make. The kernel has no per-run terminal input/resize
writer, and no host front end owns a session terminal yet (the transport, the
guest client and the runner integration are separate tasks), so channel-owned
delivery stops at the `Terminal` contract: the production runner terminal writer
and the serving-loop binding are a separately owned follow-up, and nothing here
claims production terminal delivery.
`test:kernel:workspace-channel` owns these proofs.

## 9. Authority map

| Resource kind | Authority | Gate | Run placement | Journals | Settle undoes |
| --- | --- | --- | --- | --- | --- |
| Task | task engine | `Task(TaskId)` | inline | env schema, setup, steps, process groups | stop groups; `Cancelled` or `Interrupted` |
| Task with profile (managed) | run supervisor | `Task(TaskId with profile)` | detached | setup, attachments, gateway ensure, workloads (local, remote) | workloads (remote first), then release attachments |
| Env | environment reconciler | `Env` | detached for sidecars; inline for `up` and `down` | applying stages, claims, sidecar workloads | per environment policy, under a permit |
| Data | environment reconciler | through `Env` claims | — | claim reserve, commit, release | release claim |
| Endpoint | gateway engine | through claims; `Gateway` for lifecycle | inline; daemon | reserve, commit revision, applied acknowledgement | release |
| Gateway | gateway engine | `Gateway(instance)` | daemon lifecycle run | bind, publish | stop |
| Store (vault, bundles, graph, deps) | owning lane | `Store(name)` | inline | replace commits | restore previous generation where declared |
| Delivery (release, state, deploy) | owning lane | `Task` or `Store` | inline | checkpoints | `Irreversible` reported; reconcile, never replay |

## 11. Legacy state

v0.15 has no importer for any legacy family (Q-012 §3, Tom 2026-10-26). A
v0.14.1 user updates and everything keeps working with no manual step and no
command: the runtime is the source of truth and one apply converges from it.

| # | Principle |
| --- | --- |
| M3 | Effigy never modifies or deletes a legacy file, and never writes one. Removal is explicit operator maintenance. |
| M4 | Legacy state grants nothing. Unparsable, PID-only or foreign records are ignored, and none of them refuses, locks or adopts anything. |
| M5 | A legacy version without an importer is ignored, never guessed at. Doctor lists it. |
| M7 | Running stacks survive. Named volumes and Compose project names are unchanged, so the first `up` reconciles the running stack through the ordinary apply: Compose recreates what changed, once, and the data stays. |
| M8 | Downgrade is unsupported. The store records the writing build, and a binary that predates the schema refuses Mutate. |

The gateway has no handback: the endpoint Store is the only source of routes
and loopback aliases, and no scope exports or re-imports them
([architecture 035](../architecture/035-gateway-endpoint-store.md#no-downgrade-handback)).


| Legacy family | Location | Import | Quiesce condition | `Unknown` handling |
| --- | --- | --- | --- | --- |
| Task lease locks | `<root>/.effigy/locks/` | not read: run gates replace them, so nothing refuses, locks or waits on one | — | ignored, never deleted |
| Task status v1 | `<root>/.effigy/runtime/tasks/`, `<root>/.effigy/reports/tasks/` | not imported and not read; the one-minor read-only view is retired in v0.15 (M6), so old status history is dropped. Status history is not state: nothing restarts or unlocks from it. | — | — |
| Managed sessions, stop markers, exec group markers, lifecycle state | `<root>/.effigy/runtime/managed*/` | not imported and not read: no PID-only owner refuses, locks or adopts anything, so a rebooting host reuses a recorded PID without consequence | — | — |
| Environment markers, lease, persistent owner, handover, admission | `~/.effigy/runtime/host-container-leases/`, `<root>/.effigy/runtime/` | not read: the record is written by the environment's own transitions, and the first `up` reconciles the running stack through the ordinary apply | — | ignored, never deleted; named volumes and project names are unchanged, so the data stays |
| Generated renders | `<root>/.effigy/runtime/compose/` | not imported; re-rendered into objects on the next apply | — | — |
| Backend override metadata | `crates/effigy-containers/src/lib.rs:219-235` | the environment's backend field | — | unparsable: `Unknown` backend; destructive actions refuse until the backend is named |
| Host sidecars: PID, spec, listener records | `<root>/.effigy/runtime/host-processes/` | not read and not deleted: a host process is an environment-owned kernel run (effigy#320) | — | ignored; a leftover record never blocks a start |
| Runtime-scope records | `~/.effigy/runtime-scopes/` | retired: ownership is the backend's labels; a legacy directory is neither read nor written | — | — |
| Gateway PID, identity, version; routes, loopback | `~/.effigy/gateway/` | no import: the endpoint Store is the only source | — | — |
| Vault | `<checkout>/.effigy/secrets/local.vault` | not moved; the next write uses the pointer commit | — | unreadable vault: never overwritten |
| Host-run pending facts; retired admission store | `~/.local/state/host-run/`; `~/.cache/effigy/admission/` | untouched (contract 049) | — | — |

### 11.1 The unreleased-main central store

The central `~/.effigy/state/kernel.db` existed only on unreleased main between
`8fc1b9663` (2026-10-18) and effigy#276, so no released version wrote it, and
v0.15 imports nothing (effigy#307, effigy#373).

- `kernel.db`, `migration/cutover.json`, `migration-committed.json` and
  `migration-prepared.json` are all ignored. They are never read on a durable
  open, never changed, never deleted and never a refusal. Per M3 removal is
  explicit maintenance.
- A checkout database opens as its checkout's journal in place.
- There is no migration command and no migration report. The importer's
  preflight, stop controls, staged destinations, seal resumption and copy proof
  have no replacement; `docs/knowledge/retired.toml` lists their terms and
  paths.

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
  oracles. Each scenario uses six owned shard processes, with at most two
  scenarios active together. The driver reaps every sibling before reporting
  a failed shard, so a failure cannot orphan the rest of its proof.
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
| O6 | Every checkout `kernel.db` and `machine.db` passes integrity and local foreign-key checks; cross-store references have fenced, recoverable receipts (§7.5). |
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
| Accounted boundaries | `crates/effigy-containers/src/exec/process.rs` keeps one detached-invocation spawn (`std.process.Command.spawn`, 3) so `container up` can read the runtime's progress while it runs. It replaced the blocking `.status()` wait for that same invocation (`std.process.Command.status` 1 → 0), so no capability was added, only made observable (effigy#313). |
| Done | every product count is zero (lane "done when") |

### 12.5 Per-primitive obligations

Each primitive's proof list in §5 is mandatory before that primitive merges.
The fault suite (§12.1), with every oracle green, is mandatory before any
engine migrates onto the kernel.

Native probe signal grace stays 100 ms. Positive closure verification has a
separate five-second cleanup budget for identity scans, reaping and buffered
output; it never extends the workload deadline or turns unknown closure into
success.

### 12.6 Q-010 acceptance proofs

The following qualify the Q-010 storage and custody boundary. Every fixture
uses a fresh `mktemp -d` HOME, state roots and independent checkouts, never the
operator's `~/.effigy` or live gateway. Record started PIDs/process groups, stop
only those identities, remove fixtures and preserve exit codes before reporting.

| Proof | Required evidence |
| --- | --- |
| Classification | Table-driven CLI coverage of every §3.1 row, with text/JSON, TTY/pipe, heavy/no hook, nested scripts and late durable requests; no effect before refusal or durable admission |
| Write count | Instrument database opens/commits and filesystem writes/fsyncs before/after fresh and warm foreground tasks, sequences and test suites: **0 database writes, 0 durable runtime records**, no machine DB dependency. Show transient lock writes separately. Repeat with configured source preparation and prove honest durable classification. QA-group runs are checkout-durable: retain their definition, member ledger, result and logs across owner exit, with zero machine-store writes for local-only members. Measure their checkout writes separately. Detached runs report commits by admission, spawn, requests, settle, bytes and fsyncs; no per-output, poll or heartbeat journal writes |
| Isolation/concurrency | At least 20 overlapping short runs across at least 3 independent checkouts, while long detached runs and an environment transition run; zero cross-project lock refusals, including a deliberately stalled local retention writer. Same-resource contention still returns the specified Busy result. Busy/corrupt machine store cannot block local-only foreground work |
| Foreground custody | macOS/Linux: leader exit with live descendants, HUP/INT/TERM, owner SIGKILL, guard SIGKILL, both dead, cleared tags, escaped session, reused/foreign PGIDs and SIGSTOP; positive cleanup or explicit evidence-limited residue, no numeric recovery signal, no false persisted success, no unlock while the surviving custodian cleans up |
| Durable crash recovery | Kill/ENOSPC/EPERM/Unknown at every intent, receipt, activation and settle crossing for QA-group runs, `--run-id`, detached one-shot, each managed front end, environment transition, sidecar, workspace session, remote/container run, and gateway lifecycle/publication. Preserve O1–O14 with local integrity and cross-store receipts; one authority and no double release |
| Task status read model | `tasks status <task>` and `tasks status --all`, text/JSON: first foreground run with no token/DB is live, not never-run; free/stale gates and foreground exit produce not-recorded; old durable success stays historical; held/unverifiable identity and discovered residue are unknown; interrupted durable runs keep recovery commands. No history row or token is created. Validate the v3 envelopes and durable-only latest/history per 018 |
| Gates/layout/deletion | Fresh checkout, linked worktree, primary, non-Git root, same-path replacement, explicit retire and external deletion; gate inode continuity while held, token-init races, host-only guest masking, unknown residue/claims protected, no primary journal fallback |
| Discovery/admission | Configured/unconfigured roots, linked worktrees, duplicate/ambiguous keys, symlink, missing/unreadable/corrupt checkout, depth/count/time limits, empty roots; R2 filesystem diff empty. Nested live/dead/forged admission capabilities and hook death/owner death do not use a central run lookup |
| Retired central store | A machine home holding the retired `~/.effigy/state/kernel.db` opens normally: the file, any cutover manifest and any old per-checkout import record are ignored, never changed and never deleted |
| Deleted-worktree retirement (Tom, 2026-10-22) | Disposable backend stacks from deleted linked/Queue worktrees, with their checkout journals removed and no central index: `effigy container retire --dead` groups the labelled resources by checkout path, retires the groups whose path is gone, and preserves live/unknown/foreign groups, persistent data and the unselected backend |
| Gateway/environment regression | Private gateway and disposable engine fixtures: unchanged pins/routes after migration, proxy readiness, container reuse, R4 holder refusal, idle-stop and interrupted recovery; no calls to the unselected backend or operator's gateway |

Use installed Effigy selectors, and build the changed binary once per source
head for binary-level proofs (`oxide build --bin effigy`). Extend the covering
selectors rather than raw test commands: `test:kernel:*` families touched,
`test:run-control`, `test:run-control:cli`, `test:supervisor:faults`,
`test:kernel:environment`, `test:containers:environment`,
`test:environment:runner`, `test:kernel:runtime-routing`, and the touched
gateway endpoint/operator selectors. Include task-engine/sequence/test/QA-group coverage for classification.
New acceptance harnesses need narrow bounded Effigy selectors if none exists;
a missing harness is not a waived proof. Run `check:kernel`,
`qa:kernel-ratchet`, the covering strict-clippy selectors, `fmt:check` and
`qa:docs` once for the implementation change. Tests keep their declared timeout
and forced-exit settings. Hosted CI is not a gate; milestone QA belongs on main.

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
| 9 | `qa`, `ci`, `ci:fresh` hard-wired to the scheduler | closed by Q-006 (effigy#227): no name implies a class | R5 | `Run`; custody hook |
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

- No host-wide scheduler (vision). Heavy tasks carry metadata and an optional
  launcher hook (Q-006, contract 049).
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


## Child run admission API

`Run::begin_child` commits a required child intent before admission and writes
`runs.parent_run` with the child's Admitted row. Child IDs cannot be reused;
children hold independent owner gates. The engine rejects ancestor owner cycles
before admission. Parent success requires the child's positively recorded success;
parent settlement uses the same settle writer recursively under the child's gate,
including after owner death. An unknown or incomplete child remains parent residue.
`run::children` and `RunSummary::parent` expose the relation as pure reads.
Existing begin, intent, applied and settle fault points cover both sides of child
admission (`run_children_admission_faults_recover_both_sides`). No store migration
is needed: `parent_run` is part of store schema v1.

Host-run admission is decided once at the top-level invocation. In-process
sequence (including legacy draft aliases), setup and Rhai task calls inherit
that decision, including
direct execution without a scheduler. Canonical child resolution and kernel
task gates still apply; a nested heavy step never queues or validates host-run
admission again (Q-006). The legacy heavy task name rule remains top-level
policy until its separate retirement.

Consumer sequence compatibility compares JSON plans against the dispatch baseline
and replays the real composed step graphs with harmless leaf commands, preserving
nested references and injecting failures at step boundaries. These fixtures prove
orchestration behavior; live database, deployment and full workflow qualification
is separate and requires the operator's go.
