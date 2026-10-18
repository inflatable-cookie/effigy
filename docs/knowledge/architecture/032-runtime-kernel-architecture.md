# 032 - Runtime Kernel Architecture

Contract: [052](../contracts/052-runtime-kernel-contract.md)
Status: proposed. No `effigy-kernel` crate exists. This note places the
kernel in the codebase, names the processes it runs, and orders the kernel
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
           signal · remote custody contract · observe · probe · outcome
```

Dependency rules, enforced by `qa:architecture`:

| From | May depend on | Must not depend on |
| --- | --- | --- |
| `effigy-kernel` | std, `libc`, `rusqlite` (bundled), `serde`, `serde_json`, `sha2` | any other Effigy crate; Tokio; any adapter |
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
 host                                                          guest (Linux container)
 ┌──────────────────────────────────────────────┐              ┌──────────────────────────┐
 │ effigy <task>        inline owner (client)   │──exec──────▶│ workload session (setsid)│
 │   └─ effigy __kernel-sentinel   (per run)    │  in-band ack │   └─ guest effigy (opt.) │
 │ effigy dev           owning client           │◀────────────│      state: guest $HOME  │
 │   └─ effigy __kernel-supervise  (per run)    │              └──────────────────────────┘
 │        └─ effigy __kernel-sentinel (per run) │
 │ effigy status        observer                │
 │   └─ effigy __kernel-probe-guard (backup)    │
 │ effigy gateway daemon  (user-owned, resident)│
 │ ~/.effigy/state/  kernel.db · gates · sock   │
 └──────────────────────────────────────────────┘
```

- The only resident process is the user gateway. Supervisors and sentinels
  live for one run and its settle; a probe guard for its observer's open
  probes.
- Every per-run process re-execs the binary identified at startup.
- Guests never see `~/.effigy/state`. Only immutable object closures may be
  mounted, read-only, through a backend.

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
supervisor: Run::begin → socket → "admitted" → setup steps (journalled)
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
  unreaped and kills the group at the deadline; the guard (pidfd-pinned group
  on Linux, verified leader or nonce on macOS) stops it on observer death or
  at the deadline plus grace; processes without evidence are left alone
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
