# 053 - Environment Reconciler Contract

Status: implemented in `crates/effigy-containers/src/environment/` (engine)
and `src/runner/environment.rs` (runner cutover), effigy#232. The
`data_from` data claim and retire as a transition are later work (§9). The
workspace session run and its host authority channel are implemented at the
kernel and runner seam (052 §8.3, §8.7, §8.11); ordinary production activation
remains refused. The uninstalled #531 qualification path is described below;
native backend qualification remains outstanding.

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

The qualified invocation is still refused. Recoverable Env admission now accepts
an explicit custody plan and starts it through the recoverable workspace
sentinel path. A startup failure returns the exact cleanup-only owner, including
any partial sentinel and checkout backup, rather than settling or dropping it
behind the reservation caller. The kernel controls
`reserved_env_admission_retains_interpolation_under_its_own_custody` and
`reserved_env_admission_failure_returns_the_exact_partial_custodian` cover
reserved admission, exact descriptor custody through handle drop, verified
closure cleanup, and retained ownership on partial startup. They use real
kernel helpers in disposable fixtures; they do not qualify a native backend,
Discard, or a full workspace session.

The default production activation does not supply that custody plan or execute a
private apply. The separate `workspace-private-qualification` compile-time
feature supplies a custody plan to the guarded activation and selects the
explicit `private_apply::qualification_only` strategy. No installed CLI,
configuration or environment toggle enables it. Qualification preparation
requires an already prepared checkout identity and current source closure;
missing/stale sources and shared-service preparation refuse before effects.
The parent admits a separate reserved Env Run for preparation and runs
same-executable children with discarded streams. Each child authenticates its
actual applied process receipt, purpose-tagged intent digest, current Env fence,
held gate and live same-image parent/sentinel before effects. A nonce or
declarative intent is insufficient; a receipt for another intent or purpose
cannot authorize this preparation entry.
Input preparation and validation reuse the ordinary preparation implementation;
the parent publishes the reobserved render through its actual Env Run between
those children. The exact render digest is rechecked before and after validation.
Session access crosses the existing one-shot descriptor grant, never ambient
credential bytes. Failed preparation settles that same owner before cancelling
its reservation; Unknown retains the owner and exclusion.

The qualification spawn requires the creator image captured at process entry,
the exact live detached Env sentinel, checkout backup and anchor, and the
original activation deadline. The image observations retain the documented
macOS accidental-skew limit: path/vnode observations cannot prove a mapped
image against an adversarial ABA replacement. Missing entry capture, foreign
images, expired budgets and ordinary Discard calls refuse before a workload process
intent. Helpers and anchors are re-observed before GO. Leader exit does not
qualify apply success without a separate fresh group-closure observation. The
exact interpolation descriptors remain on the Env owner until its verified
settlement, including on unknown results and caller-handle drop.

The actual activation Env Run receives the helper plan separately from its
Session owner. Named recovery commands use the CLI's `container <NAME> <verb>`
grammar. Before gate-free integrations, qualification verifies actual workload
completion and fresh group closure, retires each exact process/anchor and closes
the backup with verified interpolation cleanup. Only then may it close its own
shared gate descriptions, without explicitly unlocking another description.
The checkout fence lease and sentinel remain on the same parked Run. Unknown
closure, helper acknowledgement, cleanup, deadline or cancellation keeps Env
held. Retirement stays inside the original absolute deadline and cancellation;
it creates no fresh cleanup window. Required apply effects remain Applied until
final settlement, preserving the existing publication predicate and real exit
obligation.


An already created interpolation file first enters an exact Run-owned prelaunch
slot with a durable, non-executable pending custody obligation. Its intent and
unreadable pending receipt commit atomically, including across a lost post-commit
result; old process readers cannot see a receipt-less row and infer non-execution.
After helper-image
checks and before any fork, the private SCM_RIGHTS channel gives the checkout
backup hold-only descriptions; caller drop retains that exact file and Env gate.
A real registered process receipt, synced binding and verified exact-object
handoff replace the hold. Never-forked refusals may clean the same live file
through no-clobber quarantine/unlink/sync before releasing its hold. Partial
fork, missing owner and unknown acknowledgement remain unresolved. The pending
envelope is not a process receipt, preparation capability or cleanup authority.

