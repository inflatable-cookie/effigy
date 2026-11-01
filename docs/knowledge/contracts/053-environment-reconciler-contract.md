# 053 - Environment Reconciler Contract

Status: implemented in `crates/effigy-containers/src/environment/` (engine)
and `src/runner/environment.rs` (runner cutover), effigy#232. The
`data_from` data claim and retire as a transition are later work (§9). The
workspace session run and its host authority channel are implemented at the
kernel and runner seam (052 §8.3, §8.7, §8.11); their production activation
caller remains under development on the unmerged #512 branch; native backend
qualification remains outstanding.

For v0.15, workspace Level 1 forwards non-interactive commands only;
guest PTY, attach, reconnect and detach acknowledgement are deferred to
Level 2 as specified in 052 §8.11. This does not remove the environment
activation or its detached session supervisor.

### Workspace apply boundary (effigy#533, 2026-11-02)

The one ordinary Compose apply is an explicit boundary
(`src/runner/environment_workspace/private_apply.rs`). The owner resolves the
frozen command and the guarded Env run hands the strategy its own `Run`, child
`EffectContext` and absolute deadline through
`WorkspaceActivation::apply_sealed_with_owner`; nothing downstream can reopen a
budget. Production supplies `private_apply::qualified`. A proof substitutes a
fake backend at that same external process boundary, so the publication
sequence is exercised against the real owner — journal, gate order, binding,
gated re-observation, commit, selector switch and recovery — while the process
custody of the apply is not claimed.

