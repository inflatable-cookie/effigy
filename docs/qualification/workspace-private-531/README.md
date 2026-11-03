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
There is one retained Queue reviewer and one aggregate proof execution. Source
or fixture checks do not count as native receipts.

## Subject and immutable inputs

The subject is the **exact published PR head** recorded by Queue for the proof
execution, including this driver. `launch.py` checks HEAD and a clean tracked
source/index before and after each backend. It refuses a candidate build
receipt naming another source head. The planner must bind the reviewed selector
and actual input hashes through Queue's supported proof route before requesting
execution. No selector or hash in this document is an execution approval.

The actual runtime input is a private JSON artifact below
`/Volumes/Builds/scratch/effigy/`. It is not an editable repository brief.
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

Runtime images must already contain Python 3, a GNU loader, the root user and
ordinary core utilities. The fixture has no ports, named volumes, shared
services or user data. Its utility creates a new encrypted synthetic vault and
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
native proof does not rebuild it or bless bytes with new sidecars. All ad-hoc
files, logs and source fixtures stay on removable scratch. Linux must have positively proven approved Vault/removable backing at the same
canonical scratch path. The proposed native-ext4 placement needs an explicit
accepted host/guest provenance chain; a path prefix or filesystem name alone is
insufficient. Missing placement evidence refuses, with no internal-drive fallback.

## Separate GO receipts and the one selector

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
Queue binds **both** the runtime-input and GO-index hashes. All three GOs are
validated before a native window starts. No backend subset is a passing result.

The exact selector to bind, after replacing the four arguments with reviewed
absolute input paths and actual hashes, is:

```sh
effigy proof:workspace:private-qualification -- --inputs <inputs.json> --sha256 <inputs-sha256> --go-bindings <go-index.json> --go-sha256 <go-index-sha256>
```

The single invocation executes Colima, Desktop and Linux sequentially. The
Linux driver, probe, common module, inputs and GO index must already be placed
and hash verified on the approved Linux host; the launcher verifies those hashes
over SSH before executing its reviewed runtime there. No mutable remote command
body is supplied by the input. If the supported Queue route, host mapping or
exact native GOs cannot execute all three cells, name that concrete seam to the
planner before native action. Do not spend the one proof allowance on the old
subset launcher, split off another product route, or retry a failed proof.

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
   using the genuine older executable, and the qualification pre-workload refusal after successful genuine old-helper
   startup, with verified cleanup. An unrelated admission/startup failure cannot
   count as old-binary credit. A foreign copy control is
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

The [recovery preparation gate](recovery.md) implements pre-effect refusal for the
retained agent: a Darwin exit event does not close the identity-to-signal race.
Owned-child watcher proofs and recorded admission tests grant no runtime authority.
