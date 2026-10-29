# 053 - Environment Reconciler Contract

Status: implemented in `crates/effigy-containers/src/environment/` (engine)
and `src/runner/environment.rs` (runner cutover), effigy#232. The
`data_from` data claim, retire as a transition, the workspace session run and
its host authority channel are later work (§9).

Q-010 storage follows contract [052](052-runtime-kernel-contract.md) §3.1,
§7 and §11, approved by planner and Tom (2026-10-22: "Approve"). Environment
records, renders and holders use their checkout journal; shared endpoint and
port facts use the machine store. No legacy family is imported (Q-012 §3).
The later workflows named in §9 remain separate from
this relocation.

Owner: environment reconciler (Queue lane `v015-environment`)
Kernel: [052](052-runtime-kernel-contract.md) §5.9, §11
Supersedes: the lease, lifecycle-marker and per-site holder-check sections of
[005](005-container-runtime-contract.md) (see [retired concepts](../retired.toml))
Related: [006](006-compose-backend-compatibility.md),
[049](049-heavy-validation-admission-contract.md), [questions](../questions.md)
Q-004, Q-005 and Q-010

## 1. Purpose

Each container environment has one kernel record and one authority. Every
change to an environment's containers is a transition run that owns
`Gate(Env)`, is journalled before its backend call (K3) and is refused while
another run holds the environment (R4). Holders attach through one hold API.
No lease file, timer, marker or per-call-site holder check remains.

Compose state plus the environment's own labels are the source of truth. Effigy
keeps no record of what it believes it applied: no applied generation, no
partial activation, no admission snapshot and no second change detector. One
apply converges the stack, and the next `up` reconciles from what is running.

### Q-010 storage and lifetime

Every environment transition, holder Session, environment-owned sidecar and
remote/container workload is checkout-durable, even when `up`, `exec` or a task
looks attached and short. Classification precedes setup/ensure and the first
effect. Local foreground runs without environment obligations use memory and
never enter the environment journal. `--headless` and TUI are presentations of
the same durable managed owner, not a way to select foreground durability.

All records below, render closures, backend metadata, holder runs, effects and
requests live in the environment owner's `<checkout>/.effigy/state/`. A linked
worktree owns its own state and tokens; it does not use its primary's journal.
Attach, activation and holder protection remain local atomic transactions.
A cross-checkout borrower uses the authority session protocol in 052 §7.2;
unknown/missing owner state never permits down, data reuse or adoption.

Endpoint/route claims and port pins alone live in the machine store. Before
requesting one the environment run commits an intent; the machine authority
returns an idempotent fenced receipt, then the checkout records it (052 §7.5).
Active publication needs every required receipt. No transaction or foreign key
spans stores, and loss of a reply retains protection rather than allocating
again. Polling, status, holder checks, ordinary reuse and task close do not
write machine state. Actual endpoint/port changes obey the machine write budget.

Deleting a checkout loses this journal; it does not stop a container or release
a route, port, data claim or unknown holder. Supported retire settles and
releases first. External deletion or same-path replacement leaves resources
as residue until verified retirement, with no fallback to a primary or central
journal (052 §7.1). Cross-checkout doctor discovers state read-only with explicit
coverage limits (052 §7.6).

## 2. The record