An interpolation-bound Discard receipt has an explicit combined envelope. The
new decoder accepts it; legacy readers cannot interpret it as an ordinary
process, and recovery without the live descriptor owner preserves residue.
`test:workspace:private-qualification:kernel` exercises source/fixture custody
only. The qualification CLI control uses the real front end, reserved Env
custody, apply, guest publication, commit and selected session with doubled
external backend/guest I/O; it is not native evidence. Native backend,
genuine old-binary and consumer qualification, retained-review acceptance and the final production GO remain
separate requirements. Ordinary private backend invocation and Discard remain
refused.

Qualification preparation uses read-only running/status/containerd/BuildKit
checks. It refuses a stopped or unknown backend and never starts a profile,
repairs shared runtime units or rewrites managed VM settings. The ordinary
backend ensure remains unchanged. The real CLI fixture now delivers a synthetic
vault canary through both host environment delivery and the Colima-shaped
interpolation-file route; the latter asserts the combined custody receipt,
backend file read, verified file cleanup and absent diagnostic/durable plaintext.
Neither external-I/O double is native qualification evidence.

The qualification down uses the kernel's `DownCoordination`: foreign live or
unresolved holders refuse before stop requests; only this session's children
are stopped, their completed cleanup is checked, and the selected session must
close with complete cleanup before the ordinary Env stop rechecks R4. The
serving loop observes the adopted session's own fenced stop request as well as
the outer supervisor's controls. Explicit interrupted-session recovery takes
its recorded private slot through `settle_interrupted` before Env admission;
a dead creator alone does not prove group closure or release that slot.

The nine deferred `workspace_reserved_*` publication controls now have a
qualification-feature selector that uses `private_apply::qualification_only`
with real helpers and the same activation owner/deadline. Guest/backend I/O is
doubled. The actual CLI fixture covers committed publication, guarded failures,
down/fresh-up, and a killed selected supervisor. Existing-target artifact reuse
still refuses at `observe_decision_artifact`'s descriptor-pinned probe gate;
the crash control requires unchanged selection on that refusal, followed by
explicit down and fresh up. It does not weaken that gate or claim successful
reuse. Shared-service and stale-source preparation remain refused.

The [531 full-session packet](../../qualification/workspace-private-531/README.md)
provides one aggregate source-immutable selector for separately GO-armed Colima,
Desktop and native Linux cells. Its runtime inputs and GO index are separately
hash bound to avoid a circular hash dependency. Actual native artifacts, scoped
GOs, receipts, consumer evidence, retained review acceptance and final production
GO are still required. Packet boundary tests are source evidence only.

Q-010 storage follows contract [052](052-runtime-kernel-contract.md) §3.1,
§7 and §11, approved by planner and Tom (2026-10-22: "Approve"). Environment
records, renders and holders use their checkout journal; shared endpoint and
port facts use the machine store. No legacy family is imported (Q-012 §3).
The later workflows named in §9 remain separate from
this relocation.

### Attribution and cleanup claims (effigy#901, 2026-11-11)

A frozen render may name its image by content digest (`image: sha256:<digest>`) rather
than by repository. A bare digest names content, so an engine that reports that same
immutable digest under the repository spelling it imported the image with still names
the declared content: nerdctl answers `import@sha256:<digest>` for an OCI layout it
loaded without a reference name, while the container's own `Image` field is the exact
resolved id. `ImageIdentity::observe` and the window's row validation therefore bind a
created row whose reported reference carries the declared digest, and still address the
identity by id — the container's image id must equal this backend's lookup for the
declared digest. A tag, another digest, or that spelling against a tagged render
refuses exactly as before.

Settling a failed activation's Env run releases process custody and journalled effects;
it does not remove the Compose project the apply may already have created. The failure
sentence may therefore claim "exact owner cleanup settled" only when a fresh bounded
read-only inventory of the owned project answers empty. Otherwise it reports the exact
number and names of the owned containers that remain (or that their absence is
unproven) and names the one down authority, `effigy container down`. Custody settlement
is never reported as resource removal.

Owner: environment reconciler (Queue lane `v015-environment`)
Kernel: [052](052-runtime-kernel-contract.md) §5.9, §11
Supersedes: the lease, lifecycle-marker and per-site holder-check sections of
[005](005-container-runtime-contract.md) (see [retired concepts](../retired.toml))
Related: [006](006-compose-backend-compatibility.md),
[049](049-heavy-validation-admission-contract.md), [questions](../questions.md)
Q-004, Q-005 and Q-010