The qualified invocation is still refused. A same-Env private apply needs the
Env run to own kernel custody (`Custody::spawn_auxiliary` refuses a run without
a sentinel), and the workspace activation admits that run through
`Run::begin_claimed_recoverable`, which refuses an activation that would carry
one (`crates/effigy-kernel/src/run/mod.rs`, "recoverable claimed admission
requires an environment activation without a sentinel"). Resident Env custody,
its sentinel placement and its owner-death/old-reader recovery are the deferred
S1 item from #512; `Terminal::Discard`, the other way to keep backend bytes out
of retained logs, is refused until helper identities carry a verified image
(`crates/effigy-kernel/src/custody.rs`). Until those land, no workspace apply
starts a backend and every deferred proof that needs one states that by name.

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
| Engine fields | `resources.engine` | `{schema, retention, project}` |
| Desired | `object_refs` | the render this environment points at (#229) |
| Holders | `attachments` | runs attached to the environment, with liveness from their owner gates |

`retention` replaces `.persistent` and the lease. There is no applied,
partial or superseded generation. Reading the record is pure (R2):
`EnvironmentView` and `EnvRecord::read` open nothing for writing, import
nothing and probe no backend. The state users see for a service is the
runtime's own: Running, Stopped or Absent.

## 3. Transitions

Every transition is one `EnvRun`: a kernel run owned by the environment, so
up, down, handover, reset and reap serialise on `Gate(Env)`.
A transition waits up to 10 minutes for the gate; idle stop does not wait.

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
- **Effects.** An ordinary or reserved apply is journalled
  `environment:gate-scoped-id-difference` with `Retain` settlement; a stop is
  `environment:stop`, `Irreversible`. Recording either
  disposition calls no adapter, so it needs no holder check. A launch failure
  is `Failed` with proof; any other backend failure is `Unknown`, never
  manufactured success (R5).
- **Creation inference.** That apply record uses the same permit, gate,
  retention and one caller deadline. Ordinary unsealed applies produce an explicit
  unproven
  outcome; only the sealed workspace route may attribute creation. Complete
  before/after project inventories bind exact
  full IDs to frozen service, project, managed, scope and checkout labels and
  explicit image capabilities (052 §8.12): container image ID; reference plus
  stable resolved image-inspect ID only for IDs created in this gate; digest-pinned
  reference; or attached reference/config-hash with no resolved container-image ID.
  Attached reference-only rows must match Compose's config hash for the merged
  frozen render. A pre-window tag move underneath an attached container is
  undetectable on this engine: the current lookup is never claimed as that
  container's actual image (Supervisor under Tom’s standing delegation, 2026-10-31;
  [Q-017 amendments](../questions.md#workspace-activation-amendments)). Nerdctl's absent one-off
  flag requires actual project/service, name and frozen network corroboration.
  Observed tag moves refuse; same-user retag/restore inside the gate is outside
  the narrowed guarantee (Supervisor under Tom’s standing delegation, 2026-10-31;
  [Q-017 amendments](../questions.md#workspace-activation-amendments)).
  The post-apply service set must equal the frozen
  set. Before apply, the workspace predecessor is allowed to remain attached or
  be replaced for any Compose reason, including environment, mount and entrypoint
  changes at the same image reference. Replacement requires its predecessor to
  disappear and one new full ID absent from the complete pre-apply set. The
  new-ID difference must equal that optional workspace replacement plus the
  required fresh/recreated services; other attached dependencies retain their
  exact IDs (Supervisor under Tom’s standing delegation, 2026-10-31;
  [Q-017 amendments](../questions.md#workspace-activation-amendments); 052 §8.12). The record is
  explicitly a gate-scoped inference, never an engine create response. A same-user
  actor forging identical labels inside this exclusive window is outside the
  v0.15 guarantee (Operator ruling (Tom, 2026-10-31, on the Supervisor’s recommendation);
  [Q-017](../questions.md#q-017-workspace-client-and-operator-feedback-at-v015)).
  An `Absent` reservation names an unknown desired successor,
  including an authorized non-live predecessor replacement, and binds only this
  run's exact new ID; an `Observed` target must retain its exact ID. #506's gated
  compatibility check must refuse recreation of a live protected predecessor. The
  unmerged integration retains the activation's owned artifact, alias and boxed
  probe and its one absolute deadline; the gated reobserve is an independent
  fresh backend read on that same deadline, fenced to the exact ID, never a
  pass-through of an earlier observation, and it fails closed on unknown, changed
  or missing evidence (Supervisor under Tom’s standing delegation, 2026-11-01;
  [Q-017 amendments](../questions.md#workspace-activation-amendments); 052 §8.11).
  Every actual Compose input, including the generated Compose file, belongs to
  the frozen render closure. Before hashing/apply the adapter verifies the file
  list and exact bytes against that closure; a missing input refuses rather than
  extending the seal from containers (Supervisor under Tom's standing delegation,
  2026-11-01; [Q-017 amendments](../questions.md#workspace-activation-amendments)).
  Failed apply or incomplete observations produce no creation
  receipt. A retry reads only this run's journalled result and never reapplies or
  invents a later receipt. `created_by(project, full_id)` reads that journal;
  missing evidence is Unknown, never a global NoCreator claim. Bare Active
  publication refuses a reserved generation, including its claiming run; the
  sanctioned reserved Active route is the merged `commit_selection`/
  `commit_activation` transaction with guest/serving receipts (052 §8.11), while
  the production activation caller (#506) and native guest E2E (#512) stay
  separate, unmerged work. #510's native target-binding proof does NOT prove
  guest workspace-session
  commit/publication. Retained records
  remain readable after settle (effigy#510, 052 §8.12).
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
  from what is running. Exposure is decided per declared route target: a host
  listener that never reaches its own readiness withholds only its own route,
  the container's published routes and the other listeners' routes stay
  exposed, and the activation still publishes `active`. A detached command
  reports that partial exposure per route rather than the whole activation's
  readiness and exposure as unverified (effigy#455).
- **Reserved workspace ordering.** Supervisor ruling, 2026-11-01:
  Phase 0 is CLI read-only checkout/backend/profile identity prevalidation;
  a mismatch has zero preparation effects. Phase 1 is ordinary idempotent
  `prepare_sources`/`prepare_container_up` and generated Compose preparation
  in the CLI, before any seal, reservation or gate. This adds no new timeout
  to ordinary preparation; retry reruns it.

  The CLI hands **only declarative post-prep inputs** (paths, closure
  list/identities, expected hashes, profile/backend identity and deadlines)
  to the detached supervisor. The supervisor repeats read-only prevalidation,
  then constructs and retains the **one `SealedPreflight` lease and frozen
  plan in its own process**. No lease is serialized, reconstructed or
  transferred. A mismatch between declared post-prep facts and the sealed
  hash/closure refuses by name before apply; any change after sealing fails
  the closure/hash check.

  Supervisor Phase A holds `Gate(Env)` for one apply, target observation,
  creator attribution and reserved target/platform binding, then parks the
  same run and releases Env. The same reservation owner journals and runs
  readiness, permissions, secrets, routes and declared host integrations.
  Gate-free integrations have a separate explicit budget (generous internal
  default: ten minutes), checked before each step and after it returns.
  Supported helper timeouts are capped to remaining budget; an overrun by a
  non-interruptible helper refuses with `integration budget exhausted`.
  This is not an interruptible preparation or integration guarantee.

  Before publication, Phase B requires the entire policy-required integration
  plan completed, reacquires Env and freshly revalidates reservation, exact
  environment/session owner/run/generation, backend/target/boot/platform,
  creator receipt and created-ID set, selection inputs, original reservation
  deadline and integration budget. An empty or partial set of completion
  markers is not proof of a complete plan. Guest publication, commit and
  selection use that same guarded owner. A mismatch or expired budget refuses.
  Reverse unwind uses a fresh cleanup budget. An uncleared Attempted or
  Completed integration journal entry prevents reservation cancellation even
  after both runs have settled; Retain is not external cleanup. No second
  apply or owner may stand in for the original activation.
  Custody effects are journalled and crash-recoverable. Readiness, secrets,
  routes and host integrations are idempotent reconciled effects: after a
  crash the next `up` repeats them, reproducing the same runtime secret files
  and routes for unchanged inputs (Supervisor, 2026-11-01). Before commit,
  unwind follows the ordinary reverse best-effort order. An unknown external
  effect leaves the reservation recoverable and reports `integration effects
  unsettled; re-run up`; it never reports settlement based on an invented
  receipt. Authenticated integration receipts are deferred beyond v0.15.
- **One apply.** `compose up -d --build --remove-orphans`, with no recreate
  flag, from the render. Compose recreates exactly the services whose rendered
  configuration changed (its own `com.docker.compose.config-hash`), so an
  edited image, environment or mount is applied by the next `up` without a
  `down`. Effigy adds no per-service digest label. Attached reference-only image
  evidence requires a bounded read of Compose's own `config --hash` over the
  ordered frozen files; this observation is not a second apply. Partial
  extra-hosts overrides inherit the base service's network declarations. An
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

**Operator ruling (Tom, 2026-11-01):** v0.15 requires Level 1 of
[052 §8.6](052-runtime-kernel-contract.md#86-coverage); unsupported Level 2
guest operations refuse by name. Channel or identity failure never permits
guest-state mutation, local fallback or resending an indeterminate request
after `Unknown`. An independently invoked host CLI is not a guest retry and
remains subject to the owning engine's session/reservation, canonical-gate,
custody and authorization checks. The session-aware down and reserved
outside-gate ordering in §3 remain mandatory.

### 8.1 Host sidecars (adopted)

Host sidecars are adopted (052 §10.2, effigy#320): each declared host process
is an environment-owned detached kernel run with custody of its process group,
started after the environment's services report ready and in `depends_on`
order, stopped by `down`, `reset` and session settle with a
`HostProcessStopReport`. A sidecar listener's readiness is its own route claim:
when it fails, its own route stays withheld while the environment's published
routes and the other sidecars' routes remain, and a `depends_on` sidecar of a
withheld listener withholds its route too, through the whole chain (effigy#455).
The current dependency decision precedes reconciliation of any previous
generation. A withheld dependent's old run is settled before the report,
including a process without a listener; an incomplete stop remains a refusal.
Sidecar startup shares the bring-up's one runtime-observation budget: a listener
still starting when it is spent withholds its route and ends the command's wait
rather than holding the activation open, while the supervisor keeps its own
readiness semantics.
Every withheld listener start is settled before the command returns, including
a retrying failure before its first child. Only successful starts are detached;
the withheld supervisor cannot publish a late route.

The run's own owner publishes the environment identity and the whole host graph
before its environment phase can block; every host entry waits for that phase
first, and the planner takes the same records over once readiness is observed.
Within the planner the graph is published before the first process can block,
so a dependent's tab names its reader while that reader is still becoming
ready. It then publishes what each process waits
for — its `depends_on` reader and that reader's phase, or its own declared
readiness request with elapsed time — as an `effigy.managed.entry-wait.v1`
record under the checkout's `.effigy/runtime/waits/<scope>/`, and records each
wait as settled when it ends. A prerequisite failure is published as
`not_started` with the failing entry and its report, no dependent starts, and
the plan returns the failure. A record is written once when its meaning changes
and refreshed only while it is a live wait, so a settled entry is never
rewritten on an owner-loop tick. Exactly one publisher owns the scope: the run's
owner publishes it before the phase can block, and the host planner takes it over
before it starts any listener. Each publisher stages a complete snapshot in its
own directory and commits it by replacing one record, so a reader always resolves
a whole snapshot — the previous publisher's until the new one is complete — and a
write that races the handoff cannot replace an authoritative entry or failure.
The environment phase keeps its own reserved record, never an entry name, so a
host process an operator names `environment` keeps its own wait; the run's owner
reads that record for the container counts every phase-dependent entry names. A
read accepts a snapshot only while it is still the visible one — a listing a
commit retires mid-read is re-resolved — so an in-flight front end never observes
a missing or partial snapshot across the handoff. The host plan launches entries
one at a time in its resolved dependency order, so each queued entry names the
launch the plan is performing now, with that entry's actual phase, as its own
wait (`launch`) — distinct from a declared `depends_on` edge and never
attributed as one. A launch's hold ends when that launch returns, so a
non-listener that finished its launch is never the queued entry's hold. An entry
nothing has observed yet — no published record, no settled launch, no log line
and no ready URL — reports no status at all: neither the text nor a tab claims a
host is starting before the plan launched it. A launch that aborts the plan marks every entry it never
started `not started` with the plan's own attribution
(`plan stopped after <entry> <report>`), so no wait outlives the plan. A restart
publishes only the entries it replaces: a reader another call started stays
running in the graph, and a selected dependent names the readiness it
re-confirms (`waiting for api (running)`) while it blocks. A failure the planner
published is terminal and is never replaced by the run's generic `up` exit; the
run's own environment reason carries the planner's failing entry and its report.
The environment phase's own record is terminal on every exit — a success, a stop
or a nothing-to-await fallback settles it, and a failure or timeout publishes its
headline and first piece of evidence. A new session resets that record at its own
boundary, *before* the run starts polling it — for every named environment, with
or without host processes, and a `container up` resets it before its compose
step, held or not — so an interrupted prior run's container counts are never
projected into the new run, and the staged host snapshot is a separate record
that the reset never touches. Only a dependent
that needs a failed entry's readiness loses its prerequisite; a dependent that
only needed the entry's start keeps going, including its own scheduled delay. `status --json` reads both scopes — the run's
scope names its environment before the phase blocks — and reports each waiting
entry with the scope that published it. A record's path encodes its accepted
entry name injectively (K1: every byte outside `[A-Za-z0-9_-]` as `%XX`), and a
task scope keeps the task and profile as separate components, so two distinct
entries or task/profile pairs never share a record. Front ends read those records; they are
a projection, never authority (effigy#458).

## 9. Not yet

- `data_from` as a data claim (052 I-A3): current handover behaviour stays.
- `container retire` removes resources by scope record, not as a transition.
- The #512 workspace activation caller has an unmerged production-orchestration
  implementation and a prior fake-backend end-to-end result. It is currently
  guarded before preparation/admission until private-output runner and
  workspace-preparation paths are qualified and connected; review of cleanup-only
  guarded unwind and the integration-budget recheck after observation remains
  required. This evidence does not qualify a real backend. Remaining Level 1
  command and Observe coverage beyond `runs:list`, including run-specific
  `runs:status`, and native backend qualification remain outstanding
  (052 §8.6, §8.11, §8.12). Level 2 command coverage follows
  v0.15. The session run and channel are implemented as kernel and runner
  seams; those seams do not qualify the production caller or a backend.

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
6. Upgrade from v0.14.1 with stacks running: the first `up` reconciles the
   running stack in one apply, recreating each changed service once on its
   unchanged named volume; no importer runs and marker files stay byte-identical.
7. With both engines installed, the unselected engine sees no call.

Q-010 adds crash recovery for each durable class, multi-checkout concurrency,
missing/deleted checkout and large-store migration proofs in 052 §12.6. The
selectors above and touched gateway selectors must cover the new store paths
and fenced cross-store receipt boundaries. Phase 1 runs only `qa:docs`; no
runtime recovery or migration proof is claimed by this amendment.

## 11. Future detached-owner journal boundary

The Supervisor's 2026-11-01 Level 1 ruling in 052 §8.7 requires lifecycle E to
introduce a v2 owner-claim journal boundary, including read-only v1 observation,
a verified, transactionally consistent adjacent 0600 SQLite backup including
committed WAL state, atomic idempotent migration, explicit old-writer refusal and
tested backup restoration for supported rollback. A crash before commit leaves
valid v1; after commit it leaves valid v2 and the verified v1 backup. Detached
acknowledgement, client-claim and release-cause tombstones cannot expire while
any related owner, claim or operation is live, unresolved or Unknown; only then
may the N-day policy apply. Missing legacy evidence is Unknown. Rollback requires
quiescence and identity/ownership checks, and refuses potential post-backup data
loss or orphaned resources absent explicit operator-authorized recovery. The migration, data-loss review and backup/restore review remain
future work and do not qualify the current workspace activation caller.

## 12. Gateway diagnostic disposition before v0.15 freeze

The ordinary gateway startup path can write raw daemon stdout and stderr to
gateway logs, including diagnostic URLs or other secret-bearing upstream text.
New gateway log files are created with mode 0600, but reopening a pre-existing
0644 file does not tighten it; legacy directory and ancestor permissions can
permit other local users to read those files. New mode 0700 directories do not
repair existing directories. These facts do not establish global sanitization or
private permissions for every existing gateway installation. The v0.14.1 writer
already emitted raw output with umask-dependent modes; the current new-file mode
is an improvement, not a migration of old files.

Security lead `8d767d55` requires Tom's disposition before v0.15 freeze. This is
separate from the release-blocking requirement for private or discarded output
on workspace Phase A apply and preparation. No native gateway qualification or
release acceptance follows from the present source audit.