| Part | Where | Meaning |
| --- | --- | --- |
| Generation, state | `resources` row | `active` (attachable at the current generation), `stopped`, or neither while a run is in progress |
| Backend | `resources.backend` | exact engine, context and the policy source that chose it, written by Prepare (§6) |
| Engine fields | `resources.engine` | `{schema, retention, project, imported}` |
| Desired | `object_refs` | the render this environment points at (#229) |
| Holders | `attachments` | runs attached to the environment, with liveness from their owner gates |

`retention` replaces `.persistent` and the lease. There is no applied,
partial or superseded generation. Reading the record is pure (R2):
`EnvironmentView` and `EnvRecord::read` open nothing for writing, import
nothing and probe no backend. The state users see for a service is the
runtime's own: Running, Stopped or Absent.

## 3. Transitions

Every transition is one `EnvRun`: a kernel run owned by the environment, so
up, down, handover, reset, reap and import serialise on `Gate(Env)`.
A transition waits up to 10 minutes for the gate; idle stop and import do not wait.

- **Permit.** A backend effect needs a `TransitionPermit`. It is refused
  (`HeldBy`) while any holder remains. Before asking, the transition settles
  every interrupted holder as gate holder, taking that holder's owner gate
  `FailFast` (052 I-A2); live and unknown holders always refuse.
- **Busy.** An admission that finds `Gate(Env)` held is refused `Busy` with
  one sentence naming the holder the kernel's own gate evidence recorded: its
  run id, pid, task and freshly observed process state, and whether the wait
  ended. A holder whose identity cannot be verified is named by its category,
  never as live. The refusal acquires nothing, retries nothing, contacts no
  backend and releases nobody, and it adds no JSON payload beyond the command's
  error: `HeldBy` (a live holder protects the environment) and kernel journal
  contention stay distinct refusals.
- **Effects.** An apply is journalled `environment:apply` with `Retain`
  settlement; a stop is `environment:stop`, `Irreversible`. Recording either
  disposition calls no adapter, so it needs no holder check. A launch failure
  is `Failed` with proof; any other backend failure is `Unknown`, never
  manufactured success (R5).
- **Settlement budget.** Settlement is bounded on its own clock: admission
  makes its window once `Gate(Env)` is held, after a wait that may take ten
  minutes, and each settling holder and the run's own settle make theirs when
  the settlement starts. None reuses a context made when the transition
  opened (effigy#447). A cold stack whose activation outlasts a minute is
  therefore still settled afterwards instead of refusing `environment record
  ... timed out ... waiting for the environment to settle` on a budget its own
  apply had already spent, and a transition that waited for its gate settles
  an interrupted predecessor on a whole budget too. The budget stays bounded,
  and because an apply is `Retain` and a stop `Irreversible`, settlement
  dispatches no backend effect: it can neither act after a stop nor hold
  `Gate(Env)` longer than its bounded work. The transition reports its gate
  wait and its settlement as steps, so an operator sees which phase is
  running.
- **Bring-up** is two runs. The apply run publishes the render the
  environment points at, records retention, and runs the one apply unless a
  live run holds the environment:

  | Holders | Result |
  | --- | --- |
  | none, or only interrupted ones it settles | `Reconciled`: one apply |
  | live | `HeldByRuns`: the running stack is kept and the requesting runs attach |
  | unknown (holder list unreadable) | `Reconciled` with a warning: not held (Q-011), so the apply runs and the caller says the holders were not checked |

  The command's own steps (readiness, permissions, secrets, routes, host
  processes) run between the runs, outside the gate. The activation run
  publishes `active`, attaching requesting holders in the same transaction. A
  crash before activation leaves nothing to repair: the next `up` reconciles
  from what is running.
- **One apply.** `compose up -d --build --remove-orphans`, with no recreate
  flag, from the render. Compose recreates exactly the services whose rendered
  configuration changed (its own `com.docker.compose.config-hash`), so an
  edited image, environment or mount is applied by the next `up` without a
  `down`. Effigy adds no per-service digest label and no comparison pass. An
  apply failure is an error naming the service and Compose's own output; no
  failure is logged as a warning and continued.
- **Down** stops host processes, then the backend, publishes `stopped` with a
  fresh holder check, and ends persistent ownership. `container down`,
  `--global`, cwd-scoped down, attached-session closeout and failed attached
  bring-up cleanup all use this transition.
- **Handover** ends each live headless owner bound to the losing environment
  through its own stop, then runs a handover transition of that environment.
- **Reset** stops and drops only this environment's render references;
  sibling environments keep theirs (D3-07).

## 4. Hold API

The run supervisor attaches through this surface.

- `hold(run, ctx, env, wait, running)` attaches a live run to an environment
  under `Gate(Env)`. `running` is the runtime's own answer to whether the
  project's containers are up, so a stack deleted behind Effigy's back is
  never held. `NotActive` means the environment needs an ensure.
- A run that needs an ensure is attached by that bring-up's activation run
  (052 §5.9 `EnvNotActive`). A managed run passes its task-run id to its
  lifecycle's `effigy container up` in `EFFIGY_INTERNAL_ENVIRONMENT_HOLDER`.
- A command with no task run of its own (task activation, `exec`, a shell, a
  data command, the artifact builder) holds through a `Session`: a kernel run
  begun before the bring-up and settled when the command ends.
- Release is the holder's own settle; it takes no environment gate, so a
  holder and a waiting transition never wait on each other.
- A holder's own stop is exempt only once that run is settling: the settling
  process passes its own task-run id, and the kernel checks that the named run
  is `Stopping` or `Closed` (052 §5.9 `permit` exemption). A managed run
  records its outcome before its cleanup `container down`.

The holder variable confers no authority: it only requests protection for a
live, admitted run. The exemption is this process's own id, so it covers only
a run that is already ending.

Remote workloads (052 §5.10, effigy#236) bind the environment's generation at
intent and then check only its identity; every bring-up advances the
generation (planner ruling, 2026-10-20).

## 5. Retention and idle stop

| Retention | Set by | When no run holds it |
| --- | --- | --- |
| persistent | an explicit `effigy container up`; a v0.14.1 stack with no lease; `EFFIGY_DISABLE_HOST_CONTAINER_LEASE=1` for task starts | keeps running until `container down` |
| held | a task's ensure that started the stack | stops when its last holder settles |

Idle stop is holder-based. After a session settles, the reap transition
stops a held environment that has no live or unknown holder left; a
persistent one is borrowed (`Borrow` attachments) and left running. There is
no timer and no reaper process. A down ends persistence, so a later
task-started stack is held again. An ensure never demotes persistence.

## 6. Backend seam

`EnvironmentBackend` is the seam ("reconcile this environment from its
render", Q-004): `reconcile`, `stop` and an exact-identity `inventory`.
`ColimaBackend` runs `colima nerdctl --profile <profile> -- compose …`;
`DockerBackend` runs the host `docker` CLI on its resolved context (Docker
Desktop on macOS, Docker Engine on Linux). An Apple Containers adapter
implements the same trait; it must pass the strict-fake adapter suite and
052 §5.10 before it is selectable.

The engine and its precedence are one resolver,
`effigy_containers::backend::exact_backend` (effigy#230). Highest first:

1. `EFFIGY_COMPOSE_BACKEND` for this invocation;
2. the environment record;
3. the v0.14.1 per-checkout metadata file, read only until Prepare records
   the environment;
4. the user preference in `~/.effigy/config.toml`;
5. the manifest driver default.

Prepare is the record's only writer: it records engine, context and the source
label in `resources.backend` when it publishes the desired generation, and
never records a guess. The reconciler reads that record; its transitions,
like every Compose, volume, data and shared-service action, target that exact
backend (D3-25). Volume and data actions resolve the endpoint per environment
and refuse an unknown record. `effigy container status` and `effigy doctor`
show each environment's backend, why it was chosen, and its record.

## 7. No legacy import

v0.15 imports nothing from an earlier release (Q-012 §3, Tom 2026-10-26):
there is no marker importer, no resumable import record and no wait for an
unexpired lease.

An environment's record is written by its own transitions. A stack a v0.14.1
release left running is reconciled by the first `up`: the ordinary apply runs
`compose up -d --remove-orphans` from the current render, so Compose recreates
each service whose configuration differs and keeps every named volume. Named
volumes and Compose project names are unchanged between v0.14.1 and v0.15, so
the data stays. Upgrading straight from v0.14.1 therefore costs one container
recreate per stack, and no command.

Everything a v0.14.1 release wrote stays on disk, unread and unchanged:
lifecycle markers, leases and the persistent owner under
`~/.effigy/runtime/host-container-leases/`, and the retired runtime files under
`<checkout>/.effigy/runtime/`. Nothing refuses because of them, and nothing in
v0.15 deletes them; `effigy doctor` lists what it finds.

## 8. Guest

Inside a workspace container the engine runs with guest placement: every
transition refuses `HostAuthorityRequired` before touching state (R1), and
the record reads as kept on the host. Reading it through the host authority
channel waits for that channel (052 §8.3).

### 8.1 Host sidecars (adopted)

Host sidecars are adopted (052 §10.2, effigy#320): each declared host process
is an environment-owned detached kernel run with custody of its process group,
started after the environment's services report ready and in `depends_on`
order, stopped by `down`, `reset` and session settle with a
`HostProcessStopReport`.

## 9. Not yet

- `data_from` as a data claim (052 I-A3): current handover behaviour stays.
- `container retire` removes resources by scope record, not as a transition.
- The workspace session run and host authority channel (052 §8.3, §8.7).

## 10. Proofs

| Proof | Selector |
| --- | --- |
| Record, stopped publication, apply-run attach, exempt permit | `test:kernel:environment` |
| Strict-fake engine; transition matrix against live, interrupted and unknown holders; EPERM, ENOSPC and SIGKILL at every crossing of up, down and reset, with O2, O3, O5, O6, O7 oracles; process-level strict fakes for both adapters | `test:containers:environment` |
| Runner cutover: lifecycle, task activation, managed runs, handover, deferral, builder recovery | `test:environment:runner` |
| Busy refusal diagnostics: the live exact-gate holder, terminal and successor observations, unreadable identity, the original wait deadline, explicit unknown/absent record semantics, held-by and journal-contention controls, with no backend call | `test:environment:busy-diagnostics` (the one-sentence refusal in `runner::environment::tests` and the refusal-command test) |
| One apply: `up -d --remove-orphans` from task preparation, and a compose failure reported as an error | `test:containers:mount-prepare`, `test:containers:generations:callers` |
| Touched crates compile and lint for the busy diagnostics | `check:environment:busy-diagnostics`, `lint:environment:busy-diagnostics` |

Real-engine qualification belongs to the qualification lane. On Colima and
on Docker Desktop, with cbs and Acowtancy:

1. Fresh `HOME`: `effigy container up --detach`; `status` names the backend
   and why; the record is active and persistent.
2. Edit a service's env or image tag and run `up`: only that service is
   recreated; a second `up` with no edit recreates nothing.
3. `effigy container down` while `dev` runs: refused, naming the run.
4. Task-start a stack, let the task end: it stops; `container up --detach`
   keeps the next one running.
5. Kill a task with SIGKILL mid-run, then `container down`: it settles the
   interrupted holder and stops.
6. Upgrade from v0.14.1 with stacks running: no container restarts; the
   record imports; marker files are byte-identical.
7. With both engines installed, the unselected engine sees no call.

Q-010 adds crash recovery for each durable class, multi-checkout concurrency,
missing/deleted checkout and large-store migration proofs in 052 §12.6. The
selectors above and touched gateway selectors must cover the new store paths
and fenced cross-store receipt boundaries. Phase 1 runs only `qa:docs`; no
runtime recovery or migration proof is claimed by this amendment.
