# Private workspace full-session qualification packet — 531, revision 1

Status: source candidate and packet prepared for the retained review. **No native
execution, consumer acceptance or production enablement is recorded here.**
The default build continues to refuse private preparation, private backend
invocation and Discard. The uninstalled qualification build is selected only by
the compile-time `workspace-private-qualification` feature. It uses actual
custody, identities, publication and TLS; its native build excludes
`workspace-serving-fixture` and `fault-injection`.

This packet replaces the positive-gate gap in the
[older subset packet](../workspace-sessions-a00394012/README.md). That packet's
candidate, receipt history and subset launcher cannot qualify these cells.
There is one retained Queue reviewer. Source or fixture checks do not count as
native receipts.

## Source delivery and deferred qualification

Tom's [2026-11-03 source-split ruling](../../knowledge/contracts/release.md#source-delivery-and-separate-proof-tasks--2026-11-03)
permits the implementation and compile-time qualification packet to merge after
current-main integration review with ordinary production Discard still refused.
effigy#620 split the packet's entry points: `launch.py --backend
colima|desktop|linux` independently executes exactly one backend under its own
scoped GO, binding the exact source, programs, contract, candidate, inputs,
pinned root, old baseline, window budget and input-hash one-use marker, and
retaining the exact original complete/failed receipt bytes as a hash-bound
export inside that authorized proof turn. Positivity is recorded only after
the post-run source check, so a mid-window source edit yields honest,
nonqualifying evidence. `--backend` is required; new all-backend launches are
retired (Tom, 2026-11-10; effigy#879). Backend receipts must still match the same final source, features and
common contract — each platform's actual artifact bytes stay bound to their
own producer receipts — and the read-only offline aggregate verifier refuses
anything less before any aggregate judgment, which additionally requires the
actual consumer gates and Tom's separate production-enable GO. The [owned
transfer
map](../../knowledge/contracts/release.md#source-delivery-and-separate-proof-tasks--2026-11-03)
assigns source controls to effigy#619, SOURCE packet/backend bindings to #620,
immutable artifact production after integrated main QA to #627, the Colima,
Desktop and Linux full-session proofs to #621/#622/#623 (each bound by both
#620 and #627), aggregate judgment to #624, actual Acow/CBS consumer
continuity to #625 and reviewed production enablement to #626. Task
registration grants no execution authority.

Historical packet/fixture success is not positive native qualification. The old
subset launcher never established full sessions. The consumed guestagent check
refused two recorded `0775` parent rows before payload open/hash; no installed
payload hash, package provenance, health or closure was established. Its recorded
leaf size differs from both pinned public references. The separate untrusted
snapshot proposal remains UNAPPLIED. No consumed allowance is reusable, and
`noAutomaticRetry` remains in force. This integration grants no new observation,
signal, cleanup, install, consumer or release authority.

## Launch-mode retirement inventory (effigy#879)

The 2026-11-11 read-only inventory found no unspent request for all-backend
launches, individual C2–C6, or quota/project administration. Scratch contained
only unarmed all-backend input examples. The retained individual requests are
`C2.request.json`, `C3.request.json` and `C4.request.json` below
`/Volumes/Stash/scratch/effigy/mac-4f6a-11a01580/earlier-exploration/explore-uxl5vie0/`:
all three activation IDs have exclusive claims and terminal stage receipts
(C2/C3 closed; C4 refused). Their archived bytes retain old paths and differ from
the original proof-request hashes; they cannot be replayed. No C5/C6 individual
or quota/project request was found. The current macOS record requires a future
chain; the Linux quota packet remains unarmed. The older unspent build/check in
[colima-setup.md](colima-setup.md) uses its separate image helper and stays intact.
Full paths and hashes are retained in
`/Volumes/Stash/scratch/effigy/879-retirement-f13c85de2183/inventory.json`.
Historical receipt readers and the independent offline aggregate remain available.

## Subject and immutable inputs

The subject is the **exact published PR head** recorded by Queue for the proof
execution, including this driver. `launch.py` checks HEAD and a clean tracked
source/index before and after each backend. It refuses a candidate build
receipt naming another source head. The planner must bind the reviewed selector
and actual input hashes through Queue's supported proof route before requesting
execution. No selector or hash in this document is an execution approval.

The actual runtime input is a private JSON artifact below the canonical
removable scratch `/Volumes/Stash/scratch/effigy/` that `common.py` binds for
future invocations. It is not an editable repository brief. The consumed 531
session's receipts and setup notes name the retired `/Volumes/Builds` mount;
those are immutable historical records, never the current binding.
`inputs.example.json` is a deliberately unarmed schema example; its placeholders
are not artifact identities or receipts. Actual binary paths and SHA-256s must
come from fresh recorded builds, not an earlier Queue cleanup record. The
reviewer must inspect the build receipts and input binding before activation.

Each backend's input pins:

- exact source head; default production binary and its feature-free build
  receipt; qualification binary and its single-feature build receipt;
- the uninstalled qualification fixture utility, kernel helper, kernel library
  tests and custody integration tests, each by absolute path and SHA-256;
- a genuine older Effigy executable from the recorded different source commit,
  its source/build receipt and binary digest. A copied current executable or a
  shell stand-in is insufficient;
- the produced Linux guest artifact, original producer receipt and active-version
  sidecar by hash, at the production producer's normal artifact location in the
  frozen Effigy source. No fabricated ELF, fake receipt, download fallback or
  replacement of the ownership/platform/artifact checks is permitted;
- the resident runtime image **ID**, engine executable bytes and exact
  Colima profile / Docker context; complete container, network and volume
  baseline inventories; an explicit bounded native budget;
- an exclusively prepared empty `qualification_root`, pinning its canonical
  absolute path, device, inode and uid. It must be a private `0700` directory
  owned by the native executor, strictly below removable scratch. Missing pins,
  aliases, changed identities and already used/nonempty roots refuse;
- Linux's actual SSH alias, private `ssh_config` and `ssh_known_hosts` path/hash
  artifacts, prepositioned reviewed driver directory, and exact
  input/GO-index paths on its native Linux host. Linux executes the candidate
  there against its local engine. A macOS candidate talking to remote Docker
  cannot substitute for native Linux custody.

The prepared root is the window's exact base, not an allocation in global
scratch. Its descriptor stays held until completion; initial layout and receipts
are created relative to it, with visible identity rechecked before native
commands. The immutable input hash and native GO cover that root binding.
Replacement is retained/refused, never silently rebound or repaired. Global
input-hash/backend one-shot markers remain outside the shared root, so moving
or deleting a fixture does not erase its replay claim.

Each fixture gives its commands a separate private `TMPDIR` inside the prepared
root. The checkout is separate from that temp
subdirectory, preserving the existing Colima known-temp-path refusal without a
skip variable. Actual VM sharing/mount identity must still pass the real native
probes. Qualification preparation uses only the existing read-only backend
status and containerd/BuildKit readiness checks; a stopped/unavailable backend
refuses instead of entering ordinary ensure's VM start or shared-unit repair.

Runtime images must already contain Python 3, a GNU loader, root for control
operations, the standard nonroot `nobody` user for workspace clients and ordinary
core utilities. The fixture has no ports, shared services or user data. Its two
empty host-state mask volumes use scoped Compose names and are retired after
the final down. Its utility creates a new encrypted synthetic vault and
sealed local unlock credential through the real vault APIs, with a known
fixture canary delivered only to Compose. It reads no user/consumer vault. The
scoped native GO must cover that isolated vault/key/credential creation too. Missing images/tools/artifacts refuse; the driver never
pulls, builds, downloads, installs, starts/stops a VM, changes shared workflows,
or changes tracked source. Source archive / build preparation, any native
builder use and Linux placement require their own scoped authorization.

The [Colima preparation plan](colima-setup.md) records the failed120-byte
socket layout, successful101-byte private VM start and subsequent nerdctl stdin
context refusal with zero builds/checks. The source-reviewed continuation keeps
the same builder and transfers only the public recipe into a fresh private guest
context, with actual hash/owner/readback, engine/mount checks, one-shot stages and
the previously bound1058-second ceiling. The subsequent health-admission attempt
refused because the private profile reported not running, before any transfer,
build or check. Its recorded completion consumed331.704124seconds; neither the
old ceiling nor its observed remainder is a new allowance. No extra start or
budget reset is authorized; final accounting and operator disposition are pending.
Drive-remount history requires fresh actual health/identity admission; prior
metadata is not current VM health. The runtime recipe remains unbuilt.
The [Linux proposal](linux-setup.md) distinguishes canonical path spelling from
native filesystem and Vault-backing provenance; it prefers native ext4 only after
an explicit accepted chain, without assuming shared-folder custody semantics.
Linux SSH uses a pinned private `-F` config and pinned public known-hosts artifact;
no global config edit, alternate transport or credential sharing is introduced.
The [Linux 4f6a5ed3 preparation kit](linux-4f6a5ed3-preparation.md) plans the
Stash-hosted Ubuntu VM's Stage1 read-only probe, Stage2 setup and Recipe E route.
It is preparation only; it arms nothing.
The Linux setup adapter (`linux_setup.py`, `linux_supervisor.py`, `linux-setup-probe.sh`,
`linux-setup-stages.json`, `linux_transfer.py`, `linux_receipts.py`,
`linux_stage1.py`, `linux_observation_caller.py`, `linux_disk_identity.py`, `linux_disk_association.py`) holds the v2 probe parser, the documented Parallels writable-top selection, Stage1 request, historical setup readers and the guest supervisor. The quota
administration and project-assignment effect roles are retired; physical
preparation is the current Linux route. Exact root requests and nonces are one-use;
Stage1 requires a separately signed fresh occurrence with retained post-state.
The actual observation caller binds VM identity and SSH terminal observations.
It is source-only: `test:qualification:linux-setup`
checks it with injected transports and performs no VM, SSH, account or systemd effect.
The [macOS 4f6a5ed3 preparation](macos-4f6a5ed3-preparation.md) plans the fresh
Colima and Docker Desktop stages as a staged manifest, each needing its own GO.
Manifest r3 corrects the r2 Colima flags, route order, deadlines, caps and cleanup
authority. It records only local metadata and gives no proof credit.
The source-only executor for those stages is `macos_stage_executor.py`; see the
preparation doc's Executor section. Nothing in it is armed.
For current C2–C6 preparation, the [one-request Colima chain and unsealed native
smoke selector](macos-4f6a5ed3-preparation.md#current-route-effigy830) keep each
stage's original budgets, claims and receipts; exploratory results give no proof credit.

Preparation and proof requests are different admissions. Assemble one concrete
preparation request listing each unresolved backend fact, exact permitted probe
or effect, owner authority, remaining numeric bound and retained-resource
disposition. Unknown inputs stay explicitly missing; do not manufacture runtime
inputs or request proof from partial preparation receipts. Separately authorized
preparation may establish those facts without spending the aggregate proof.
Only after actual backend inputs and candidate/source bindings are complete may
the planner bind the three scoped native GOs and single aggregate invocation.
An unavailable runtime stops its affected step and retains its receipt; a new
request or evidence-only review does not authorize replay. The task-budget choice
to change approach preserves this ordering and grants no budget extension,
runtime activation, cleanup, consumer action or production-enable authority.

Preparation uses the existing Effigy build surfaces:
`build:workspace:private-qualification` builds the uninstalled candidate/utility;
`build:workspace:private-qualification:test-inputs` lists the actual compiled
library and custody test executable paths without running them;
`test:workspace:private-qualification:kernel` builds the same helper.
The ordinary binary can be built through `build:runtime-storage` and frozen
before the qualification build. Export Cargo metadata through
`build:workspace:private-qualification:cargo-metadata`; freeze it and the
binaries metadata with the pinned test executables. If binaries are copied,
update only those exact binary paths in the derived metadata and record its
hash. Pin the nextest executable and `tests/config/gateway-nextest.toml` too.
Runtime uses prebuilt nextest metadata, so no Cargo build runs in the native
proof. The repository's 180-second test timeout, 2-second forced-exit grace,
30-second leak failure and 1800-second global timeout remain enforced.
Use the older source's own covering Effigy
build selector for its genuine old executable, in an explicitly authorized
fresh scratch source fixture, without adding a worktree or switching this task
branch. Record command exits, exact source/tree identity, features and byte
hashes; freeze copies before another build changes a managed output link.
Do not infer that a managed output remains after its lease or cleanup.

The guest artifact uses the existing `workspace:linux:artifact` producer under
its separate scoped builder GO. Its normal receipt must be preserved. The
native proof does not rebuild it or bless bytes with new sidecars. The Darwin
Colima exact-bound source entry is `linux_recipe_e_proof.py`
(`test:qualification:linux-recipe-e`); it does not run the producer. Bound
inputs are rechecked after the producer; kit §6.3 host writes ≤5 GiB are carried
per writer, and the producer's published `effigy.builder-write-boundary.v1` record
is bound field by field before the claim (cargo intermediates are the builder's
own container-local tree, VM-side and disclosed, never counted as a host write).
All ad-hoc
files, logs and source fixtures stay on removable scratch. Linux must have positively proven approved Vault/removable backing at the same
canonical scratch path. The proposed native-ext4 placement needs an explicit
accepted host/guest provenance chain; a path prefix or filesystem name alone is
insufficient. Missing placement evidence refuses, with no internal-drive fallback.

## Separate GO receipts and the entry selectors

First freeze the runtime inputs and obtain their SHA-256. Each native GO is then
recorded as `effigy.workspace-private-native-go.v1`, naming Tom's actual decision
ID, kind `native`, this exact source head, input hash, backend, engine identity,
expiry, native budget and effects `workspace-private-531-full-session-r1`.
A release/consumer GO or a relay without Tom's scoped decision cannot stand in.
The planner validates the actual decision provenance; a JSON file alone is not
operator authority.

A separate GO index maps `colima`, `desktop`, `linux` to each receipt's
`{ "path": "<absolute receipt>", "sha256": "<actual receipt hash>" }`.
Keeping this index separate avoids a circular input-hash/GO-hash dependency.
Queue binds **both** the runtime-input and GO-index hashes. `--backend` is
required: each invocation validates exactly the selected backend's GO and runs
that backend alone. No backend subset is a passing aggregate result.

Bind one backend cell of the frozen inputs with the selector below, replacing
the paths, hashes and backend with their reviewed values:

```sh
effigy proof:workspace:private-qualification -- --inputs <inputs.json> --sha256 <inputs-sha256> --go-bindings <go-index.json> --go-sha256 <go-index-sha256> --backend <colima|desktop|linux>
```

It validates exactly the selected backend's scoped GO, consumes that backend's
input-hash one-use marker, runs only that backend's full session, and records
the original complete/failed receipt with its binding record in a fresh
evidence directory, retaining the exact original receipt bytes through a
quoted, size-bounded, hash-bound export inside the proof turn. Positivity is
recorded only after the post-run
source-unchanged check: a mid-window source edit refuses and yields honest,
nonqualifying evidence. One entry is one positive native receipt: it is never
an aggregate result and never writes an aggregate `complete.json`. Separate
backend entries may bind different inputs and GO receipts; the common
candidate contract must still compare exactly.

For Linux, the driver, probe, common module, inputs and GO index must already
be placed and hash verified on the approved Linux host; the native runner
verifies the bound inputs before executing the reviewed runtime there. No
mutable remote command body is supplied by the input. If the supported Queue
route, host mapping or scoped GO cannot execute the selected backend, name that
concrete seam to the planner before native action. Do not spend the allowance
on the old subset launcher or retry a failed proof.

After #621/#622/#623, effigy#624's independent judgment runs the read-only
offline aggregate verifier over the three invocation receipts:

```sh
effigy verify:workspace:private-qualification:aggregate -- --source-head <head> --receipt colima=<receipt> --receipt desktop=<receipt> --receipt linux=<receipt> --receipt-sha256 colima=<hash> --receipt-sha256 desktop=<hash> --receipt-sha256 linux=<hash>
```

It performs no native action and writes nothing: no SSH, no runtime, no
qualification-root reopen, no backend or VM observation. It reads and hashes
the retained original-receipt exports locally, refuses missing, failed,
source-changed, foreign, stale, mixed-source, mixed-candidate or incomplete
receipts, and validates the fixed full-session cell contract of each export
(native, production-disabled, the production-refusal through restored-baseline
cell sequence, each cell's recorded positive facts, and the recorded
baseline/cleanup/fixture-retention equality) — a
caller bool alone is never authority. The shared contract compares the same
final source, features and common contract; each platform's actual artifact
bytes stay bound to their own producer receipts. Its verdict still requires
the actual consumer evidence and Tom's separate release-kind production-enable
GO. The deterministic producer recipes and the exact receipt/report formats
that effigy#627 fills are fixed in [producer.md](producer.md).

The native budget must be within existing spending authority and at most the
1800-second test-runner window per backend. This cap is not a spending grant.
Work commands use the smaller of their 60-second phase bound and the remaining
window; the real product's operation/cleanup deadlines remain enforced.

## Cells and effects per backend

`runtime.py` claims the backend's exact prepared root, which must be empty. The
utility calls the real checkout-token producer. The fixture uses an ephemeral
Compose identity, a single resident-image primary, and a bind of its own new
checkout. It never links another checkout or uses a sibling path. The sequence
contains three synthetic container lifetimes, four up attempts (one must
refuse), three explicit downs, and one exact supervisor-kill fault. Every native
container is resolved from that checkout's exact scope and service labels.

1. **Ordinary refusal and baseline.** Compare complete engine inventories to
   the pinned baseline; require ordinary production preparation refusal without
   engine inventory changes. A drift is a refusal, not a rebaseline authority.
2. **Positive full session.** Actual qualification CLI up must commit selection;
   its actual live session and Env generation must match the one full selected
   v2 document read from the native guest. Match exact engine container ID and
   pinned guest artifact hash. Require actual Compose canary delivery, the
   combined interpolation/Discard receipt for Colima (host Docker uses its
   secret environment without inventing a file), and no plaintext canary in durable
   host state after commit. No v1 projection or synthetic session is accepted.
3. **Native identity, TLS and credentials.** Execute the actual guest identity
   probe, match target/boot, perform the real pinned TLS dial and require a wrong
   pin to fail. The actual guest client enrols for a host task. A different new
   client with a corrupt enrolment credential must refuse before task execution;
   restore the fixture's original credential bytes without retaining them in
   receipts, argv or ambient environment. Scan host fixture files for leakage.
4. **Level 1 streams and real outcomes.** Invoke declared host tasks from the
   actual guest. Assert stdin EOF/no PTY in the host task, incremental distinct
   stdout/stderr before exit, final output, and the real journalled exit 23 with
   completed cleanup. A separate self-TERM task must record its real signal.
   After verifying its `/proc` birth, kill the actual guest protocol client
   started by this driver for a running host task and require actual host-owner
   cancellation/settlement. The stream task waits for the driver's acknowledgement
   after both first frames, so buffered completion cannot count as streaming. For rotation, stop the exact
   guest client after verifying its `/proc` birth, emit enough real host output
   to rotate capture, then resume it. Require gap and over-limit-line truncation
   to report incomplete/Unknown instead of a complete result.
5. **Retained kernel controls and genuine old helpers.** Run the prebuilt real
   descriptor handoff/caller-drop/owner-death controls, helper fault coverage and
   custody kill matrix under this native host. Run ordinary old-sentinel refusal
   using the genuine older executable: the old helper must be admitted, then the
   current parent's Discard must refuse before process intent, with verified
   cleanup. Run the qualification pre-workload refusal after successful genuine
   old-helper startup, with verified cleanup. An unrelated admission/startup
   failure cannot count as old-binary credit. Neither control exercises the
   legacy discard decoder; that is a separate unit test (see
   `producer.md`, Recipe D). A foreign copy control is
   additional negative coverage, not old-binary credit. Actual helper death,
   uncertain closure and partial startup must preserve the landed fail-closed
   predicates. The packet never changes the ratchet baseline.
6. **Crash and explicit recovery.** Resolve the selected session's actual
   recorded outer supervisor and full boot/PID/birth identity. The fixture
   utility uses the kernel's verified-signal boundary for the exact fault. A
   subsequent existing-target up must refuse without changing selection: the
   descriptor-pinned existing-target artifact-probe gate is still closed. Then
   explicitly recover/retire via CLI down and commit a fresh session via up.
   A kill is never itself closure evidence.
7. **CLI down, fresh up and cleanup.** Down must acknowledge only after the
   actual selected session is closed with complete cleanup and its container is
   absent. Fresh up must commit a different session at a larger Env generation;
   final down must restore the exact engine baseline and leave every fixture
   run closed with complete cleanup. Missing/unknown evidence fails the packet.

Each backend first claims an exclusive input-hash-bound one-shot marker, including when invoked directly. Each cell writes an exclusive, fsynced bounded receipt. A failure keeps all
prior receipts and exact unresolved resources; there is no retry, pattern kill,
forced cleanup, guessed absence or invented success. A passing run retains its
private fixture and receipts as evidence under its explicit GO disposition;
there is no tree deletion in the launcher. Fixture removal/retention must be
covered by that GO and a verified exact manifest, not inferred from this source
or from Queue's older cleanup records. No process or runtime may be described
as absent just because an output directory is missing.

## Source/fixture evidence and remaining release gates

`test:qualification:workspace-private` checks input/GO/source-hash/subset/symlink
and exclusive-receipt refusal boundaries without native actions. It also checks
the host-only setup socket admission: observed long path, exclusive byte limit,
canonical path/profile and symlink refusal, without creating setup resources. Its fixtures
are freshly created and removed under removable scratch. The nine named
deferred publication controls have a maintained selector,
`test:workspace:private-qualification:publication`, using the actual qualified
apply/custody and doubled external guest/backend I/O. The CLI fixture selector
also covers committed publication, preparation/intent failures, guarded
publication failure/late revalidation/handoff recovery, actual down/fresh-up,
and exact supervisor crash followed by refusing reuse and explicit down/fresh-up.
These prove source/fixture behavior, not native backend or consumer acceptance.

### Per-case source evidence (effigy#619, 2026-11-03)

Both maintained selectors pass at the reconciled head: `test:workspace:private-qualification:publication`
(9/9) and `test:workspace:private-qualification:cli` (13/13), exit 0, source head
`90b8299035913230850bfd414184344620386647`, recorded by effigy#619's run. These
are source receipts; they are not native evidence. Each named case pins the
following exact source and controls:

| Case | Exact source | What it pins | Controls asserted |
| --- | --- | --- | --- |
| Expired publication deadline refuses without guest I/O | `workspace_reserved_expired_publication_deadline_refuses_without_guest_io`, `src/runner/workspace_command/tests/reserved_guest.rs` (`Fault::ExpiredDeadline`) | An already-past absolute deadline is refused, never replaced by a fresh budget | error names the expired deadline; zero credentials, zero guest effects, no probe; predecessor selectors unchanged; no `unfinished_selection` row a successor could settle |
| Every guest write boundary crash preserves authority | `workspace_reserved_every_guest_write_boundary_crash_before_after_preserves_authority` (`Fault::Io(index, after)` over every enumerated effect, before and after I/O) | Each staging/selection effect interrupts cleanly; authority never leaves the owner | partial effects cannot return success; binary/entry selectors stay predecessor or complete-object; context stays `3`/`4`; an interrupted committed switch is settled by bounded abandonment, never left Pending |
| Gated re-observation of a replaced target refuses | `workspace_reserved_gated_reobservation_of_a_replaced_target_refuses` (`Fault::GatedChangedTarget`) | Same-shaped target with a changed boot id refuses at the gated read | refusal names the preflight/gated identity mismatch; exactly one gated read; no credential; no artifact staged |
| Callback keeps the original activation deadline | `workspace_reserved_callback_keeps_the_original_activation_deadline` (`Fault::LongObservationBudget`, `recording_backend`) | A 24-hour producer request cannot leak: reobserve and apply both get exactly the activation's own absolute deadline | reobserve deadline is at or before the activation's deadline and in the future; the apply receives that same instant, so a fresh or self-measured budget fails |
| Gated re-observation past the deadline refuses without reading | `workspace_reserved_gated_reobservation_past_the_deadline_refuses_without_reading` (`Fault::GatedExpiredDeadline`) | A spent absolute deadline refuses before the gated read runs | zero gated reads; error names the deadline; no credential |
| Gated re-observation of another valid id refuses | `workspace_reserved_gated_reobservation_of_another_valid_id_refuses` (`Fault::GatedIdMismatch`) | An individually valid foreign full id is not an acceptable observation | refusal as a replaced target; one gated read; no credential; no staging |
| Owner publishes one full v2 through real commit and selection | `workspace_reserved_owner_publishes_one_full_v2_through_real_commit_and_selection` (`Fault::None`) | The positive path: one real frozen Compose apply, real commit/CAS, selector switch, selected session | exactly one `up -d` with `--context fixture-context compose`, `--build --remove-orphans` and the compose files; one credential mint at the generation gate; one context probe before any switch; the committed v2 document names the selected run/generation, endpoint/pin equality, 0600 credential; independent `verify` passes and stale generations refuse; cleanup settles the owner |
| Owner refuses identity/alias/peer/pin/publication without selector switch | `workspace_reserved_owner_refuses_identity_alias_peer_pin_and_publication_without_selector_switch` (ten faults: identity/Unknown, alias, peer, pin, publication, context probe/peer/pin, stale document) | Every foreign or Unknown observation at the identity, channel and document boundaries refuses before the selector switch | context/binary/entry selectors stay at predecessor values; minted credentials only where the fault sits past the credential boundary; staged-but-unswitched bytes never become a publication |
| Revalidation re-observes and refuses a changed target | `workspace_reserved_revalidation_reobserves_and_refuses_changed_target` (`Fault::Revalidate`, `Fault::RevalidateChanged`) | Explicit revalidation re-reads through the same gate; a changed target refuses after the second observation | positive revalidation selects; the changed-target run refuses with two reads and no credential |
| Real CLI reaches committed selection, ownership and cleanup | `actual_cli_fake_backend_reaches_committed_workspace`, `tests/workspace_client/container_up_production.rs` | The real `effigy container up --system dev --detach --json` front end through real preparation, Env/helper/file custody and doubled engine/guest I/O | exactly one actual apply; one committed reservation; one durable switched obligation naming the live selected run (phase not closed); one complete v2 publication naming that run; fixture settled; no durable error bytes; no forced cleanup |

The remaining selector cells are the failure/foreign/Unknown controls around
that positive route: preparation failure discards backend diagnostics, forged
receipts refuse the preparation entry, preparation never starts or repairs a
backend, handoff/phase-B/late-revalidation failures unwind outside Env and
retain Unknown, down/fresh-up and crash recovery refuse reuse then recover, and
secret interpolation is owned through commit and never reaches diagnostics or
durable state. The kernel custody fixture's `old` mode still requires the
native packet's genuine older executable and refuses otherwise.

Existing-target artifact reuse remains refused by the unchanged
`observe_decision_artifact` descriptor-pin gate. Shared-service preparation and
missing/stale source closures also refuse. These limitations are not native
receipts or a waiver; the retained reviewer must judge the safe fresh-session
route and each explicit disposition. Any broader production switch is a later
reviewed delta, never this compile-time feature becoming a normal CLI toggle.

Consumer verification remains a separate owner-authorized real Acow/CBS
execution with its own evidence. Ordinary-Compose synthetic #538 and this
isolated workspace fixture cannot count for it. The merge hold remains until
all required native receipts, consumer evidence as applicable, retained-review
acceptance and Tom's separate final production-enable GO are recorded. This
packet is neither that final GO nor a release, installation or merge approval.

## Backend compatibility and evidence limits

Network inventory lists names and obtains full engine IDs through inspect,
without `network ls --no-trunc`, which [nerdctl does not implement](https://github.com/containerd/nerdctl/blob/v2.3.1/docs/command-reference.md).
Its [network inspect model](https://github.com/containerd/nerdctl/blob/v2.3.1/pkg/inspecttypes/dockercompat/dockercompat.go)
allows an absent engine ID for built-in CNI networks. That absence is preserved
as null beside a digest of the observed stable configuration; it never becomes
an invented ID, network absence or cleanup authority. Unrelated baseline
networks are never mutation targets. Container identities remain full 64-digit
engine IDs. Down verifies absence through a successful fresh full inventory;
an inspect/transport error cannot stand in for absence.

Prebuilt test execution uses nextest's supported
[binaries/Cargo metadata route](https://nexte.st/docs/ci-features/archiving/).
Freeze its required non-test binaries, build-script outputs and linked-library
closure as well as the two test binaries; derive metadata paths only to those
frozen copies and record the derived metadata hashes. Do not rely on a released
Oxide slot or surviving symlink. The fixture helper override is pinned explicitly.
The source preparation and its output provenance remain review inputs; their
mere presence is not native acceptance.

The CLI fixture selector serializes these process-heavy controls. This keeps the
late-revalidation fault inside its original two-second integration budget;
there is no clock override, deadline extension or ratchet baseline change.

## Prelaunch interpolation ownership

The exact file enters a Run-owned slot before fallible qualification checks.
A durable `kernel:process` obligation has an explicit
`interpolation_prelaunch_v1` purpose and an unreadable-to-old-process-readers
`interpolation_pending_v1` envelope; this is **not** a process receipt or GO.
Intent and envelope commit atomically. A lost post-commit result cannot expose a
receipt-less row that an old reader could misclassify as an unopened barrier.
The extra obligation records an already created file, including on an early
image/deadline refusal. It cannot authenticate a preparation child.

After the actual same-image helper checks, but before any fork, the existing
private SCM_RIGHTS channel transfers hold-only descriptions to the actual
checkout backup. The helper holds that exact file and Env gate across caller
drop. The hold grants neither execution nor cleanup authority. Only an actual
registered process receipt, synced binding and exact-object descriptor handoff
replace it with normal bound-file custody. A partial fork or lost ACK retains
the live owner and fails closed; it never promotes missing receipt to absence.

A never-forked local owner may perform identity-checked no-clobber quarantine,
unlink and directory sync. Only then may the helper release its hold. The live
phase permits a failed sync to retry the same positively performed unlink;
recovery after owner death never infers that unlink from a missing pathname.
The prelaunch marker without its exact live owner remains unresolved. The
caller-drop source control verifies the resident descriptor through its shared
file lock after all caller descriptions close, and verifies Env remains held.
Its exact-helper fixture teardown is not production settlement credit.

The [administrative recovery route](recovery.md) stays unarmed until a separate
Queue-bound operator GO explicitly accepts one numeric-PID SIGINT and its residual
PID-reuse risk. It never claims race-free signaling or closure from an exit event.
Owned-child proofs and recorded admissions grant no runtime authority.
