# 053 - Environment Reconciler Contract

Status: implemented in `crates/effigy-containers/src/environment/` (engine)
and `src/runner/environment.rs` (runner cutover), effigy#232. The
`data_from` data claim, retire as a transition, the workspace session run and
its host authority channel are later work (§9).

Owner: environment reconciler (Queue lane `v015-environment`)
Kernel: [052](052-runtime-kernel-contract.md) §5.9, §10.3, §11
Supersedes: the lease, lifecycle-marker and per-site holder-check sections of
[005](005-container-runtime-contract.md) (see [retired concepts](../retired.toml))
Related: [006](006-compose-backend-compatibility.md),
[049](049-heavy-validation-admission-contract.md), [questions](../questions.md)
Q-004 and Q-005

## 1. Purpose

Each container environment has one kernel record and one authority. Every
change to an environment's containers is a transition run that owns
`Gate(Env)`, is journalled before its backend call (K3) and is refused while
another run holds the environment (R4). Holders attach through one hold API.
No lease file, timer, marker or per-call-site holder check remains.

## 2. The record

| Part | Where | Meaning |
| --- | --- | --- |
| Generation, state | `resources` row | `active` (attachable at the current generation), `stopped`, or neither while a run is in progress |
| Backend | `resources.backend` | exact engine, context and the policy source that chose it (§5) |
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

Selection, highest first, with the reason recorded and shown:

1. `EFFIGY_COMPOSE_BACKEND` for this invocation;
2. the environment record (the backend its stack was applied on);
3. the v0.14.1 per-checkout metadata file, read only until a bring-up
   records the environment;
4. the user preference in `~/.effigy/config.toml`;
5. the manifest driver default.

Every Compose, volume, data and shared-service action targets that exact
backend. A volume action on a profile resolves the backend of the
environments using it, never the profile's name; environments on different
backends refuse (D3-25). `effigy container status` and `effigy doctor` show
each environment's backend, why it was chosen, and its record.

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

## 8. Guest

Inside a workspace container the engine runs with guest placement: every
transition refuses `HostAuthorityRequired` before touching state (R1), and
the record reads as kept on the host. Reading it through the host authority
channel waits for that channel (052 §8.3).

## 9. Not yet

- `data_from` as a data claim (052 I-A3): current handover behaviour stays.
- `container retire` removes resources by scope record, not as a transition.
- The workspace session run and host authority channel (052 §8.3, §8.7).
- Host sidecar supervisors as environment-owned runs (052 §10.2).

## 10. Proofs

| Proof | Selector |
| --- | --- |
| Record, stopped publication, apply-run attach, exempt permit | `test:kernel:environment` |
| Strict-fake engine; transition matrix against live, interrupted and unknown holders; EPERM, ENOSPC and SIGKILL at every crossing of up, down, reset and import, with O2, O3, O5, O6, O7 oracles; process-level strict fakes for both adapters; importer against copies of real Underlay and DecodeLabs `nerdctl inspect` captures | `test:containers:environment` |
| Runner cutover: lifecycle, task activation, managed runs, handover, deferral, builder recovery | `test:environment:runner` |

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
