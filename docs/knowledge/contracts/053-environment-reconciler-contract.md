# 053 - Environment Reconciler Contract

Status: implemented in `crates/effigy-containers/src/environment/` (engine)
and `src/runner/environment.rs` (runner cutover), effigy#232. The
`data_from` data claim, retire as a transition, the workspace session run and
its host authority channel are later work (§9).

Q-010 storage follows contract [052](052-runtime-kernel-contract.md) §3.1,
§7 and §11, approved by planner and Tom (2026-10-22: "Approve"). Environment
records, renders and holders use their checkout journal; shared endpoint and
port facts use the machine store. §11.1's central-store importer is retired
(effigy#307), so v0.15 ships only the §11 v0.14.1 family importers. The later
workflows named in §9 remain separate from this relocation.

Owner: environment reconciler (Queue lane `v015-environment`)
Kernel: [052](052-runtime-kernel-contract.md) §5.9, §10.3, §11
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
| Engine fields | `resources.engine` | `{schema, retention, project, applying, imported}` |
| Desired, last applied, superseded | `object_refs` | render digests (#229); the last 3 superseded are kept |
| Holders | `attachments` | runs attached to the environment, with liveness from their owner gates |

`applying` names a desired digest whose apply ran but whose activation did
not finish: a partial generation. It replaces `.partial.json` and
`.complete`. `retention` replaces `.persistent` and the lease. Reading the
record is pure (R2): `EnvironmentView` and `EnvRecord::read` open nothing for
writing, import nothing and probe no backend.

## 3. Transitions

Every transition is one `EnvRun`: a kernel run owned by the environment, so
up, down, handover, reset, reap, repair and import serialise on `Gate(Env)`.
A transition waits up to 10 minutes for the gate; idle stop and import do not wait.

- **Permit.** A backend effect needs a `TransitionPermit`. It is refused
  (`HeldBy`) while any holder remains. Before asking, the transition settles
  every interrupted holder as gate holder, taking that holder's owner gate
  `FailFast` (052 I-A2); live and unknown holders always refuse.
- **Busy.** An admission that finds `Gate(Env)` held is refused `Busy` with
  the kernel's own gate evidence carried through: the holder's run id and pid
  with its freshly observed process state, the wait that ended and the
  deadline it was given, and one pure read of the record's state and
  generation. A holder whose identity cannot be verified is reported as
  unknown, never as live; one that has exited, or whose pid now belongs to a
  successor process, is named as such. The refusal acquires nothing, retries
  nothing, contacts no backend and releases nobody. `HeldBy` (a live holder
  protects the environment) and kernel journal contention stay distinct
  refusals. `--json` carries it as `effigy.environment-busy.v1`.
- **Effects.** An apply is journalled `environment:apply` with `Retain`
  settlement; a stop is `environment:stop`, `Irreversible`. Recording either
  disposition calls no adapter, so it needs no holder check. A launch failure
  is `Failed` with proof; any other backend failure is `Unknown`, never
  manufactured success (R5).
- **Bring-up** is two runs. The reconcile run publishes the desired
  generation, records backend and retention, and reaches the backend only
  when needed:

  | Running (exact inventory) | Desired is last applied, partial, or an adopted import, with no observed mount drift | Holders | Result |
  | --- | --- | --- | --- |
  | yes | yes | any | `Reused`: no backend call |
  | yes | no | yes | `KeptForHolders`: running generation kept, a warning names the pending change |
  | otherwise | | none | `Reconciled`: one apply |
  | `Unknown` | | | refused; nothing recreated (R3) |

  The command's own steps (readiness, permissions, secrets, routes, host
  processes) run between the runs, outside the gate. The activation run
  promotes the generation and publishes `active`, attaching requesting
  holders in the same transaction. It refuses when a transition stopped the
  environment or replaced its desired generation in between. A crash leaves a
  partial generation that the next `up` reuses without recreation.
- **Down** stops host processes, then the backend, publishes `stopped` with a
  fresh holder check, and ends persistent ownership. `container down`,
  `--global`, cwd-scoped down, attached-session closeout and failed attached
  bring-up cleanup all use this transition.
- **Handover** ends each live headless owner bound to the losing environment
  through its own stop, then runs a handover transition of that environment.
- **Reset** stops and drops only this environment's render references;
  sibling environments keep theirs (D3-07).
- **Repair** (the exec-readiness restart) is a transition: it refuses while
  any run holds the environment.

## 4. Hold API

The run supervisor attaches through this surface.

- `hold(run, ctx, env, wait)` attaches a live run to an active environment
  under `Gate(Env)`. `NotActive` means the environment needs an ensure.
- A run that needs an ensure is attached by that bring-up's activation run
  (052 §5.9 `EnvNotActive`). A managed run passes its task-run id to its
  lifecycle's `effigy container up` in `EFFIGY_INTERNAL_ENVIRONMENT_HOLDER`.
- A command with no task run of its own (task activation, `exec`, a shell, a
  data command, the artifact builder) holds through a `Session`: a kernel run
  begun before the bring-up and settled when the command ends.
- Release is the holder's own settle; it takes no environment gate, so a
  holder and a waiting transition never wait on each other.
- A holder's own stop is exempt only once that run is settling:
  `EFFIGY_INTERNAL_ENVIRONMENT_EXEMPT` names it, and the kernel checks that
  the named run is `Stopping` or `Closed` (052 §5.9 `permit` exemption).
  A managed run records its outcome before its cleanup `container down`.

Neither variable confers authority: the holder variable only requests
protection for a live, admitted run; the exemption only covers a run that is
already ending.

Remote workloads (052 §5.10, effigy#236) bind the environment's generation at
intent and then check only its identity, because every bring-up, including a
`Reused` one, advances the generation (planner ruling, 2026-10-20). A
non-destructive bring-up that leaves the generation unchanged would be better
hygiene; it is a recorded follow-up, not required by remote custody.

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

## 7. Marker importer

The first Mutate of an environment imports its v0.14.1 markers from
`~/.effigy/runtime/host-container-leases/<profile>/<project>/` (052 M6):

| Marker | Record |
| --- | --- |
| `<name>.persistent` | persistent |
| `<name>.json` lease, no persistent marker | held |
| neither | persistent (v0.14.1 never idle-stopped it) |
| `<name>.partial.json` without `.complete` | a failed-unknown partial generation (052 §11) |
| ownership labels or, for a pre-label stack of an unscoped checkout, a bind mount of the checkout on every workload, with the primary running | adopted `active`, no backend call (M7) |
| anything less | not active: holds refuse and transitions need the reconciler's own ownership proof (M4) |

The import is idempotent and resumable after a crash. It never changes or
removes a legacy file (M3) and records each marker's path and SHA-256. While
an unexpired v0.14.1 lease remains (its reaper may still be sleeping), the
first Mutate waits for it, up to 10 minutes, before adopting. A later `up`
adopts an imported stack without recreation when the v0.14.1 admission
matches the current inputs.

### Central-store relocation (retired 2026-10-24)

The 052 §11.1 importer for the current-main central store is retired
(effigy#307). v0.15 ships only the v0.14.1 importers above, so an environment
closure arrives through the family import on first use: desired/applied/partial
generations, backend, retention, object refs and holders keep their identity,
and containers are not recreated (M7). A machine that committed the retired
cutover ignores the old store and never deletes it; one still holding it
without a committed manifest is refused once with the Effigy main `7b82d26`
upgrade path. PID-only evidence remains Unknown, and old input stays
byte-identical (M1/M3/M4/M8).

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
| Strict-fake engine; transition matrix against live, interrupted and unknown holders; EPERM, ENOSPC and SIGKILL at every crossing of up, down, reset and import, with O2, O3, O5, O6, O7 oracles; process-level strict fakes for both adapters; importer against copies of real Underlay and DecodeLabs `nerdctl inspect` captures | `test:containers:environment` |
| Runner cutover: lifecycle, task activation, managed runs, handover, deferral, builder recovery | `test:environment:runner` |
| Busy refusal diagnostics: the live exact-gate holder, terminal and successor observations, unreadable identity, the original wait deadline, explicit unknown/absent record semantics, held-by and journal-contention controls, with no backend call | `test:environment:busy-diagnostics` |
| Touched crates compile and lint for the busy diagnostics | `check:environment:busy-diagnostics`, `lint:environment:busy-diagnostics` |

Real-engine qualification belongs to the qualification lane. On Colima and
on Docker Desktop, with cbs and Acowtancy:

1. Fresh `HOME`: `effigy container up --detach`; `status` names the backend
   and why; the record is active and persistent.
2. Run a container task twice: the second reuses the stack (no recreate).
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