### Inherited dispositions (effigy#533, 2026-11-02)

Every item #533's brief named has one recorded disposition. "In place" means
the mechanism exists in this tree and the cited proof passes at this head;
"partly proven" names exactly which cited proof does not, and "deferred" names
the owner or capability that can finish it. Nothing below claims native
qualification, Discard, or a workspace apply, and no row cites a selector that
fails without saying so.

| Item | Disposition and evidence |
| --- | --- |
| Exact original-root worker | In place. `WorkspaceOwner` carries the checkout `StateRoot` (`root: &'static StateRoot`) and the whole sequence runs under it; the reserved proofs drive that owner. |
| Host/session/Env admission once | In place and proven. The session is admitted before Env and one activation holds it: `workspace_activation::tests::reserved_owner_admits_task_before_env_and_refuses_concurrent_or_unselected_success`. |
| Consuming transition and unwind ownership | In place. `WorkspaceFinishCustody::{Activation,Session}`, `GuardedAbortCustody` and `ParkedWorkspaceActivation` hand the exact owner back on every failure; `WorkspaceUpError::{Owner,Finish}` names which one. |
| Bounded truly read-only external observation | In place. `TargetProbe`/`ServingProbe` facts and backend reads run under `observation_deadline`/`bounded_until`; the gated re-observation proofs read no selector. |
| Owner-local finalization | In place. `finish_selected` ends Env before the session is handed over; `recover_selection` completes a committed obligation; proven by the crash-after-commit and partial-switch resume proof. |
| One sealed original-root lease | In place. `Objects::seal_for_preflight`, the retained `predecessor_seal`, `SealedPreflight::matches(&closure)` and the "decision differs from its frozen plan" refusal. |
| Attempt-scoped stage, tag/claim, cancellation, competing-attempt fencing | Partly proven, one inherited failure named. `ActivationClaim::{Environment,Session}` admits only the session staged by this attempt; staging is a prediction under `Gate(owner)`; a competing activation is refused. In `test:kernel:workspace-session`, 117 of 118 pass, including `three_overlapping_sessions_keep_their_exact_liveness_and_lifecycle_fences`; `workspace_stage_crash_retry_keeps_predecessor_selectors_and_immutable_bytes` covers staging retry. The remaining failure is `every_activation_boundary_recovers_without_displacing_the_live_predecessor` at `Point::BeginAdmitted`: it settles the activation run with `settle_interrupted` from inside the process whose identity that run recorded, and `verify_reserved_owner_dead` correctly refuses to declare a live owner dead. That is the deferred second-process owner-death proof (#659 "Kernel and CLI recovery"), not a passing proof. No kernel byte differs from base in this change (`git diff --name-only ed2a21007 -- crates/` is empty), so the failure is inherited. |
| Reserved activation gate ordering/deadline | In place and now proven at the apply too. `apply_sealed_with_owner` hands the activation's own absolute deadline, and `workspace_reserved_callback_keeps_the_original_activation_deadline` asserts the strategy received exactly that deadline. Gated re-observation refuses a replaced target, another valid id, and a spent deadline before any read. |
| Reverse unwind | In place. `unwind_workspace_integrations` unwinds in reverse in the integrated sequence; a failure keeps the exact guarded or parked owner (`integrated::IntegratedFailure`). |
| Idempotent recovery | In place. `unfinished_selection`/`selection_recovered`/`selection_abandoned` with `workspace_reserved_crash_after_commit_and_each_partial_switch_resumes_under_real_env_guard` and the unknown-liveness/dead-listener proof. |
| Authenticated receipts | Guest publication receipts are authenticated (pinned TLS-only dial, `WorkspaceContextV2::verify`, canonical enrolment credential, `SelectionReceipt`/`SessionRef`). Integration effects are reconcilable, not authenticated receipts, as the slice interface records — an intent, not a claim. |
| Absent ChildGrant real startup | Proven at this head. `test:workspace:effigy-build` builds the binary, then `effigy test:focused -- -p effigy --lib --features workspace-serving-fixture --run-ignored only -E 'test(reserved_engine_admits_default_and_named_profile_under_the_reserved_id)'` passes: the real reserved engine admits default and named profiles with no session passphrase and no ChildGrant. The test stays `#[ignore]`d because it needs that built binary, not because it is unproven. |
| Checkout symlink/catalog identity | Not proven here. Source pins are frozen at reservation (`workspace_source_closure`, `PostPrepDeclaration::verify_sources`) and source refresh rechecks workspace mode, but a canonical symlink/catalog identity proof across guest routes needs the qualified private apply plus the native identity gate (052 §8.11). Deferred with them; no claim. |
| Frozen plan/manifest drift | In place. Planned-policy equality, `plan.publish_inputs` equality, the retained seal match, and `PostPrepDeclaration::verify_policy`/`verify_sources` at the supervisor boundary all refuse drift. |
| Observe status/context | Partly in place, proof deferred. The channel carries `ObserveQuery`/`ObserveBody` under `ObserveContext` and `run::status` returns `Observed<RunStatus>`; v2 context verification binds the exact session generation. The production-route exact-status/context proof needs the same apply capability as the production E2E, so it stays with #531; unproved adapters refuse by name (052 §8.6). |
| Level 1 coverage and missing adapters | Recorded and implemented as policy. Tom's 2026-11-01 ruling in 052 §8.6, `workspace_client/coverage_policy.rs` and `tests/workspace_client_coverage_policy.rs`: Level 1 forwards; every other guest request refuses with a stable unsupported diagnostic. |
| Level 2 PTY work | Not conflated. 052 §8.6/§8.11 keep attach, reconnect, terminal input and real PTY as Level 2; nothing in #533 claims them. |
| #529 helper custody | Its landed identity/ACK and anchor checks are preserved. Ordinary `Terminal::Discard` remains refused; the compile-time qualification candidate and its current limits are described above. |
| #526 interpolation cleanup | Not this task's owner and untouched. Interpolation files are already per-invocation with a lock and a sweep (`policy/render/vm.rs`); #533 neither changes nor claims them. |
| Discard refused | Unchanged. `run_private_detached_command`, `support/workspace_private.rs` and `Terminal::Discard` all still refuse; no Discard route was enabled. |
| Production workspace apply | Refused by name, as the section above records. No backend process starts. |

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
  Ordinary `container up` uses the same ordered runtime effects. Attached
  failures run lifecycle cleanup; detached failures preserve Compose and partial
  exposure; workspace failures retain the journal and exact reservation owner.
  Workspace handoff then provisions the guest and renders the transition without
  repeating gateway startup or route registration.
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

The managed owner records its environment identity and complete prelaunch host
graph on its existing run before the environment can block. The activation
operation then records phase counts and the host graph together in its checkout
journal, using the same operation through readiness and ordered host startup.
Its own gate never holds `Env` across readiness, so `down` can interrupt a child
that never binds. Activation operations for one environment are serialized by
that operation's kernel gate.

Each `managed:waits` effect contains one complete snapshot. Readers match the
exact managed parent run; an unrelated activation cannot replace its waits, and
an unobserved child graph leaves the complete parent graph visible. The phase
and host entries are separate fields, so a host named `environment` never
collides with its phase. Changed waits and live elapsed time refresh on the
owner's existing cadence; settled observations are not rewritten on each tick.
A failed prerequisite remains terminal and attributed, and no dependent starts.
Status, text and panes read these journal observations; best-effort events only
wake them. New owners write no wait directories, session files or owner pointers.

The host plan launches entries
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
Both daemon stream writers and the operator-event appender now open the selected
log without following aliases, verify a regular single-link file and applicable
ownership, secure the held descriptor to mode 0600, and verify that mode before
writing or handing the stream to the child. Existing bytes and append behavior
are preserved. This repairs only a log opened during that normal operation; it
does not sweep unopened legacy logs or repair directory and ancestor
permissions. Raw diagnostic contents remain a separate exposure limit. The fix
does not qualify native gateway behavior or release acceptance, and workspace
Phase A apply and preparation still require qualified private or discarded
output before their production path is accepted. Tom's v0.15 acceptance of the
remaining raw-output and historical-file limits is recorded in the
[release disposition](release.md#workspace-and-gateway-diagnostic-disposition-before-v015-freeze).
