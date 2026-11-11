# macOS 4f6a5ed3 preparation: Colima and Desktop stages

## Current route (effigy#830)

The C2–C6 sealed preparation is one chain request, consumed in one owner-launched
proof occurrence. It uses the existing C1 receipt; every stage still gets its own
launch-time activation, exclusive claim, unchanged budget/reserve/caps and original
receipt. A failed or unknown stage stops all successors. The chain verifies each
receipt before continuing, holds the original root identities throughout, and
refuses source or machine-input drift between stages. Desktop windows are unchanged.
The older revision notes below describe the historical preparation sequence.

Runtime input prefetch reuses HTTPS connections only within its single bounded
helper. Every payload still has its original length/hash checks and exclusive
file; unread responses and redirects discard their connection. All connections
close on success or refusal. This avoids a separate TLS handshake for each deb
without threads, retries, or a larger C4/D2 download window.

Prepare with `qualification:macos:proof-only -- plan-chain BINDINGS OPTIONS`.
OPTIONS contains only `activation_id` (at most 60 characters), `evidence_root` and
`prior_receipt` (the original C1 stage/path/sha256). Freeze the resulting request
read-only and bind its SHA-256 to the later owner-launched proof occurrence.
Execution uses `qualification:macos:chain-proof-only -- REQUEST SHA256`.
No individual C2–C6 packet preparation is needed. The chain's total window is the
sum of its stage windows; each stage retains its smaller deadline and reserve.

`qualification:macos:colima-smoke` is deliberately **unsealed**. It requires an
existing canonical TMPDIR below Stash scratch, creates a fresh token/control root
and private claims/activation/evidence directories, runs C0/C1 and the same C2–C6
chain, and stops only its own isolated profile in a finally block. It retains
all evidence (also copied into the owned control root as `exploration-evidence`)
and the stopped VM. The retained copies preserve original bytes and original
run-directory references; they are exploratory archives, never sealed predecessors. `exploration.json` explicitly records `proof: false`.
It consumes no Nucleus proof-only occurrence and supplies no #621 credit.
Colima's HOME and all three temp variables point inside the owned control root;
Lima's temporary forward-socket shape participates in the socket-length check.
Fresh C1 starts pass `--port-forwarder none` and create an exclusive, fixed
`_lima/_config/override.yaml` with an ignore rule for TCP/UDP ports 1–65535.
Colima 0.10.3 alone emits an ignore rule for the wildcard guest address, which
Lima treats as an exact wildcard-address match unless `guestIPMustBeZero: false`
is set. Loopback guest DNS otherwise reaches Lima's hardcoded `/tmp`
pseudoloopback sockets despite TMPDIR.
The override is hashed into the start child's inputs. Colima SSH and the runtime's
explicit Unix-socket forwarding remain available; automatic host TCP/UDP
publication is unnecessary for these stages. Existing sealed C1 resources are
never modified by this fresh-profile setup.

BuildKit's Colima iidfile names the manifest digest. Its authenticated by-digest
inspection must match that exact digest in RepoDigests before both immutable identities
are bound to the disposable check: run by manifest, inspect by manifest or config. The chain verifier re-derives this mapping
from both original streams. Nerdctl container inspection may report a mutable
image display name; identity instead requires the authenticated by-digest create
command, its exclusive cidfile and the exact inspected container ID. A conflicting
immutable digest refuses. No display-name allowlist grants identity. Desktop keeps its direct config-ID comparison.

After its direct child exits, the guest helper allows at most two seconds for
adopted descendants to settle and reaps only its own kernel children. Remaining
live descendants or unavailable observations stay unknown, with remaining PIDs
recorded. A process disappearing during `/proc` enumeration is treated as exited;
malformed or unreadable observations still refuse closure.

Sealed-run plan: after review and merge, the planner freezes the request against the
exact integrated program/tool hashes and the existing q4 C1 receipt under
`mac-4f6a-b0544c12`, checks the C4 destination is absent, then launches one C2–C6
proof occurrence. Exploration never touches that root. After the candidate freeze,
#621 separately binds the Recipe A–E artifacts, baseline, guest artifact provenance,
qualification-root identity and scoped native GO described in [producer.md](producer.md)
and [README.md](README.md), then runs the single-backend full-session entry. The
stage-chain receipt is a prerequisite, not a full-session result. The full-session
freeze also pins a Python interpreter whose scratch read-only SQLite probe passes
and the genuinely produced guest host-identity sidecar; a missing binding is a
preparation failure, never a sidecar to manufacture during the proof. Before freezing
that full-session baseline, its preparation owner must initialize the profile's
container networking on owned resources and prove the temporary container is
removed. A cold Colima engine lists only intrinsic network modes; its first
container creates a persistent default bridge. The full-session window neither
rebaselines nor deletes that engine network to disguise the change.

`qualification:macos:colima-session-smoke -- --request REQUEST --sha256 SHA256`
runs the complete local Colima cell driver without consuming a sealed occurrence.
The hash-bound request records `control`, its observed `identity`, the actual
`exploration_authority` and one `cell` with real, pinned artifacts and inventory.
It requires an already running owned profile at `control/c`, Docker config at
`control/docker-config`, and an empty qualification root under `control/s`.
Before creating the session checkout, it requires a real copied `colima` on
PATH matching the pinned executable, then checks that the guest can write the
owned `control/s` share and read back a fresh host probe. The probe is retained
as evidence. C0–C6 starts with `--mount none`; that profile must be stopped and
prepared with its owned session share before a full session. These checks refuse
with the required preparation step instead of reaching activation.
Nerdctl intrinsic network modes carry no object ID and cannot be inspected;
their complete structured listing participates in the baseline. Every network
with an ID still requires inspection, and duplicate names refuse.
The detached host supervisor carries the declaration's explicit Colima/cache,
Docker-config and temp paths plus its supported artifact-source choice, so its
reobservation reaches the same owned namespace after the kernel clears ambient
environment. Those values are compared again with the prepared declaration.
The fixture uses a project catalog fragment and a bare `PROBE_TOKEN` environment
key, so current generated Compose can consume the same-owner interpolation
environment without importing a legacy `${PROBE_TOKEN}` Compose file.
The workspace client runs as `nobody` with an owned `/home/nobody`; root is only
the fixture's control exec identity. Ordinary permission preparation refuses a
root workspace identity. Colima file staging streams unshared host bytes over
stdin into an exclusive guest staging file. Optional guest reads return explicit
presence framing: Colima collapses nonzero guest exits to host exit 1, so an exit
code cannot supply absence evidence.
Forwarded host shell probes use the dispatcher's original reserved run through
the detached owner. They inherit its nonterminal capture descriptors and stdin
EOF. Admission is acknowledged after both durable stream projections exist;
each projection is redacted again after the merged sanitize/redact pass. Signal
exits keep ordinary shell outcome classification. Standard sequences, container
placement and schema `exec()` retain their pre-fallback refusal boundaries.
Before creating the session checkout, the unsealed entry checks that its Python
can read an actual scratch SQLite database with a live WAL writer and that writes refuse.
On this host, system Python 3.9.6/SQLite 3.51.0 failed that read-only open;
Homebrew Python 3.11.17/SQLite 3.53.4 passed. Select a capable interpreter on
PATH and retain its canonical path/hash; do not bypass locking with
`immutable=1`. All backend session reads use a canonical `mode=ro` URI with
WAL/SHM siblings visible and query-only enforcement. A live corruption report
gets one fresh read-only connection and a backup-API snapshot in private memory;
only an integrity-checked snapshot can recover the query. Recovery is explicitly
reported on stderr. A failed snapshot recheck retains the live error separately,
so an inconclusive observation cannot be presented as proven corruption. No
journal or WAL bytes are copied by hand or written by the reader. The full-session baseline also binds the producer's real
`.install-identity` sidecar, alongside its binary, receipt and active version.
The fixture's private HOME explicitly selects its backend/profile for retirement.
After the final down, it retires only that synthetic checkout's owned data before
requiring the original inventory. A failed exploration retains the fixture and
its native resources for diagnosis; closed owner journals alone do not prove the
baseline was restored. Down and retirement must finish on that owned checkout
before comparing inventories. Workspace state masks carry checkout/scope
labels through compaction and keep Compose's project-prefixed physical names
for nerdctl's label-limited volumes. Unreferenced original declarations are
removed, so retirement can identify the two masks after the container disappears.
The 1800-second cell bound remains. Exploratory activation/down/retirement
commands may spend at most 300 seconds each, clipped to that original window;
other commands keep 60 seconds. The exploratory receipt records these limits.
The sealed driver retains its reviewed 60-second command bound. A debug host
activation exceeded that bound after genuine publication reached a pending
selection, so the later candidate freeze must measure the optimized artifacts
against it or explicitly review a new phase bound before sealing. This task
neither changes the sealed bound nor claims that timing proof.
The full-session gap control throttles the real client's stdout inside the guest,
before engine/SSH buffering, while leaving channel polling live. Its fixture utility
reads typed retained extents without output payloads; actual expiry and truncation
must accompany client refusal and complete task settlement. Stopping the entire
client coupled log rotation to the five-second TLS idle timeout, so that obsolete
probe is removed. Capture coalesces at most 64 KiB for up to five milliseconds;
retention and transport limits stay unchanged. After successful publication,
the adopted session owns an open serving operation with the same cancellation,
limits and capability; each request takes a fresh ten-second phase. The finite
publication deadline remains enforced at handoff, and fixed-context channel
callers keep their original deadlines.
Its result is `exploration-complete.json`
with `exploratory: true, proof: false`; aggregate verification refuses it even
if renamed. This entry grants no VM startup or artifact-build authority.

## Native exploration, 2026-11-09

The unsealed smoke closed C0–C6 on the owned control root
`/Volumes/Stash/scratch/effigy/mac-4f6a-11a01580`, including the C4 offline build,
C5 runtime facts, exact-container removal and C6 observation. Its profile then
stopped with waited exit 0. The host log contains no automatic TCP forwarding or
`/tmp/lima-*` pseudoloopback sockets. The longest planned socket path (91 bytes)
was also bound successfully on this host before the start.

Original receipts and streams are archived under that control root's
`exploration-evidence`; earlier exploratory failures are copied under
`earlier-exploration`. This is native smoke evidence only. The #621 full-session
exploration is recorded below. Candidate freeze and sealed C2–C6/full-session
occurrences remain separate work; no #621 qualification is claimed.

## Full-session exploration, 2026-11-10

The complete unsealed Colima cell passed on the same owned profile. Its receipt is
`/Volumes/Stash/scratch/effigy/mac-4f6a-11a01580/s/session-30/exploration-complete.json`.
It records actual v2 publication and private interpolation, guest identity and TLS
pin refusal, distinct incremental streams, stdin EOF/no PTY, exit 23, signal 15,
credential refusal, caller-loss settlement, retained-output expiry and truncation,
the genuine older-helper controls, supervisor crash/reuse refusal, three closed
sessions, retirement and the original native inventory. Each forwarded fixture task was
admitted once. The receipt explicitly carries `exploratory: true, proof: false`.

The optimized host artifacts, genuine produced Linux guest, exact source patches,
request and per-cell observations are retained under that control root. The host
uses the integrated worker changes over the planning main; the guest uses that
main plus the recorded identity/version/authentication fixes. These exploratory
roles do not substitute for a coherent reviewed-head Recipe A–E freeze. The later
sealed plan above still applies, including its independent timing check.
Both owned Colima profiles then reported `Stopped`, with waited stop exit 0 for
the session profile. A separate audit of 580 recorded process identities across
42 retained journals found no matching live owned process. Earlier failed
explorations remain retained as failure evidence, including unqualified startup
records; they are not relabelled as settled qualification.

### Fresh-profile check (effigy#849)

At `b502fc823`, the full unsealed entry passed on the fresh owned profile under
`/Volumes/Stash/scratch/effigy/3328a540/QG5UJX`, started with its `s` share.
The host and kernel proof inputs were rebuilt; the genuine #830 guest outputs,
sidecars and original producer freshness anchor remained hash-bound. The receipt
is `s/session-3/exploration-complete.json` below that root. Bind/reacquire,
publication, client controls, kernel custody, crash recovery, fresh sessions and
retirement passed, with the original native inventory restored. The profile was
then stopped. No runtime regression or cleanup change was needed; this remains
exploration, never sealed qualification credit.

## Historical revision notes

Preparation only. It arms nothing and gives no proof credit. The executable plan is
[macos-4f6a5ed3-stages.json](macos-4f6a5ed3-stages.json), manifest revision r9 (r9 section below). Each stage
needs its own fresh GO under standing decision `63c338e2-1c50-435e-9783-e78e9d8558e6` v2,
naming the stage, the manifest, source and input hashes, the engine identity and a window
of at most 1200 seconds. The standing decision is not a blanket authority, and no GO is
recorded here.

## Revision r9 (supersedes r8 for the C4 runtime-input transfer; manifest `macos-4f6a5ed3-stages-r9`)

C4 is admitted. The missing path r8 named — a closed transfer of the host-fetched OCI/deb set into the guest —
is now one fixed finite representation in [guest_inputs.py](guest_inputs.py):

```
EFFIGY-COLIMA-INPUTS-1\n
HEADER\n <compact JSON: entries [{path, bytes, sha256}], total_bytes>
DATA <bytes>\n <exactly that many payload bytes>     (once per entry, header order)
END\n
```

The paths are only the fixed layout's names (`oci/oci-layout`, `oci/index.json`,
`oci/index.upstream.json`, `oci/blobs/sha256/<64-hex>`, `debs/<name>_<version>_<arm64|all>.deb`), the sizes and
sha256s are the reviewed pins, and `oci/index.json` plus `oci/oci-layout` are re-derived from the received
upstream index rather than trusted. The host writes it from the pinned prefetch, re-reading and re-hashing every
payload by descriptor while copying. The guest writes the two fixed sibling directories
`{guest_context}.oci` and `{guest_context}.debs` exclusively, re-hashes each readback, requires the exact entry
set, and re-derives the OCI relationships (one linux/arm64 manifest, its pinned config and one layer). A second
guest check re-derives the same tree immediately before the build. Caps are exact and enforced while copying:
per-input (the largest reviewed input), aggregate (the reviewed payload bytes plus one 64 KiB framing bound) and
framing. A zero or too-small cap refuses every entry; a short, long, duplicated, missing, extra, mis-hashed,
link, foreign or already-present entry refuses and is retained, never adopted or pruned.

C4 now fetches exactly what D2 fetches (same 26 pinned URLs, same `download_cap_bytes` 38409328). Its step list
is: render verify spec, make `{colima_input_dir}`, prefetch, render the transfer spec, render the framed stream,
transfer, check the transfer receipt, verify the C3 context, re-derive the inputs on the guest, check that
re-derivation, `nerdctl build --help` and its flag check, the build, the iidfile read, and the by-ID inspect with
its platform and label check. Step envelopes are rebalanced inside the same 1200 s stage and 120 s reserve: the
transfer window is 90 s guest (+5 grace) in a 125 s host envelope, the pre-build input re-derivation is 35 s
guest (+5 grace) in 70 s host, and the build is 530 s guest (+5 grace) in 590 s host. Every guarded window keeps at
least 30 s of host receipt margin. The absolute stage deadline and reserve are unchanged.

C4's four remote commands are no longer exec steps the local ssh client carries. The manifest still states each
reviewed argv; the executor re-renders it as the fixed guest-helper launch, passing the command as bounded hex.
The helper rebuilds the same command from that hex and refuses anything else, runs it as its contained child,
and reports the argv it ran in its authenticated envelope. The executor compares that report with the reviewed
command before the next step. A local ssh exit closes nothing. `sudo -n env … nerdctl build` is still a client
outside the helper; its own daemon work is not claimed by the helper, and its completion is the synchronous
`nerdctl`/BuildKit client contract for the installed version, not a helper fact. `--network none` covers the
build's RUN steps; FROM cannot contact a registry because the base layer and debs arrive as local additional
contexts. The base image digest, the 22.04 platform assertions and the finite package closure are unchanged.

At r9, C4 joined C1, C2 and C3 as a plain proof request; current windows are described under Executor. C5 and C6 remain unadmitted, now because
C5's remote `nerdctl run` and its guest observations are still manifest exec steps the local ssh client carries.

## Revision r8 (supersedes r7 for D2 admission from 756; manifest `macos-4f6a5ed3-stages-r8`)

Keeps the r7 helper launch (hex plus `PR_SET_CHILD_SUBREAPER`) and the r6/r7 C2/C3 guest-helper admission.
D2 is admitted on the host path from 756: pinned Ubuntu 22.04 arm64 OCI blobs (Hub index, the one
linux/arm64 manifest, config, one layer) and the 22 offline jammy python3 + ca-certificates debs, each
hashed with exact size. They are assembled as `{desktop_input_dir}/oci` (an oci-layout) and
`{desktop_input_dir}/debs`. The docker build uses `--network none` and
`--build-context ubuntu=oci-layout://{desktop_input_dir}/oci` plus `debs={desktop_input_dir}/debs`.
FROM cannot contact a registry. The recipe is `FROM ubuntu` / `COPY --from=debs` / three explicit `dpkg -i`
invocations in pre-depends order: the 20 debs that have no in-set Pre-Depends, then `python3-minimal`, then `python3`.
Each later invocation starts after the previous one has configured its packages. A 22-package `installed` count, then
the unchanged loader, Python and coreutils checks, follow. Sha256 `8f1fa681…`, 1827 bytes. D3 and D4 become statically
admitted through predecessor D2. The earlier 318-byte recipe (sha256 `c00d5bd9…`, one `dpkg -i /debs/*.deb` glob)
failed the 775 Desktop build: `python3-minimal` and then `python3` were unpacked before their Pre-Depends were
configured, so dpkg refused them. Its bytes stay as evidence and are no longer a reviewed input. The ordering control
is `runtime_recipe_tests.py`: it reads the pinned deb metadata in `fixtures/runtime-deb-control.json` and runs the
recipe's own dpkg segments against a private fake dpkg.

C4's build argv is the same offline form with guest sibling paths `{guest_context}.oci` and
`{guest_context}.debs`. r8 left C4 unadmitted for want of a closed transfer of those host-fetched inputs;
r9 closes it (r9 section above). C2 and C3 join C1 as plain proof requests. D2 is a window that names the
original D1 receipt and requires `desktop_input_dir` absent.

## Revision r7 (supersedes r6 for the helper launch form; historical manifest `macos-4f6a5ed3-stages-r7`)

The helper still establishes genuine guest closure. It no longer crosses the Colima route as Python
source. The `-c` program is the pinned one-argv launcher
`exec(__import__('binascii').unhexlify(__import__('sys').argv[1]))` (no `$`, backticks, newlines or double quotes).
The helper bytes follow as lowercase hex. `own_source_sha256` hashes that decoded payload via
`sys.orig_argv`, `/proc/self/cmdline`, or the `-c` hex in `sys.argv` (Darwin 3.9 has no `orig_argv`).
Before spawn the helper sets `PR_SET_CHILD_SUBREAPER` (`prctl` 36 via ctypes) and refuses if that
call is missing or fails. After the waited child exits, a remaining live or zombie process whose
ppid chain reaches the helper (other than the waited child) is leftover and the envelope cannot
close. A descendant reparented to init is not leftover evidence. A stage that
refuses after a closed guest step reports stage-level `guest_closure: unknown`.

## Revision r6 (supersedes r5 for C2 and C3; historical manifest `macos-4f6a5ed3-stages-r6`)

C2 and C3 are admitted. Their Colima `ssh` echo, probe and stage steps, and C4's verify step, run the pinned
`macos_guest_helper.py`. r7 launches it as the pinned launcher plus hex; r6 used `-c <source>` directly.
The helper records Linux `/proc`
identity, runs one allowlisted payload as a contained child in the same session and group, waits, and emits a
length-prefixed envelope. The host authenticates that envelope against a durable one-use intent written before
launch: nonce, operation, helper source hash, boot, waited child exit 0, same session and group, empty group.
A local `colima ssh` client exit is `guest_unknown`. Inner stdout/stderr replace the step streams; the raw
transport is retained as `stdout.transport` / `stderr.transport`.

C2 order is unchanged: status, echo, compare, probe. An authenticated matching echo dispatches the probe. A
client-exit-only echo still lets compare read the captured bytes (quoting proof) and records the probe
`not_dispatched`. C3 derives `guest_spec` from the C2 probe parent and the C0 `token32`, stages the reviewed
recipe on bounded stdin, and refuses if the stage reports a different parent.

r6 left C4 and D2 unadmitted for engine-side network. r8 admits D2 on the host OCI/deb path; C4 stays
unadmitted for guest transfer of those inputs. C4's verify helper can close; `nerdctl build --help` remains
`guest_unknown`. Ordinary entry stays refused. At r6, C2 and C3 joined C1 as plain proof requests; current windows are described under Executor.

| Step | Prerequisite | Closed when | Otherwise |
| --- | --- | --- | --- |
| `c2-route-echo` | pinned helper, one-use nonce, hostile `ROUTE_ECHO` last | authenticated envelope, waited child 0, same session/group, empty group, inner bytes = `ROUTE_ECHO` | local client exit, truncated/missing envelope, leftover group, unreaped, setsid, cap/timeout: Unknown; nonce/boot/source/PID mismatch: refuse; lost stdout: Unknown, compare skipped |
| `c2-guest-probe` | closed echo and matching compare | same helper closure over the pinned probe source | not dispatched if the echo did not close |
| `c3-stage-context` | closed C2, C0 `token32`, recipe pin | helper closure and reported parent = C2 parent | replaced parent refuses; C4 still unadmitted for guest transfer |
| `c4-verify-context` | closed C3 | helper closure | engine-side help/build remain Unknown |

No native C2/C3 occurrence. Unsupported remote/engine closures stay Unknown. No generic SSH executor.

## Revision r5 (supersedes r4 for C1; manifest `macos-4f6a5ed3-stages-r5`)

C1 is the one download stage whose only fetch is pinned and sized, so it is admitted. Upstream metadata gives the image size.
Colima v0.10.3 caches the image by URL and trusts an existing cache file without a hash check, so the cache is not used.
Instead the fixed `c1-prefetch-colima-image` helper fetches the one reviewed URL into the owned, empty
`{colima_cache_home}/effigy-image/` directory and starts Colima with `--disk-image`. Colima checks that file against the
same sha512 at use (`limautil.DownloadImage` / `downloader.SHA.ValidateFile`; `colima start --help` lists the flag).

Source qualification (2026-11-08, public metadata only; no image or payload downloaded):

- Size `446992920` bytes: GitHub release asset `size` for colima-core v0.10.4 (`gh api repos/abiosoft/colima-core/releases/tags/v0.10.4`).
- sha512 `86fd0879…` equals the release `.sha512sum` sidecar (184 bytes) and colima v0.10.3 `embedded/images/images.txt`
  (arm64 containerd line).
- Release metadata also gives a sha256 digest (`3389c283…`). It is recorded here and not pinned: the pinned identity is sha512.

Metric. `download_cap_bytes` counts payload bytes the runner itself writes. HTTP headers, redirect bodies and the TLS
wire are not counted and not claimed. The bound is exact: the transfer must be `Content-Length` = the pinned size with
identity encoding and no chunking, the write loop refuses at size+1, and a live monitor checkpoint runs per chunk.
The C1 cap is lowered to the exact size. Redirects follow only to `github.com`, `objects.githubusercontent.com` or
`release-assets.githubusercontent.com` over HTTPS, at most three hops. A partial file is retained, never used or reused,
and an existing name, including a symlink, is a collision.

C4 and D2 were not admitted in r5. Their base-image pull and in-build apt fetches happened inside the engine
build, so the runner saw no payload bytes and nothing bounded them. r8 admits D2 on the host OCI/deb path
(756). C2 and C3 are admitted in r6 through the pinned guest helper. C4 still needs guest transfer of the
host-fetched inputs.

## Revision r4 (supersedes r3 for D1 and D4)

r3 refused D1 because `docker volume create` is idempotent and the name was caller-derived, so exclusive
creation could not be established. r4 names no volume in the create. The engine generates the name, the
create prints it, and the receipt is the inspect of that printed name with this occurrence's label. D4
re-observes the engine and each exact target before its own removal, compares it with the receipt that
produced the target, and includes the complete image inventory in the before and after foreign lists.
The contract is source-qualified by controlled transports. Native execution remains under a separate
owner GO and a Nucleus proof-only launch. Native execution of D2 through D4 stays under its own GO; the source
admission state is in the Desktop and proof-entry sections below.

The D1 window request names the original D0 receipt (`prior_receipt`, exact path and sha256). It verifies
that receipt's complete chain at plan time and at launch. It also pins the control root and the two Docker
directories to the identities the original D0 helper recorded. D0 is not rerun and is not relabelled.

## Revision r3 (supersedes r2)

r2 (`macos-4f6a5ed3-stages-r2`, manifest sha256 `50d6f46c…`, `macos_preparation.py`
`d651b3b6…`) was reviewed against upstream Colima 0.10.3 and Docker flag metadata. These
defects are corrected in r3:

- **C1 `--cpu` is not a Colima 0.10.3 flag.** The flag is `--cpus` (`-c`). The installed
  `colima start --help` example line still shows `--cpu 4`, but its flag table defines only
  `--cpus`. Tables win over examples.
- **C1 global writes.** `--ssh-config` and `--activate` default to true upstream. The stage
  passes `--ssh-config=false --activate=false`. Booleans take the `=` form, since a separate
  `false` token is parsed as a positional argument.
- **C1 host Docker config.** Colima runs `docker context create` on every start, whatever
  `--activate` says, so it writes the host Docker config store. The stage pins
  `DOCKER_CONFIG` to an owned empty directory.
- **C1 image cache.** Colima caches the VM image under `COLIMA_CACHE_HOME`, falling back
  to `~/Library/Caches/colima`. The stage pins `COLIMA_CACHE_HOME` to an owned directory.
- **C2 route echo first.** Colima passes `colima ssh` arguments through to `limactl shell`,
  so quoting is not established by Colima's source. The route echo now runs before any
  probe, stage or check source crosses the route. The echo runs the pinned guest helper
  with the hostile constant last; its characters cannot redirect, pipe or background.
- **C4 and C5 deadlines.** Each remote `nerdctl build` and `run` has a guest deadline
  inside a host envelope (table below). The check also carries an in-container alarm.
- **C4 and C5 image identity.** The build writes `--iidfile`. The check runs by that
  authenticated `sha256:` ID with `--pull=never`, never by the mutable tag. The ID is
  inspected for `linux/arm64`.
- **D2 build `--pull`.** Docker `build --pull` is boolean. `--pull=never` is not valid
  syntax for build, so the stage omits `--pull`. A missing base digest is pulled once from
  the official registry under the download cap. A present base is used with no network.
  `run --pull=never` is valid and stays on D3 and C5.
- **D4 cleanup authority.** Label, name prefix and same-name resources are not authority.
  A resource is removable only when this occurrence's creation receipt matches its exact
  engine, ID, name and label. The receipt comes from the stage's own `--iidfile`,
  `--cidfile` or `volume inspect` output.

C0 and D0 are byte-identical to r2. Their canonical hashes are `5eec2ae2…` and `91bad78a…`.
The planner can readiness-check them at the r2 manifest hash. Their evidence is an input
later only after exact pin comparison. Other stages bind to r3 only.

## Current local metadata

Observed once, 2026-11-07 20:42Z, by [macos_metadata_tranche.py](macos_metadata_tranche.py)
(runner commit `1ccd55420`). Five local commands, all exited 0, retained under the
private child `linux-artifact-preparation.l0tx7tam/macos-metadata-rfzuiadz`.

| Item | Observed |
| --- | --- |
| colima | `/opt/homebrew/bin/colima` 0.10.3 |
| docker client | `/usr/local/bin/docker` 29.8.0 (client `--version` only) |
| Colima profiles | `effigy` Stopped, `effigy-release` Stopped; `effigy-531-qualification` absent |
| Desktop context | `desktop-linux`, host `unix:///Users/tom/.docker/run/docker.sock`, TLS verify on |
| Other contexts | `colima-swallowtail-kiro-owner-proof` and `default` exist; not used by any stage |

This is local metadata. It is not a running-engine, daemon or guest observation.
Desktop reachability and the engine baseline are unknown until D0 runs under a GO.

Flag metadata read in this revision, with no daemon, VM or guest contact: installed
`colima start --help` (run with `COLIMA_HOME`, `COLIMA_CACHE_HOME`, `DOCKER_CONFIG` pinned
to a scratch fixture), `docker build --help` and `docker run --help`. Upstream sources read:
Colima `v0.10.3` `cmd/start.go`, `cmd/ssh.go`, `cmd/root/root.go`,
`environment/container/docker/docker.go`, `environment/vm/lima/yaml.go`,
`util/downloader/download.go`, `config/files.go` and `embedded/images/images.txt`. Docker
buildx build reference. nerdctl `v2.3.1` `docs/command-reference.md`.

## Paths (fresh, owned, pure-checked)

Control root `/Volumes/Stash/scratch/effigy/mac-4f6a-<token>`, with `<token>` eight
lowercase hex minted at C0. Owned children:

- Colima home `<control>/c`, the only Colima data root.
- Colima image cache `<control>/cache`, created by C1 before start.
- Colima Docker config `<control>/docker-config`, created by C1 before start.
- Qualification root `<control>/s`, empty and 0700, the only permitted host share. C1 passes
  `--mount none`, so the stage has no share; the root stays empty.
- Desktop config `<control>/d/docker-config` and context `<control>/d/ctx`, both owned and empty.
- Desktop image ID file `<control>/d/image-id` and container ID file `<control>/d/container-id`.

Profile `q4`. The pure `resolve_stage` plan lists each derived host socket and its
UTF-8 byte length, and refuses before activation, occurrence or claim when the path
plus its NUL and a four-byte margin exceeds `UNIX_PATH_MAX=104`. The check covers
the chain's profiles even when planning an earlier stage. Collisions, aliases and
Builds paths also refuse; protected and historical resources are never adopted.

### Socket source qualification

Read-only source check for #816 (no Colima/Lima binary execution):

| Repository | Tag | Resolved commit |
| --- | --- | --- |
| `abiosoft/colima` | `v0.10.3` | `00f6c297e92a82c04a4ab507db0a61435650d7e8` |
| `lima-vm/lima` | `v2.2.1` | `27bc4c4b101e27994e01e36566a0f2091bbca507` |

Lima's installed Cellar directory is `/opt/homebrew/Cellar/lima/2.2.1`.
Static binary SHA256: Colima `55278419bd4288e4ab11eb30be13f5e655f3886c0979b68e435df910dbc9bbdf`;
limactl `e79b14815dc7e51629e064dba950120c4c9323e2b8ba6a52fe816b15f7f9803d`.
Sources retained for review under this run's owned
`/Volumes/Stash/scratch/effigy/775dd65d/source.LGWcCoKr/{colima,lima}`.

Derivation at those commits:

- Colima [config/files.go:49–57,129–140](https://github.com/abiosoft/colima/blob/00f6c297e92a82c04a4ab507db0a61435650d7e8/config/files.go#L49-L140)
  reads `COLIMA_HOME` and adds `_lima` when `LIMA_HOME` is unset;
  [config/profile.go:31–35,58–73](https://github.com/abiosoft/colima/blob/00f6c297e92a82c04a4ab507db0a61435650d7e8/config/profile.go#L31-L73)
  adds `colima-` for the instance and the short profile for its config directory.
- Lima [pkg/limatype/dirnames/dirnames.go:53–59,82–91](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/pkg/limatype/dirnames/dirnames.go#L53-L91)
  derives the instance and `_networks` directories.
  [pkg/limatype/filenames/filenames.go:55–64,97–105](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/pkg/limatype/filenames/filenames.go#L55-L105)
  names SSH, HA, GA and the OpenSSH 16-digit temporary suffix. The instance creation
  refusal uses that longest name in [pkg/instance/create.go:38–43](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/pkg/instance/create.go#L38-L43).
  SSH joins at `pkg/sshutil/sshutil.go:538,550,572,581`; HA at
  `pkg/instance/start.go:229`; GA at `pkg/hostagent/hostagent.go:766`.
- Colima [environment/vm/lima/yaml.go:137–141,253–266](https://github.com/abiosoft/colima/blob/00f6c297e92a82c04a4ab507db0a61435650d7e8/environment/vm/lima/yaml.go#L137-L266)
  selects `user-v2` and forwards containerd/buildkitd. Their host paths come from
  [environment/container/containerd/containerd.go:20–28](https://github.com/abiosoft/colima/blob/00f6c297e92a82c04a4ab507db0a61435650d7e8/environment/container/containerd/containerd.go#L20-L28).
- Lima [pkg/networks/usernet/config.go:20–45](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/pkg/networks/usernet/config.go#L20-L45)
  derives `<name>_<fd|qemu|ep>.sock`; [pkg/networks/usernet/recoincile.go:35–60](https://github.com/lima-vm/lima/blob/27bc4c4b101e27994e01e36566a0f2091bbca507/pkg/networks/usernet/recoincile.go#L35-L60)
  requests all three, including the QEMU-named listener even for VZ. The VZ attach
  path uses the named network at `pkg/driver/vz/vm_darwin.go:178–181,417–439`.

The fixed chain uses VZ/containerd, named user-v2 and no mounts. VZ's serial output
is a file (`pkg/driver/vz/vm_darwin.go:376–384`), and its VM sockets use the
Virtualization framework rather than filesystem socket names (`:821–824`).
QEMU's serial/QMP/TPM and indexed `virtiofsd-%d.sock`, external-driver sockets (VZ registers internally in `pkg/driver/vz/register.go:15`,
imported by `cmd/limactl/main_darwin.go:9`), krunkit sockets and Docker/Incus runtime forwards are outside this chain.
`check_colima_start` refuses a runtime or VM-type change; a future change to these
fixed settings must requalify the inventory.

For any eight-hex token, `COLIMA_HOME` is 50 bytes including the trailing slash
used below. Plans list these nine paths (length excludes NUL):

| Relative to `<control>/c/` | Full path bytes |
| --- | --- |
| `_lima/colima-q4/ssh.sock` | 74 |
| `_lima/colima-q4/ssh.sock.1234567890123456` | 91 |
| `_lima/colima-q4/ha.sock` | 73 |
| `_lima/colima-q4/ga.sock` | 73 |
| `q4/containerd.sock` | 68 |
| `q4/buildkitd.sock` | 67 |
| `_lima/_networks/user-v2/user-v2_fd.sock` | 89 |
| `_lima/_networks/user-v2/user-v2_qemu.sock` | 91 |
| `_lima/_networks/user-v2/user-v2_ep.sock` | 89 |

The longest path uses 92 bytes including NUL, leaving 12 bytes before the limit,
well beyond the reserved four-byte margin. The #813 SSH temporary path remains
105 bytes before NUL and is refused by the pure planner with its full path and length.

### Fresh C1 after #813

A fresh C0 is required, with a new eight-hex token, new owned roots, a newly minted
token32, and fresh manifest/source/input pins and stage activations. Then run C1
once against that accepted C0 receipt. No native occurrence is authorized here.

The accepted #787 C0 for `9b47cb21` cannot supply new bindings: `validate_bindings`
requires the token's exact `control_paths`; `chain_key` binds token and control;
`verify_receipt_tree` checks that key; `origin_roots` requires the exact C0-created
roots and `check_recorded_roots` checks their device/inode identities. A new manifest
does not loosen these checks. Keeping the old token/root fails C1 first-use checks on its retained cache and
Docker config even though the shorter profile names a different instance. New
leaf bindings for that token are not supported. The smallest supported route is
a new token and C0; the existing control-root layout stays unchanged.
Do not delete, adopt or reuse #813's partial state, claim or activation under
`/Volumes/Stash/scratch/effigy/mac-4f6a-9b47cb21`.

## Executable rules

- Every exec step is an argv list whose first element is the pinned absolute tool.
  Strings, shell text and English placeholders are refused.
- Placeholders are `{slot}` names from a fixed set. `render()` refuses an unknown or
  unbound slot. `{{json .}}` is a literal Docker format string.
- Colima effect steps (C1–C6) pin `COLIMA_HOME`, `COLIMA_CACHE_HOME` and `DOCKER_CONFIG`
  to owned roots, and unset `LIMA_HOME`, `DOCKER_HOST` and `DOCKER_CONTEXT`. C0 keeps its
  r2 pin set, which `check_step(legacy=True)` applies only to C0 and D0.
- Desktop steps pin `DOCKER_HOST` to the observed endpoint and `DOCKER_CONFIG` to the
  owned empty config, and unset `DOCKER_CONTEXT`. Desktop argv is exec'd directly, with no
  shell. Each step runs against the observed socket, not a context name.
- Colima `start` must carry `--cpus`, `--ssh-config=false`, `--activate=false` and
  `--mount none`, and must not carry bare `--cpu`, `--ssh-config` or `--activate`.
- `docker build` and `nerdctl build` carry no `--pull` form, an `--iidfile` into an owned
  path, and `--platform linux/arm64`. `run` carries `--pull=never` and the immutable
  `{image_id}`, never a tag.
- A remote `nerdctl build` or `run` carries a `guest_deadline`. Its `timeout -k` wrapper
  sits inside the host envelope, with at least 30 seconds of receipt margin.
- No argument carrying source or JSON (`probe_source`, `stage_source`, `verify_source`,
  `check_source`, `guest_spec`, `guest_verify_spec`) crosses the Colima route before the
  c2 route echo has matched byte-for-byte.
- Helper steps name one of a fixed set of host actions, each a pure check or a receipt
  producer. No helper runs a shell.

## Stage order and bounds

Numeric caps are bytes. Storage is allocated bytes, measured by the runner on the named
owned roots. Downloads are declared sources, each pinned. Exceeding a cap is
Unknown and retained. Caps and sources are enforced by the runner, which is a remaining
gate below.

Colima (`colima`):

- **C0** admission (unchanged from r2): tool hashes against the tranche, tranche summary
  check, intent claim, owned roots created 0700 and empty, `colima start --help`.
- **C1** start, 900 s stage: `c1-make-owned-dirs` (cache, Docker config and `effigy-image`), then
  `c1-prefetch-colima-image` (210 s, the pinned image, see r5), then `colima start -p q4 --runtime containerd
  --vm-type vz --arch aarch64 --cpus 2 --memory 2 --disk 20 --root-disk 20 --mount none --disk-image {image}
  --ssh-config=false --activate=false` (600 s). The step budget is 30 + 210 + 600 = 840 s, the stage cap less the 60 s
  reserve. Download cap 446992920 bytes (the exact pinned size); storage cap 8 GiB. Source:
  `https://github.com/abiosoft/colima-core/releases/download/v0.10.4/ubuntu-24.04-minimal-cloudimg-arm64-containerd.raw.gz`,
  sha512 `86fd0879…`.
- **C2** identity, 300 s stage: `colima status -p q4 --json`,
  the pinned guest helper `echo` of the hostile constant, the byte comparison, then the
  guest parent probe through the same helper. Nothing else crosses the route until the
  comparison passes. The echo and probe close only on an authenticated helper envelope.
- **C3** stage Dockerfile by the existing helper, with the same argv and
  `runtime.Dockerfile` on stdin. Storage cap 1 MiB.
- **C4** runtime inputs, verify and build, 1200 s stage, 120 s reserve (r9). Steps: render the verify
  spec, make `{colima_input_dir}`, prefetch the pinned OCI blobs and 22 debs into it, render the transfer
  spec, write the framed stream, transfer it through the fixed guest helper (90 s guest deadline + 5 s grace
  inside a 125 s host envelope), check the guest transfer receipt, verify the C3 context, re-derive the inputs
  on the guest and check that receipt, `nerdctl build --help` and its flag check, then one local-path build:
  `timeout -k 5 530 sudo -n env DOCKER_CONFIG={guest_context}/docker-config nerdctl build
  --platform linux/arm64 --provenance=mode=max --iidfile {guest_iidfile} --tag
  effigy-4f6a-runtime:r1 --network none --build-context ubuntu=oci-layout://{guest_context}.oci
  --build-context debs={guest_context}.debs {guest_context}`. Host command envelope 590 s; the guest deadline
  is 530 s plus a 5 s kill grace, leaving 55 s of host receipt margin. Then the iidfile is
  read and checked as a `sha256:` ID, and `sudo -n nerdctl image inspect {image_id}` is
  checked for linux/arm64. The tag is never used after the build. Every remote command runs through the
  fixed guest helper and its receipt carries the argv that ran. Download cap 38409328 bytes (the pinned
  OCI/deb set); storage cap 4 GiB; sources: the four Hub OCI URLs and the 22 `ports.ubuntu.com` deb URLs.
  Guarded windows, each with a 5 s kill grace: transfer 90 s in 125 s, pre-build re-derivation 35 s in 70 s, build
  530 s in 590 s. The three small remote steps (build help, iidfile read, image inspect) run a 17 s guard with a 2 s
  grace in their 20 s allowance, so the receipt margin is kept. Step total 1075 s inside the 1080 s budget.
- **C5** one disposable check, 360 s stage, 60 s reserve (r10). Eleven steps, all guest steps through the fixed
  guest helper over the colima ssh route, and host-side facts checks. In order:
  `c5-guest-nerdctl-version` runs `nerdctl --version` (8 s, 2 s grace inside it) and `c5-check-nerdctl-version`
  requires exactly the source-qualified `nerdctl version 2.3.1`; otherwise nothing runs. `c5-disposable-check` runs
  `timeout -k 5 200 sudo -n nerdctl run --pull=never --network none --cidfile {guest_check_cid} --name
  effigy-4f6a-check-{token} {image_id} python3 -I -c {check_source}` (no `--rm`: the container stays for inspection;
  200 s guest deadline in a 240 s host envelope). `c5-check-facts` passes only with exit 0 and uid 0, Python 3,
  OpenSSL, a GNU C library loader and coreutils. `c5-read-check-cid` reads `{guest_check_cid}`, one full container
  ID. `c5-container-inspect` (`sudo -n nerdctl container inspect {check_container_id}`) and `c5-check-inspect` require
  one object whose `Id` is that ID, whose `Image` is the predecessor image, and whose `State` is `exited` with
  `ExitCode` 0. `c5-container-rm` removes that exact ID (`sudo -n nerdctl container rm {check_container_id}`, never
  forced) and `c5-container-list` (`sudo -n nerdctl container ls --all --no-trunc --quiet`) with `c5-check-absent`
  requires the ID to be absent from every full ID listed. Step total 293 s inside the 300 s budget after the reserve.
  Each guarded step's host allowance contains its guard, grace and a 1 s receipt margin, and dispatch refuses when the
  remaining allowance cannot contain the guard and grace.
  `{guest_check_cid}` is the C4 verify spec's context with `.check.cid` appended, read back from the C4 receipt's own
  spec file, whose sha256 the C4 helper recorded; the context must name this chain's token32.
  Closure: a local ssh exit alone closes nothing. Every guest step needs its authenticated helper envelope closed
  with waited exit 0 and the reviewed command. A nonzero exit, a deadline, an unknown closure, a version other than
  2.3.1, a container that is not exited with exit 0 or not from the predecessor image, a replaced container ID, a
  removal that fails, or a listing that still carries the ID stops later effects. The cidfile, the container and its
  residue are retained, and nothing is removed by name or pattern.
  Source qualification (nerdctl v2.3.1 source; the guest's own nerdctl version is observed at run time, not assumed):
  `nerdctl run` waits in `runAction` and returns the container's exit status. `--cidfile` is written during `Create`,
  before the container is created, and an existing file is refused (stat then create, not an exclusive create).
  `container rm` refuses a running container without force (`container %s is in %v status`), so the exited check
  container is removed without force. The inspect fields are `Id`, `Image` and `State.Status`/`State.ExitCode`. The
  `Image` field's content is not established as an image ID from source, so a mismatch fails closed and that is owed
  as a native fact. The `id` filter's exact matching was not verified, so the listing is tested for exact membership.
  Owed native facts: the guest's own nerdctl version, the `Image` field form, and the actual lifecycle on the guest.
- **C6** observe only: `colima list --json`. No stop. A running VM is retained until a
  separate GO covers a stop.

Desktop (`desktop`), with the pinned env above:

- **D0** endpoint, owned dirs and baseline (unchanged from r2). The owned/foreign split is
  observation only.
- **D1** owned fixture volume, named by the engine (r4). The create names no volume:
  `docker volume create --label effigy.qual=<token>` prints one engine-generated 64-hex name. Before it, a
  label observation (`volume ls --filter label=effigy.qual=<token>`) must be empty. After it, `volume inspect`
  of that exact printed name must carry the same name and this occurrence's label. Both streams are retained.
  Not a receipt unless the inspect closed. Window 300 s, reserve 60 s, storage cap 1 MiB, download 0.
- **D2** runtime image, 1200 s stage, 120 s reserve. Steps: engine info, the Buildx link (`link_buildx_plugin`, before any fetch), the plugin's own metadata
  command through the owned link (`d2-buildx-metadata`, `docker-cli-plugin-metadata`, env as the Docker route, 10 s),
  the metadata check (schema 0.1.0, vendor `Docker Inc.`, version `v0.37.1`), make this occurrence's input directory
  `{desktop_input_dir}` and build context `{desktop_context_dir}` (both fresh, below), prefetch the pinned OCI blobs
  and 22 debs into the input directory, stage the Dockerfile, a custody check of the same link and bytes, then
  `docker build --platform linux/arm64 --provenance=false --label effigy.qual=<token> --tag
  effigy-4f6a-desktop-runtime:<token> --iidfile {desktop_iidfile} --network none
  --build-context ubuntu=oci-layout://{desktop_input_dir}/oci
  --build-context debs={desktop_input_dir}/debs {desktop_context_dir}`.
  `--provenance=false` is the documented boolean form that disables attestations. It is
  pinned so the image's provenance state is explicit, not left to the driver default.
  Build host envelope 840 s, and a kill or timeout is Unknown. Then the iidfile is read, and
  `docker image inspect {image_id}` is checked for linux/arm64 and the occurrence label.
  Download cap 38409328 bytes; storage cap 2 GiB. Sources: the four Hub OCI URLs and the 22
  `https://ports.ubuntu.com/ubuntu-ports/...` deb URLs. Step total 1074 s inside the 1080 s
  budget. The proof-entry window names the original D1 receipt and requires this occurrence's three destinations
  absent.
  **Buildx route and fresh occurrence (planner amendment for effigy#770).** The failed occurrence ran
  `docker build --provenance=false` with the private config's `ClientInfo.Plugins=[]`. Buildx was not discovered, the
  CLI fell back to the legacy builder, and it rejected `--provenance` (exit 125), so there was no image. The route now
  links the pinned plugin into the owned private config. `link_buildx_plugin` requires Docker Desktop bundle version
  4.92.0 (`CFBundleShortVersionString`) and the Desktop `docker-buildx` sha256
  `a8638d2188cfd8bdfce7c1be2ce16aeeaf02c57b51443e9860799de5cf5ee105`, then creates one new `{desktop_config}/cli-plugins`
  directory whose only entry is a symlink to that plugin. A missing, changed or other plugin, or an existing link,
  refuses before any fetch. Nothing is read from a global config, and no legacy fallback is reachable from this stage.
  The plugin's metadata command is the one Docker's discovery uses before it accepts a candidate (docker/cli
  `cli-plugins/manager`: symlinks in `$DOCKER_CONFIG/cli-plugins` are accepted, and a candidate whose metadata is
  invalid is not found). The buildx `main` runs `NewDockerCli` first; that source step is source-established only for
  the metadata path, and the native run is owed. The plugin's bytes are part of the stable input set, so every launch
  re-hashes them, and the custody check repeats the link and bytes before the build. Host-only helper steps in D2 are
  5 s or less; every native exec clock is unchanged. Each activation's D2 outputs are fresh siblings of
  the original paths, derived from the verified activation by `d2_occurrence`: `{desktop_config}-<suffix>` (the private
  Docker config, created exclusively and empty before the first D2 step; a collision, symlink or other foreign entry
  refuses before any step), `{desktop_input_dir}-<suffix>`, `{desktop_context_dir}-<suffix>` (created by
  `d2-make-context-dir`) and `{desktop_iidfile}-<suffix>`. D2's engine, metadata, build and readback steps run with
  `DOCKER_CONFIG` set to the fresh config, and its `cli-plugins` link is made there. The 775 config and its link stay
  untouched, and D3/D4 keep the original `d/docker-config`. The original
  D0/D1 paths and every earlier payload, recipe, claim and receipt stay in place and are never deleted, reset or
  adopted.
- **D3** disposable check: the activation derives a 16-hex SHA-256 suffix with the
  d3-occurrence domain. Its name is effigy-4f6a-dcheck-<token>-<suffix> and its
  cidfile is <control>/d/container-id-<suffix>. The window binds both identities,
  checks absence and the D0 directory identities, and refuses a collision. The run
  keeps --pull=never, --rm, --network none, the fixed label, image ID and check source.
  The file is read as a full ID and its path and hash are bound to the creation receipt;
  D4 rechecks them and the name before selecting an exact ID.
  Docker CLI 29.8.0's
  [run](https://github.com/docker/cli/blob/v29.8.0/cli/command/container/run.go)
  calls createContainer; its
  [create path](https://github.com/docker/cli/blob/v29.8.0/cli/command/container/create.go)
  creates the cidfile, writes the returned ID and removes an unwritten file on failure.
  The fixed client uses the existing private config and parent directory. Allow 8192
  allocated bytes on the existing high-water monitor: the observed 4096-byte cidfile
  block and one 4096-byte directory allocation. The control inventory permits only
  that new owned regular file (at most 64 bytes and one block) and its parent metadata
  change; other observed changes refuse. Sampling is not a kernel quota, and the
  control-root counter does not measure engine or container storage.
  The 779 occurrence remains failed and spent. Its original cidfile and D2 image are
  preserved. Before any fresh D3, a separate bounded, receipt-based observation must
  establish whether container
  2a35eb59a74953c0c78c252a1fcbea83463cd488e9d701c2d3292ae366ee2f2f
  on engine ef9cc944-30a9-4df6-b4e5-0d8b56ee9ecd, with the original
  cidfile and effigy.qual=e272a9cc provenance, is terminal or retained. That
  source-ready observation entry now exists (see the Executor section); it is source
  only and unarmed, and until its own pinned proof-only launch records a native receipt,
  this stays an outstanding gate, not evidence of absence or closure.
- **D4** cleanup by creation receipt only (r4). Observations: engine (`docker info`), `ps -a`, `volume ls`,
  the complete image inventory (`image ls --all --no-trunc`), and the selected image's inspect by ID. The
  helper `select_cleanup_targets` selects only receipted creations on this engine. Each class (containers,
  volumes, images) then runs, only if it selected exactly one target: a fresh `docker info`, an exact
  `inspect` of that target, the helper `check_cleanup_target`, and the removal. The check compares every
  immutable fact the receipt kept (name, driver, scope, mountpoint, creation time, labels for volumes;
  ID, repo tags, creation time, OS, architecture, labels for images; ID, name and label for containers).
  Any drift, missing fact, foreign label or replacement refuses before that removal. After the removals:
  `ps -a`, `volume ls` and the complete image inventory. Foreign lists must equal the lists observed
  before the first removal. Removals are `docker rm`, `docker volume rm` and `docker image rm` by exact
  ID or name, never `-f`, never prune. Stage budget 600 s, reserve 60 s, step total 460 s. The pre-removal
  check is not an atomic conditional delete: the engine can change between the check and the removal, and
  the post-compare is the only detection after that. Each class with no selection is recorded as skipped.


## Proof-entry bindings (G1–G3, gated)

The frozen 627 artifacts are bound in the manifest by path and sha256, unchanged from r2.
Each file was verified present with its index sha256 on 2026-11-07. Bound, not run:

- Source: head `4f6a5ed3`, tree `dcb4c791`, candidate
  `/Volumes/Stash/scratch/effigy/release-source/candidate-4f6a5ed3.xwm4pr7j`, and
  the E14 binding file sha. Identity is dev `16777245`, inode `156037736`, uid 501, 0700.
- Baseline D (627 `baseline_D`, reused not rebuilt): `old_binary` at
  `/Volumes/Stash/scratch/effigy/627-baseline-correction.f_43nyce/producer-replacement-823d859d-r1/frozen/effigy-old-823d859d-r1`,
  sha256 `a603c82a…`, `old_source_head` `823d859d`, and the `old_source_receipt` hash
  `f86a1f3a…` (the index gives no receipt path). Launch's binding record reads these
  cells, so they are bound here and not rebuilt.
- Receipts: candidate `895b3cf2…`, production `52e2c55b…`.
- Frozen: production, candidate, nextest binary, kernel and custody test executables,
  kernel custody fixture, utility, cargo metadata, nextest config (`e4afe6df`) and
  derived binaries metadata (`d247ddd1`).
- **Nextest fixture path rule.** The kernel custody fixture is an absolute frozen copy
  in `non-test-binaries`. Nextest joins the build directory with the path, and an
  absolute path replaces the join. 627 did not exercise this. G3 refuses until a
  metadata-only probe in the qualification root proves the handling. The guest path
  is the identical absolute path, or a planner-approved rewrite. It is never an
  assumed join.

Gates:

- **G1** E14 source placement. Either a copy of the exact candidate tree or a single
  read-only share of only the candidate directory. The planner chooses at activation.
- **G2** artifact, sidecar and producer receipt placement. Byte-identical copies under
  `<source_root>/.effigy/linux-release/artifacts`, each matching its frozen sha256. No
  producer receipt is synthesized. RecipeE and 686 are missing.
- **G3** proof run: `launch.py` with an exact per-backend GO, after G1, G2 and the
  fixture path proof.

## Closure, retention and failure

- Closed means a waited exit 0 with no deadline or cap. `may_continue` admits the next
  step only on `closed`. A deadline, a cap, a missing waited exit or a host kill is
  **Unknown and retained**. Unknown never replays, and no later disposable check or cleanup
  runs on that stage.
- A daemon can keep building after its client is killed. A timeout or kill claims no
  container or build closure. The retained resources are listed per stage.
- Each activation's intent is claimed once. A reused intent refuses before effects.
- Old 531 markers and consumed activations stay history, and the boundary test refuses
  them. The guest name `effigy-531-image-<token32>` is the helper's shape. The token is
  fresh, and no 531 receipt is reused.
- Retain intent, GO copy, every stream, PID, waited exit and hash under the run's
  private child. Receipts stay outside Queue cleanup.

## Decisions

- Zero host shares is the preferred C1 form. `--mount none` returns nil mounts upstream,
  so no `$HOME` mount is added. The qualification root is an empty directory, not a share.
- Colima 0.10.3 flags come from its own `start` command table. The installed help example
  is not a flag source.
- Colima effect stages pin the Docker config and image cache to owned roots. C0 keeps the
  r2 pin set, since it only reads help.
- The Colima build is a local-path build from a helper-staged file, not tar-stdin.
  Installed nerdctl 2.3.1 refuses tar-stdin contexts, so no builder switch is made.
- Desktop pins the observed endpoint with `DOCKER_HOST` and an owned empty
  `DOCKER_CONFIG`, not a context name. Desktop build and run do not use `--pull` in the form
  r2 wrote. Build omits it and run uses `=never`. D2 pins `--provenance=false`, and C4 pins
  `--provenance=mode=max`, one explicit form per backend. Each build argv must carry exactly
  its form.
- The guest name stays under the helper's `effigy-531-image-` shape. The helper
  enforces it, so the plan binds the name instead of changing the helper.
- Route echo is the pinned guest helper with the hostile constant last. The constant has
  no redirect, pipe or background character. A mangled shell route cannot write. The helper
  source is the one reviewed exception on the echo argv; other source slots still wait for
  the compare.
- `c4-nerdctl-build-help` reads flag metadata before the build. The build only starts if
  `--iidfile`, `--platform` and `--provenance` are in the guest's own help text.

- Manifest r4 changes, D1 and D4 only. C0 and D0 are byte-identical to r2 and r3. D1 replaces
  `d1-volume-absent` with the label observation `d1-label-absent` and takes the name from the create's
  output, so no argv carries a caller-chosen name. D4 gains per-class observation, inspect and check steps
  before each removal, `d4-observe-images` and `d4-post-images`, and a budget of 600 s (step total 460 s).
  Each stage's step total stays under its budget.
- Why the engine-named create (r4). Docker's daemon chooses `stringid.GenerateRandomID()` when the name is
  empty, and the CLI prints the name the API returns. The API has no 'created' flag: the store's internal
  `created` boolean is not sent, and an existing volume with the requested name is returned without a
  label comparison. Attribution therefore rests on three things, not on the API alone: the engine-generated
  64-hex name; label absence observed before the create; and the inspect of that exact name carrying this
  occurrence's label. A name collision needs a random 256-bit collision, and a label collision needs a
  same-user process that already holds this token. The caller never chooses the name. Source reads:
  moby `volume/service/service.go` (`GenerateRandomID` for an empty name), `volume/service/store.go`
  (`create` returns an existing volume with `created=false`, no label comparison), and docker/cli
  `cli/command/volume/create.go` (prints `res.Volume.Name`). Native confirmation that the daemon's create
  prints a 64-hex name is a D1 proof obligation; a different shape refuses at the create post-check.

## Executor (source only)

`macos_stage_executor.py` is the stage runner. This source preparation starts no native backend.

### Colima windows name their original predecessor receipts

C1's launch-time window carries `prior_receipt`: the exact path and sha256 of the original executor C0
receipt of this token's chain. The plan and the launch both verify that complete chain then
(`checked_prior`, then the chain verifier in execution): a closed, non-simulated executor chain bound to
this token, control root and endpoint, whose C0 origin minted the chain's `token32` and recorded the owned
roots. A window ref is exactly those three keys and never carries `historical`, so the pinned 688 C0 record
stays a plain-request binding only: a fixture record, a doctored or simulated receipt, another token's
chain or a changed hash refuses at the plan and again at launch. The request binds the control root
identity; execution re-derives the three owned roots from the verified C0 creation record (first-use
entries included) before the claim. The C1 `absent_paths` are the token's first-use `cache`,
`cache/effigy-image`, `docker-config`, Colima profile directory and `_lima` VM state under `colima_home`:
any of them present refuses before the request claim, so no cache, config, image or profile state is
adopted. The GO is written at launch for the requested duration — at most the 900-second stage with the
60-second reserve shared by prefetch and start. Every C1 runtime gate, the exact pinned image and the
fixed start flags are unchanged, and the plain C1 request keeps its pre-created absolute-time activation
route. C1 execution itself remains a separate proof-only launch.

C2–C6 use the same launch-time window mechanism: C2 names C1, C3 names C2, C4 names C3,
C5 names C4 and C6 names C5. Each `prior_receipt` is exactly `{stage, path, sha256}`;
planning and launch verify the complete genuine, closed, non-simulated chain back to C0.
C4 additionally requires `{colima_input_dir}` absent before the request claim. C2, C3, C5 and C6
create no first-use host destinations; guest destination checks remain in the authenticated helpers.
Every stage keeps its own seconds, reserve, steps, caps, fetch pins and closure rules. C2 still checks
the hostile route echo before the probe, C3 derives from that probe and C0's token32, C4 transfers
only the fixed finite inputs, and C6 retains and observes without stopping. Plain requests remain
supported. No executor change is needed: the existing launcher re-runs `window_recipe` before the
claim, and the executor already verifies these predecessor chains and dispatches these exact stages.
Each native execution requires its own Nucleus source-immutable proof launch.

### Desktop readiness before D1

The prospective `plan-ready BINDINGS OPTIONS` request binds the original D0 receipt, its three
directory identities, the expected engine, the exact installed Desktop bundle and launcher, the
source-reviewed Docker CLI, and the freshly observed identity of the existing `Docker.raw`. It
composes a readiness prefix with the existing D1 window. The proof launch has one monotonic deadline
of at most 900 seconds, including the 60-second reserve; the selector clock is 1380 seconds, the largest proof stage
(C4 and D2, 1200 seconds including the reserve) plus 180 seconds of process margin, and no stage clock changed; readiness has at most 600 seconds and D1
has at most 300 seconds or the remaining allowance. The readiness and app-start claims are distinct
from D1's claims. A spent or uncertain claim is not replayable. The original D1 creation and
inspection receipt still determines whether a volume is attributable.

The fixed CLI context-name query is the sole default-config exception. Its normal config loader may
internally decode stored auths. It does not retrieve credential values, call helpers, authenticate or
contact a registry. The adapter does not parse, copy or expose the global config. The exact request's
`requiresOperatorGo` discloses this loader behaviour and both possible effects: starting Desktop and
creating an empty volume. A later Nucleus qualification must pin that request and explicitly obtain
GO for that scope; this source preparation grants no native action. The pinned CLI is Docker 29.8.0,
sha256 `4fb51be4a4e9389b942ecbf52901a962bdccdaa38275e6de4d353df33981d600`, built from
[`88096ef`](https://github.com/docker/cli/tree/88096ef00576baf72a9cb45caa45c0544c40e0a7).
Its [context show](https://github.com/docker/cli/blob/88096ef00576baf72a9cb45caa45c0544c40e0a7/cli/command/context/show.go)
prints the selected name; its [config loader](https://github.com/docker/cli/blob/88096ef00576baf72a9cb45caa45c0544c40e0a7/cli/config/configfile/file.go)
can decode auth fields. A synthetic config and helper sentinel exercise this exact CLI without a
daemon or real credentials. The shared stage supervisor has one narrowly restricted context-query
mode: it retains the original name and empty stderr only when the command succeeds with a valid name.
It suppresses all output on other results, since the CLI can include configuration aliases in errors;
the process, waited exit and suppression disposition remain recorded. This avoids writing unexpected
config text into evidence while keeping the existing responsive supervisor and its bounds.

The context is read before and after readiness. Startup is refused unless it is already
`desktop-linux`, because [Docker documents that Desktop selects that context at startup](https://docs.docker.com/desktop/setup/install/mac-permission-requirements/).
The already-ready path skips startup and requires the selection to remain unchanged. Neither check
prevents concurrent external changes. The fixed launcher opens only `/Applications/Docker.app`;
its exit does not establish readiness. An initial bounded engine read makes the startup decision:
a matching engine skips startup, while only the two recognised local connection errors permit a
separately claimed start, including when the socket exists but refuses connections. Other failures
and changed engine facts refuse. The recognised error requires exit 1 and the exact stderr; stdout
is retained and may contain client facts, as in the spent 702 attempt. Nested helper timers preserve
the earlier outer deadline. The owned socket and a matching engine response establish readiness.
Drift, missing telemetry, unexpected
failed commands and exhausted reserve stop before D1.

The held, no-alias Desktop disk is sampled before, during and after the operation against a 512 MiB
allocated-growth threshold, alongside the private-root monitor. Samples are not a hard filesystem
quota. The launcher process group does not contain the app or daemon's entire lifetime; Desktop may
remain running after a timeout. No descendant-closure or complete resource certificate is claimed.
Fixture simulations are labelled and their D1 receipts cannot be consumed as native chain evidence.

### The failed 779 Desktop observation (source only)

[macos_desktop_observation.py](macos_desktop_observation.py) is one fixed, proof-only,
receipt-bound observation entry for the exact container the failed 779 D3 disposable check
left behind. It is not a stage of the manifest, grants no qualification credit, and cannot
make 779 pass, mint D3 creation credit, authorize adoption or removal, refund the consumed
allowance or count toward effigy#622. Source only: nothing is armed, and no native
observation has run.

The entry consumes the genuine retained 779 record by exact SHA-256 — the parent retention
index, the retained bindings, the failed D3 stage receipt, the failed step receipt with its
empty original streams, the resource observations, the original run intent, the retained
cidfile, the D2 prepared-image receipt and the original engine read — and derives the
observed identity block from those bytes: container full ID, name effigy-4f6a-dcheck-e272a9cc,
label effigy.qual=e272a9cc, prepared image sha256:8abc0af…, engine ef9cc944-30a9-4df6-b4e5-0d8b56ee9ecd
and the original D0 control paths. Every derived value must equal its pinned constant, so
the bytes and the review agree or the entry refuses. Nothing is inferred from age or list
emptiness, and no closure fact is claimed from the failed receipt's later state.

`plan EVIDENCE_ROOT ACTIVATION_ID` is read-only: it verifies the reviewed 29.8.0 Docker CLI
bytes, re-derives the whole record, holds and checks the original D0 chain's root identities,
and prints the request. The request binds the merged helper source (the executor's program
set plus this entry), the tool and config identities, the ten retained originals, the
derived container/engine identity, the D0 receipt reference, the frozen root identities and
one finite 120-second window, and it carries a `requiresOperatorGo` disclosure of exactly
the two read-only operations. External provenance remains the existing Nucleus exact
merged-commit source-immutable proof-only launch under recorded same-sequence authority;
the request file is a hash-pinned binding, not an activation. The execute form refuses any
request that differs from a fresh derivation before the one-use `obs779-` claim and before
any child runs.

Execution holds the three original D0 roots by identity, starts a strict live watch over the
whole control tree (any sampled change kills the child and is retained as a refusal;
sampling is not a quota), and runs at most two fixed children through the shared
preparation_stage supervision with separate original streams, waited exits, input-hash
custody, process identity and per-step allowances inside the one monotonic deadline: one
`docker info --format {{json .ID}}` engine-identity read (30 s) and, only after that read
closes with exactly the recorded engine ID, one exact full-ID `docker container inspect`
with a template that selects only `Id`, `Name`, `Image`, `Config.Labels` and `State`
(60 s). Both children run with the original intent's pinned Desktop env (owned private
config, observed endpoint, no inherited context) and no shell. The inspect template requests
no environment, no secrets, no logs and no unrelated inventory.

Verdicts are recorded in one exclusive fsynced receipt beside the step originals:
`terminal` (identity matches, state exited/dead), `nonterminal` (identity matches, state
created/running/restarting/paused/removing — retained and reported, never stopped, killed,
restarted or waited for), `absent` (only one source-qualified exact-ID byte pair after a
matched engine read: exit 1 and stderr containing exactly one final LF. The pinned Docker CLI
29.8.0 source at [`88096ef`](https://github.com/docker/cli/tree/88096ef00576baf72a9cb45caa45c0544c40e0a7)
and its Moby client establish the local `Error: No such container: <full id>` form with empty
stdout, while retained 794 establishes the daemon-wrapped pair: stdout is exactly one LF and
stderr is `Error response from daemon: No such container: <full id>` plus one LF. The CLI's
inspection command routes formatted output to stdout even when the inspect returns an error;
its printer supplies the terminal line ending. The classifier admits only these two complete
pairs. Additional bytes, lines, other IDs, transport/API/context errors and malformed outputs
remain `unknown`; the retained bytes identify the observed daemon form, and do not authorize
general error matching. `mismatched` (a present object
whose ID, name, label or image differs — reported, never acted on), `refused` (fresh engine
identity differs: the inspect is never dispatched), and `unknown` (daemon, context,
transport, parse, malformed, ambiguous, timeout, cap or any other nonconforming shape,
which stays unknown and retained). A replayed request refuses at its already-spent claim.

Controls run through `effigy
test:qualification:workspace-private:macos-desktop-observation`: retained-shape fixture
originals and one supervised harmless fake Docker that records every argv it is given,
covering exact terminal match, still-running retention, proven exact-ID absence, wrong
engine (refusing before the inspect), wrong name/label/image/ID-in-response, malformed and
ambiguous responses, unrecognised status, transport and other nonzero failures, a bounded
timeout, missing, altered and unindexed originals, an original-intent mismatch, source/tool
drift after freeze, a control swap after freeze, replay, alias and nested evidence roots,
request canonicality, invalid activation ids, live mutation detection, separate per-step
originals, and that only the two fixed read-only argv forms exist at all. Tests stay under
$TMPDIR; the production entry keeps only its own literal constants and cannot be pointed at
a caller root. Compile runs through
`effigy check:qualification:workspace-private:macos-desktop-observation:compile`.
The proof-only entry is `effigy qualification:macos:desktop-observation-proof-only`.
Not proven: any native execution. The entry has never run against the real engine.

- **Activation.** The ordinary `execute_stage` entry refuses. The separate
  `qualification:macos:proof-only -- REQUEST SHA256` entry is only for an actual Nucleus proof-only
  launch at the pinned commit and inputs, after native operator GO. Nucleus must record that execution;
  the program cannot independently establish its provenance against another process of the same user.
  A request file grants no authority. The entry checks its bindings and makes an exclusive input-hash claim.
- **Prospective proof binding.** A later Nucleus qualification must pin the accepted commit, this exact
  selector and the SHA-256 of the read-only request. The request has exactly `stage`, `activation_path`,
  `activation_sha256`, `bindings`, `evidence_root`, `inputs_sha256`, `manifest_sha256` and
  `source_sha256`, plus `inputs`. `inputs_sha256` is the canonical hash of `inputs`: exact reviewed
  repository-relative paths and content hashes, plus canonical machine paths and hashes for the interpreter
  and both tools. The executor independently derives and compares the entire set before claiming. It retains
  the actual canonical runtime input map in the stage receipt and rehashes it before each launch. The planner still
  needs a fresh token and private roots, exact request and activation bytes and their hashes, current
  tool hashes and an expiry window. The operator question must name the selected stage's native effects
  (both claim and create private files and directories; C0 invokes Colima's start help, while D0 reads
  the Desktop endpoint and engine inventory). No question,
  GO or proof execution is created by this source task.
- **Preparing a D0 binding without native effects.** From the exact merged preparation checkout, choose
  a fresh eight-hex token. Create a read-only bindings JSON under the preparation root using
  `macos_preparation.control_paths(token)` filtered to `SLOTS`, plus `desktop_host=DESKTOP_HOST`,
  `token`, `check_source=CHECK_SOURCE`, and `route_echo=ROUTE_ECHO`. For example, after checking
  `a19b3c4d` is fresh and selecting a private preparation directory, make that file with:

  ```sh
  python3 -I -B -c 'import json,sys; sys.path.insert(0,"docs/qualification/workspace-private-531"); import macos_preparation as p; t="a19b3c4d"; b={k:v for k,v in p.control_paths(t).items() if k in p.SLOTS}; b.update(desktop_host=p.DESKTOP_HOST,token=t,check_source=p.CHECK_SOURCE,route_echo=p.ROUTE_ECHO); print(json.dumps(b,sort_keys=True))' > /Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam/bindings-a19b3c4d.json
  chmod 400 /Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam/bindings-a19b3c4d.json
  effigy qualification:macos:proof-only -- plan D0 /Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam/bindings-a19b3c4d.json
  ```

  The paths and token are examples to be checked for collisions, not a reservation. Run
  `effigy qualification:macos:proof-only -- plan D0 /absolute/path/to/bindings.json`.
  This only reads the checkout, interpreter and pinned tool files; it prints the stable `inputs`,
  hashes, candidate head and bindings hash, and creates no roots, claims or activation. Copy those
  exact fields into the request and activation: the request uses `stage`, `bindings`, `inputs`,
  `inputs_sha256`, `manifest_sha256`, `source_sha256`, `activation_path`, `activation_sha256`, and
  `evidence_root`. The activation uses the printed hashes, `candidate_head` and `bindings_sha256`,
  plus `decision`, `decision_version`, `stage`, a unique `activation_id`, `issued_utc`, `expires_utc`,
  and `scope: single-activation` as checked by `macos_preparation.check_go`. Hash the read-only
  activation first, then place that hash in the read-only request and hash the request. The future
  selector is `qualification:macos:proof-only -- /absolute/path/to/request.json REQUEST_SHA256`;
  neither file names the future checkout. The pinned candidate head remains H; Nucleus separately pins
  the exact merged preparation commit K and selector/request hashes and records the real operator answer.
  D0 requires its token's control directory to exist privately, its `d/docker-config` and `d/ctx`
  paths to be absent, a private evidence root and private activation and claims directories under the
  configured scratch root. The Desktop endpoint, engine identity, tool bytes, all input rechecks,
  one-use claims and other runtime gates still apply. A real operator GO and Nucleus proof-only launch
  remain necessary; this plan grants neither.
- **Launch-time window recipe.** The legacy exact-file route above remains supported. For a delayed
  approval, provision the private activation, claims and evidence directories and the C0 parent or D0
  control root first. Create an options JSON containing exactly `activation_id` (fresh, 8–64 safe
  characters), `duration_seconds` (integer greater than the stage reserve and at most its stage
  seconds and 1200), canonical `evidence_root`, and — for every stage whose window consumes a
  predecessor (D1–D4 and C1–C6) — `prior_receipt`, the exact `{stage, path, sha256}` of that original receipt.
  With the bindings JSON described above, run:

  ```sh
  effigy qualification:macos:proof-only -- plan-window D0 /absolute/path/to/bindings.json /absolute/path/to/options.json
  ```

  Use C0 in place of D0 when appropriate. C1–C6 each name their original executor predecessor
  (C0–C5 respectively) as `prior_receipt` (see Executor). Save the output unchanged as a
  read-only request under scratch, retain its exact bytes and SHA-256, and pin the merged K, selector
  `qualification:macos:proof-only -- /absolute/path/to/request.json REQUEST_SHA256` and request hash
  in Nucleus. Null root identities are provisioning preconditions: rerun the plan after provisioning,
  before freezing the request. The D0, C1 and C4 `absent_paths` are fixed first-use destinations and must
  remain absent through launch; existing directories and symlinks refuse before the request claim. Check all
  named destinations are unused. The native operator answer and
  actual proof-only launch remain necessary. At launch the request claim is spent even on an uncertain
  or partial write; retain the claim, activation, proof evidence and stage evidence, and never retry it.
  The runtime timestamps come from launch, not from the preparation file. Source-test simulations confer
  no native proof credit.
- **Static admission.** C0 and D0 have no remaining static helper refusal. Their actual execution still
  needs the external proof-only precondition and all runtime checks. Remaining refusals:
  - C1: admitted statically (r5). Its one fetch is the pinned Colima image through the prefetch helper, with the exact
    size and sha512. Ordinary execution still refuses. It is in the proof entry (`PROOF_STAGES`) as a plain proof
    request (the pre-created absolute-time activation) and as a launch-time window that names its original executor
    C0 receipt (see Executor). C2–C6 also support windows naming their original predecessor receipts.
  - D2: admitted statically (r8 / 756). Its fetches are the pinned Ubuntu 22.04 arm64 OCI blobs and 22 offline debs,
    hashed with exact size, then a `--network none` docker build from local `--build-context` oci-layout and debs.
    Ordinary execution still refuses at the owner mechanism; its proof-only window names the original D1 receipt
    and requires this activation's three occurrence destinations absent.
  - C4: admitted (r9). It fetches the same pinned OCI/deb set D2 fetches into `{colima_input_dir}`, frames it
    as one fixed finite transfer, streams it to the guest through the fixed helper, re-derives the guest layout
    twice (on receive and immediately before the build), and runs every remote command through the same helper.
    Ordinary execution still refuses at the owner mechanism. Plain proof request or window naming its original predecessor receipt.
  - C2: admitted (r6). The pinned guest helper authenticates echo and probe closure. Ordinary
    execution still refuses. It is in the proof entry as a plain proof request or window naming its original C1 receipt.
  - C3: admitted (r6). `guest_spec` derives from the C2 probe output and the `token32` that C0 mints.
    Ordinary execution still refuses. Plain proof request or window naming its original predecessor receipt.
  - D1 is statically admitted (r4). Its ordinary execution still refuses at the owner mechanism; its
    proof-only window names the original D0 receipt (see Executor).
  - D3 and D4: in the proof entry as windows. D3 names the original D2 receipt and requires its activation-derived cidfile absent;
    D4 names the original D3 receipt, selects only receipted full IDs on this engine, and refuses a predecessor that
    did not close, so a failed predecessor cannot trigger cleanup. Native execution still needs a separate GO.
  - C5 and C6: admitted (r10). C5's check, cidfile read and container listing are fixed guest-helper operations
    (see C5). C6 follows C5. Both support plain proof requests and windows naming C4 and C5 respectively.
- **C0 (fresh tokens).** Its handlers are `verify_tool_sha` (the live colima and docker binaries
  against the sha256 the pinned 688 C0 record names) and `verify_tranche_summary` (the tranche
  summary against the sha the same record names). C0 creates its own roots under the scratch root
  and refuses on any collision. For the recorded token it refuses: that C0 is historical, and C1
  binds it. Each claimed C0 activation mints one
  `token32` (32 lowercase hex), written exclusively as `token32.json` and recorded by hash in the receipt
  as `origin`. It is the chain's origin: `origin_token32` reads it back from the verified C0 receipt before
  C3 and C4 claim. A historical C0 minted none, so C3 and C4 refuse for the recorded token. That receipt also
  supplies the owned roots a fresh chain's later C stages must re-observe (effigy#698): its creation step's
  evidence, identity rows and pinned inventory are the binding, with no caller path and no 688 record.
- **C1 binds the pinned 688 C0 record, by identity.** The record is
  `macos-4f6a5ed3-activation-7deaf1b3/C0.stage.json` (sha256 `15a5373e…`) and its tranche summary
  (`macos-metadata-rfzuiadz/summary.json`, sha256 `cdfb7fb6…`). C1 accepts it only when the bytes
  match the pinned sha, the record is a passed, zero-share, safe C0 for the recorded token, the live
  colima and docker binaries match the recorded tools, and the recorded roots match the recorded
  identities. This is a binding, not a receipt: C1 claims its own activation, and nothing is
  replayed, adopted or recreated. A fresh token takes no part of this: its C1 consumes its own verified
  executor C0 receipt, and the 688 records are never consulted for it.
- **Plan.** `plan` is pure. Each stage resolves to its argv, pinned env, timeouts, caps, runtime
  preconditions, the sources it derives at run time (`runtime_derived`), and both its global and
  static admission reasons. Derived and unpinned slots render as explicit placeholders.
- **Bindings.** Checked against the reviewed paths for the token, the pinned socket URL and the
  pinned check program. A caller who supplies a derived slot or an unpinned source refuses.
- **Inputs.** Program files, selector configuration, manifest, interpreter, recipe and tool binaries
  are bound by the activation. The fixed helper checks its input hashes before and after its work.
- **Claims.** `macos-stage-claims/<activation_id>.claim`, exclusive, under a provisioned directory.
  A missing directory refuses. The executor never creates one under the scratch root.
- **Pins.** The evidence, claims, activation and owned directories, and for D1 to D4 the private
  Docker config and context directories, are pinned by canonical path, owner, mode 0700, device and
  inode. Each is rechecked before every step. C0 pins its roots once it creates them.
- **Helper supervision.** C0 and D0 use a fixed child entry through the shared stage supervisor.
  The parent samples the same absolute deadline and live counter while the child runs, including a
  zero limit. A cap, timeout, lost counter or invalid result stops the stage. Polling is observation,
  not a physical quota. The parent verifies and holds created directory descriptors and records both
  original streams, the observed process group, spawn time and waited exit. Cleanup waits only through
  the stage reserve; an unreaped child remains unknown with its identity recorded. An uninterruptible
  kernel wait is not promised to terminate. Later-stage helpers remain behind their existing refusals.
- **After an unknown guest closure.** A guest step whose closure is unknown halts later effects. The
  only helper that may still run is the C2 route-echo comparison, a read of the captured bytes against
  the reviewed constant before any payload. Every later step is recorded as `not_dispatched`.
- **Verification of a preceding receipt** is field-level. The receipt must be a closed executor
  receipt on the same chain, with the expected PRIOR stage and a genuine origin. Its claim and
  activation copy must match. Its step inventory and kinds must equal the manifest's. Each step's
  receipt and both streams must match recorded hashes. Each creation receipt must equal, field for
  field (kind, id, name, label, engine), what its producing stage's receipt, recorded argv and
  original streams establish.
- **Recorded identities.** C1 onward binds the owned roots by device, inode, owner, mode 0700 and (at C1
  only) their first-use entries. Each root is opened no-follow once: that descriptor supplies the comparison and
  then becomes the stage's held pin, so no path is re-observed between them and a same-user swap in that gap is
  pinned as the opened directory and refused at the next recheck before any step. A fresh chain's roots come
  from its own verified C0 creation record: the paths of the token's supported shape, and the identities and
  first-use entries that record's `make_owned_dir` evidence and pinned inventory name (effigy#698). The
  recorded token instead keeps the three pinned 688 roots (token `7deaf1b3`) by count; that route never
  receives a minted token32 or a fresh binding, and a fresh chain never adopts the 688 identities. D1 to D4
  require the chain engine to be `ef9cc944-30a9-4df6-b4e5-0d8b56ee9ecd`, the engine 690 observed. An empty or
  default `docker info` template refuses.
- **Guest sources (pinned).** Every C2, C3 and C4 guest step runs the reviewed
  `macos_guest_helper.py` (sha256 `204d44b7…`) as
  `python3 -I -B -c <LAUNCHER> <hex(helper)> <operation> <nonce> …`. Probe, stage and verify then run the
  reviewed `macos_guest_context.py` (sha256 `5daf93e2…`) as the contained child; transfer and check-inputs
  run the reviewed `guest_inputs.py` (sha256 `d2d0ea05…`), with the transfer's framed stream inherited on the
  helper's own stdin so it is never buffered by the helper. That context file is a copy of the historical
  `image_context.py` (sha256 `af74047f…`, unchanged), with one contract change: `stage` also reports the
  parent binding, so the C4 verify spec derives from the C3 output. build-help, build, read-iidfile and
  image-inspect are the helper's own fixed operations: it rebuilds each command from a bounded hex spec,
  refuses anything but the reviewed form, and records the argv it ran. The executor reads every source from
  the pinned files and refuses a changed one. All three files are bound program files: the GO commits to their
  bytes, so a change after activation refuses before any dispatch as an input drift. The pin is the content
  check; the binding is what the activation commits to.
- **Derived specs.** `render_guest_spec` builds C3's spec with `build_guest_spec` from the C2 probe's
  parent binding, the chain's `token32` and the recipe hash. In C4 it builds the verify spec with
  `build_verify_spec` from the C3 output and the reviewed runtime pin; the spec's context must carry the same
  `token32`, and it now also carries the pinned input layout the guest must find beside the context. The
  context path becomes `guest_context`, `guest_iidfile` is that path with `.iid` appended, and
  `guest_oci`/`guest_debs` are that path with `.oci`/`.debs`. `guest_input_spec` is the compact JSON of
  `guest_inputs.transfer_spec` (parent, name, layout, limits) as lowercase hex, written by
  `render_input_specs`; `render_input_transfer` writes the framed stream from the pinned prefetch. None of
  these is caller-supplied: the plan shows placeholders, and the run derives the values from the chain and the
  reviewed pin. A C3 stage that reports a different parent than the C2 probe refuses.
- **Colima and Desktop.** Colima children resolve `docker` to `/usr/local/bin/docker` through an owned
  `bin/docker` link at the front of `PATH`, re-checked on every Colima step. Desktop runs with its
  pinned socket, `DOCKER_HOST`, an owned `DOCKER_CONFIG`, no `HOME` and no `DOCKER_CONTEXT`.
- **Guest.** A local ssh client exit, zero or not, is `guest_unknown`. Every guest step of C2, C3 and C4
  closes only when the authenticated envelope is closed and the local client also waited exit 0: echo, probe,
  stage, transfer, verify, check-inputs, build-help, build, read-iidfile and image-inspect. A timeout, cap or
  monitor refusal is `unknown_retained`. C5's remote run stays `guest_unknown` on a client exit.
- **Test seams (tests only).** `TEST_SEAM` lifts owner/static refusals inside the
  test process. Historical closure simulations are replaced by refusal tables
  and real-handler controls (effigy#854). Tables reject simulated receipts before
  evidence reads, and owner/predecessor refusals before dispatch or claims.
  Tests with stubbed predecessors or C0 origin walks identify those seams.
  `FreshChainRootTests` retains genuine C0, C1 and C2 receipts with controlled
  local tool children; it replaces no predecessor or root verification.
- **Desktop context (D2).** `prefetch_runtime_inputs` fetches the pinned OCI blobs and debs into the
  empty owned `{desktop_input_dir}` and assembles the oci-layout. `host_stage_dockerfile` writes the
  reviewed recipe exclusively as the build context's `Dockerfile`, reads it back and compares it with
  the pin. An existing `Dockerfile` refuses: nothing is adopted or overwritten.
- **Dispatch coverage (`CoverageLedgerTests`).** Every manifest step has one entry. The kinds are:
  `executed` (the adapter ran it with a real local child or the real handler, including D2's
  `host_stage_dockerfile`), `executed_stub_chain` (the adapter ran it with a stubbed predecessor),
  and `helper_direct` (the handler ran on controlled inputs, not through a stage).
  C2 echo and probe are `executed` through injected helper envelopes. C3 is
  `executed_stub_chain` the same way. All seventeen C4 steps are `executed` on a
  genuine C3 predecessor: host handlers run for real, transfer and pre-build
  re-derivation run the pinned guest source against the fixture's owned parent,
  and every guest step closes on an authenticated envelope naming its command.
  A `not_dispatched` step proves refusal only. Production coverage: a
  matching authenticated C2 echo dispatches the probe; a client-exit-only echo lets compare run and records
  the probe `not_dispatched`; a mismatched echo refuses; an echo whose allowance runs out stops before the
  probe. C4 closes end to end; a replaced or missing host input refuses before the stream is written; a guest
  receipt that names other inputs refuses; an unknown transfer closure stops the stage before any later effect;
  a missing build flag stops before the build; a non-zero, lost or unreaped build stops before the iidfile
  read; and the chain verifier re-derives the retained spec, stream and guest receipts and refuses a tampered
  one. D2: the Dockerfile is staged, built and read back; a
  pre-existing Dockerfile refuses before the build; a build whose allowance runs out stops before the
  readback. Static admission and owner refusals are tabled without simulating closure.
- **Not proven.** Native execution or an actual Nucleus proof execution and native GO. Native confirmation
  that the engine's volume create prints one 64-hex name and that its inspect carries the anonymous and occurrence
  labels as the source reads predict (D1). Native per-target re-observation and removal (D4). Termination of an
  uninterruptible kernel wait. Native C2/C3/C4 helper closure (Linux `/proc`, prctl child subreaper and the
  `/proc` process walk in a Colima guest). Native C4 fetch, transfer, build and the synchronous nerdctl/BuildKit
  completion the build receipt rests on. The C0 origin is exercised by a test run only, so the `token32` route has not run
  natively. The fresh-chain root binding is proven against local fixture C0 receipts only: no native C0 has
  created a root that a later C stage then re-observed. C5 still cannot close under the guest rule.
- **Create-unknown volumes.** A D1 create whose closure is unknown, or whose inspect failed, leaves an
  engine-named volume with the occurrence label and no receipt. D4 never removes it. The stage stops
  and retains the create stream, so the volume is reported for manual removal.


## Remaining gates

- **RecipeE and 686**: G1 and G2 wait on the Linux recipe and its artifacts.
- **Native proof of the executor**: the controls are injected and local. Running any
  admitted stage needs its own owner GO, and the executor has not run natively.
- **Failed-779 observation**: the source-ready observation entry exists and its controls
  pass, but no native observation receipt exists. Any fresh D3 stays gated on that
  separately pinned, receipt-based observation of the exact failed container.
- **Colima `docker` child**: the owned link is the enforcement. Every Colima step re-checks
  the link's target and refuses a different one.
- **Nerdctl `--provenance=mode=max`**: the guest `build --help` check proves the flag
  name exists, not that the `mode=max` value is accepted. The planner checks the retained
  help text at C4 activation. r9 additionally records the argv the helper ran, so the
  activation can compare it with the reviewed command.
- **C5 routing**: C5's remote `nerdctl run` and its guest reads still cross as local ssh exec
  steps. They need the same fixed-helper routing before C5 can close.
- **Activation**: each stage needs its own fresh GO with its hashes.

## Validation

Controls run through `effigy test:qualification:workspace-private:macos`
(`macos_preparation_boundaries.py`: 112 cases, injected only, no native action; the stage bounds, the fixed
commands and the manifest pins are checked against the files on disk). Docs run through `effigy qa:docs`.
The executor runs through `effigy test:qualification:workspace-private:macos-executor`
(`macos_stage_executor_tests.py`: 256 cases, injected local children, no native action; execution runs only
under the test-only seams described in the Executor section; `macos_guest_helper_tests.py`: 30 cases;
`guest_inputs_tests.py`: 29 cases over the framing, caps, receive, readback and guest layout re-derivation,
one of them in a real child process).
Its shared primitive change is covered by `effigy test:qualification:linux-preparation`.
The failed-779 Desktop observation entry runs through
`effigy test:qualification:workspace-private:macos-desktop-observation`
(`macos_desktop_observation_tests.py`: 35 cases, retained-shape fixture originals and one
supervised harmless fake Docker child, no native action) and compiles through
`effigy check:qualification:workspace-private:macos-desktop-observation:compile`.

## Desktop exploration route (effigy#831, in progress)

`qualification:macos:desktop-smoke` creates a fresh token and owned control below
its fresh TMPDIR child, with private claims, activations and evidence. It runs D0,
one D1–D3 chain request, then D4 on that chain's own receipts. It records complete
container, image, volume and network inventories before and after, plus their
difference. It never adopts the retained D2/D3 resources or the prepared D4 packet.
A failure retains its exact resources and receipts; it does not guess cleanup IDs.
The original top-level control paths remain valid for existing sealed requests;
nested controls derive every child from the canonical, hash-bound control parent.

Prepare the later sealed chain with
`qualification:macos:proof-only -- plan-desktop-chain BINDINGS OPTIONS`, using
D0 as the predecessor. Run its frozen request only through an owner-launched
`qualification:macos:chain-proof-only` occurrence. The D1–D3 budgets add up to
one outer deadline; every original stage deadline, reserve, cap, exclusive claim,
receipt check and drift refusal remains in place. Colima uses the same loop.

D4 deliberately remains outside the chain. The full-session driver consumes a
resident runtime image, while it creates its own workspace state volumes and
never mounts D1's anonymous volume. Keep the prepared historical D4 packet
untouched while exploring. After the full session closes, the planner can order
that packet's exact receipted cleanup; deleting its image before a session using
that image would fail the driver's resident-image check. Freeze the final
session against integrated Recipe A–E artifacts and a stable shared-engine
baseline before any sealed #622 occurrence. Exploratory receipts give no #622
credit. Full-session Desktop exploration and the final ordering observation
remain work in progress.

The fresh native preparation chain closed on 2026-11-10 under
`/Volumes/Stash/scratch/effigy/a9b25b38/desktop-smoke-1es4tm7j` (token
`ca385b4b`). D0, chained D1–D3 and D4 each closed. All four complete engine
inventories matched before and after: no container, image, volume or network
was added or removed at closeout. The historical D2/D3 resources and D4 packet
were untouched. This is unsealed preparation evidence, not a full-session
result. An earlier attempt remains at `desktop-smoke-c8iowqdj` under the same
run root: D0/D1 closed, then an author edit triggered source-hash drift before
D2 could proceed. Its inventory difference contained only that owned volume. The volume was later
removed by exact identity after re-reading that D1 receipt and matching its live labels
and engine; the cleanup receipt is `fs.Ve8uJ2ev/initial-d1-volume-cleanup.json` under the run root.


## Desktop full-session exploration (effigy#831)

The full session is **not complete**. The current-main optimized host candidate
and current kernel inputs were built locally through Effigy/Oxide. The real guest
producer outputs and source freshness anchor were copied with their original
bytes, timestamps and provenance into the owned Stash fixture. No installation,
sealed occurrence or #622 credit was consumed.

`qualification:macos:desktop-smoke -- --retain-runtime` closes fresh D0–D3 and
retains `preparation.json`. `qualification:macos:desktop-session-smoke` consumes a
hash-bound request with that D3 receipt, the same runtime image and pinned real
artifacts. It uses the fixed Desktop socket and an owned private Docker config,
without changing contexts. The receipt chain is checked in its own preparation
claims namespace. After the session, clean only that preparation with
`qualification:macos:desktop-smoke -- --cleanup-preparation PREPARATION --sha256 HASH`.
This runs the unchanged D4 checks against that chain, never the historical packet.

The exploratory request may also carry the planner-approved `external_checkout`
directory identity described in [contract 053](../../knowledge/contracts/053-workspace-activation-interface.md#detached-docker-routing-effigy831).
It is unsealed Desktop-only, canonical, fresh and owned. The checkout is bounded
to 16 MiB of source/state; artifact, temp and evidence roots remain on Stash.
The sealed entry does not accept this exception.

Native evidence lives under `/Volumes/Stash/scratch/effigy/a9b25b38/`:

- `desktop-smoke-tf0e6sag`: fresh D0–D3 closed on token `1754da80`; its image was
  `sha256:e2e7b9eaf725608f8baa4a75b0fe92fb91d8880704c8ecbc22d4b2e0d48d50a2`.
  Its separate cleanup D4 later closed after the failed sessions were removed.
- `fs.Ve8uJ2ev/session-3.log` exposed a real product fault: the detached owner put
  the pinned socket URI in `DOCKER_CONTEXT`. The supervisor now preserves endpoint
  routing with `DOCKER_HOST` and an empty context. Named-context and Colima routes
  retain their original meaning. The diagnostic-only error expansion was removed.
- `fs.Ve8uJ2ev/session-4.log` reached an owned Compose container on Stash but timed
  out after 300 seconds with the container still `Created`. A tiny read-only Stash
  bind also timed out during create; a no-mount container ran normally.
- The planner-approved tiny `/Users` bind passed (`usersbind-qq56avc3`). The full
  `/Users` fixture then also stalled in `Created` and hit the same 300-second bound
  (`fs.Ve8uJ2ev/session-6.log`). An independent plain actual-checkout bind, a tiny
  masked bind and a no-copy volume probe also timed out. No-copy did not solve it;
  the exact Docker Desktop cause remains unresolved. No shared settings or engine
  lifecycle were changed. A timeout now records the exact owned bind source and
  points to Desktop bind/file-sharing diagnosis without widening its deadline.
- Both journalled supervisors were independently identity-checked and stopped via
  the fixture utility. Their recorded process groups were observed empty. Exact
  inspected IDs and creation receipts governed removal of all five retained
  containers, their five volumes and two networks. The `/Users` fixture was
  archived and removed; its final regular-file size was 328,933 bytes, with no
  observed sample above the 16 MiB bound. The archive and cleanup records are in
  `fs.Ve8uJ2ev/`; no fixture process remains.

Final container, image and volume inventories match task start. The intrinsic
foreign `bridge` network changed from prefix `35b7af735a59` to `d731d2007d15`;
its cause is unknown. The complete final network IDs and the normalized comparison
with the original short-ID listing are retained in `final-inventory.json` and
`final-inventory-difference.json`. Every requested network mutation named one of
the two receipted owned project networks by its full ID. Do not describe the
foreign inventory as unchanged. The original #805 image/volume and #807 D4 packet
remain untouched.

### D4 ordering and remaining sealed plan

Keep D4 **after the full session**. The attempted session actually created its
container from the D3-receipted image. Its two mounted state-mask volumes were
new session-owned volumes, not D1's anonymous volume. D4 removes image and volume
together, so running it before a session that selects that image would break the
resident-image prerequisite. Keep the historical packet untouched until the #622
owner completes the session using its pinned image.

Before freezing a candidate, resolve the Desktop bind-start failure and run the
complete unsealed cell successfully. Then freeze coherent reviewed Recipe A–E
artifacts, genuine guest sidecars, engine/tool hashes, a fresh qualification-root
identity and complete stable foreign inventories. Measure optimized commands
against the sealed 60-second command bound; the exploratory 300-second activation
allowance does not change it. Reuse retained successful preparation where valid;
if new preparation is needed, freeze one D1–D3 chain after its fresh D0. The owner
then launches the separately authorized #622 full-session occurrence. Only after
that session closes does the owner launch the already prepared historical D4.
The `/Users` exception above is exploratory only; a sealed use would need its own
reviewed admission and authority. No full-session success is claimed here.

### Desktop directory-lock diagnosis (effigy#864)

On 2026-11-10 at 23:20–23:30 UTC, fresh owned probes did **not reproduce** the
earlier hang. Desktop's retained backend log does locate the earlier failure:
`DockerApiProxyVolumeSharer` waited on Desktop's own `POST /locks/lock-directory`
for the Stash checkout's `.effigy/state` from 04:41:49.985Z until a five-minute
timeout at 04:46:49.987Z, then timed out locking `.effigy/secrets`. The `/Users`
session hit the same state/secrets lock timeouts at 04:58:50.644Z and
05:03:50.647Z. This identifies the stalled Desktop operation, not its underlying
cause or what cleared it. The original fixture is no longer available for an
exact replay; no full-session or #622 success is claimed.

Desktop reported 4.92.0 (240144), Engine/CLI 29.8.0, Compose v5.5.1, 18 CPUs,
8192 MiB memory, 1024 MiB swap and Resource Saver enabled. Effective shared roots
were `/Users`, `/Volumes`, `/private`, `/tmp` and `/var/folders`. VirtioFS was
confirmed by VM startup and live mounts; the also-enabled legacy `useGrpcfuse`
flag does not describe the active implementation.

Brief-v2 probes used a fresh empty owned `/Users/tom/.effigy-desktop-diag-*`
directory and owned Stash scratch only. All bounds were 20 seconds for create,
25 seconds for start and 15 seconds for inspect; none expired. Running time
below ends at the first successful inspection, measured from start invocation.

| Probe | Create | Start | Running confirmed |
| --- | --- | --- | --- |
| Busybox, both dedicated binds | 0.102 s | 0.129 s | 0.176 s |
| Effigy-rendered ordinary service, both dedicated binds | 0.213 s | 0.326 s | 0.375 s |
| Stash bind with two empty read-only named-volume masks, no-copy | 0.083 s | 0.134 s | 0.173 s |

The rendered service also wrote successfully to its owned checkout bind. The
fresh nested-mask probe reached the same Desktop lock API and returned OK in
about 6–9 ms. The evidence supports an earlier path/state-dependent Desktop
directory-lock stall; its trigger and lock holder remain unknown. It does not
establish a consistency, propagation or path-rendering fault. Effigy's relevant
mask construction is `build_guest_mask_mounts` in
`crates/effigy-containers/src/workspace.rs`, applied by
`rewrite_workspace_mounts_for_direct_compose` in `workspace/compose_rewrite.rs`.
No clearly correct product fix was established, and the isolation masks must
not be removed as a workaround. No code or Desktop settings changed.

**Planner-brief error:** before v2, four read-only probes mounted `/Users/tom`
itself, including `effigy864-qe9lkmvl-users`, triggering Tom's home-sharing
warning. All four had been removed before v2 resumed. No broad bind followed
the amendment. The dedicated empty `/Users` fixture was removed after use.

Durable evidence is `/Volumes/Stash/scratch/effigy/desktop-diag-qe9lkmvl/`:
`README.md`, `commands.jsonl`, `lifecycle-summary.json`,
`stall-window-backend.txt`, `current-lock-api.txt`, archived fixture inputs and
exact cleanup receipts. `SHA256SUMS.json` has SHA-256
`3a759215246ea4b2e2407db21a53136e6b3a01e0af0132ef40d6f920ab27c00e`.
All nine containers, two volumes and the newly pulled busybox image were
removed by exact identity; no network was created. Complete final container,
volume, network and image inventories match task start. No fixture process
remains. Archived pre-v2 inputs are evidence and must not be replayed.

No restart or settings change is indicated while fresh probes pass. If the
same lock API stalls in the next authorized unsealed session, retain its exact
fixture and logs; Tom can then restart Desktop and retry that small owned
fixture as a recovery experiment. A restart is not a proven fix, and only Tom
applies Desktop lifecycle or settings changes.

### Desktop session retry (effigy#865)

The 2026-11-10 retry at main `0d2c58883` did **not pass**. Fresh D0–D3
preparation closed on owned token `27985a4c`. The session ran at
23:55:26–23:55:31 UTC and failed in its first activation, at **apply frozen
workspace render**, with child exit 125. It created no session container,
volume or network. No matching Desktop lock-directory request was observed;
this attempt therefore did not exercise the earlier directory-lock stall.

This was a worker preparation error: the empty private Docker config lacked
Compose plugin discovery. Read-only controls reproduce exit 125 with
`compose --project-name … version`; a separate owned config naming Desktop's
existing `cliPluginsExtraDirs` returns Compose v5.5.1 and exit 0. The failed
child's raw stderr was not retained, so that attribution combines its journalled
exit with the matching read-only reproduction. No product defect is established.
An earlier entry refused before creating the checkout because PATH selected a
symlinked Docker CLI; a same-byte real copy corrected that preflight issue.
There was one activation attempt and no activation retry.

D4 then closed, removing the preparation image and volume by exact identity;
D3 had already removed its check container. Complete final container, image,
volume and network inventories match task start. All four fixture journal runs
are closed with complete cleanup; none of twelve recorded process identities
remains live. No shared setting, context, engine lifecycle or Colima profile
changed. Evidence originals remain under this run's scratch until its release;
the [retained evidence](desktop-865/README.md) survives that cleanup.

Task #866 below supersedes the manual CLI-copy workaround. This result provides
no full-session or sealed qualification credit.

### Desktop CLI/Compose preflight and single session (effigy#866)

The unsealed Desktop preparation and full-session entries now resolve the real
Docker CLI behind its PATH symlink and record the link, target and original
content hash. They reference the existing bundle's Compose directory through
`cliPluginsExtraDirs` in a fresh private config, without reading or changing
Desktop's own config. Both Docker and Compose versions must succeed before
native preparation resources or the session checkout are created. Session
checks use the exact private HOME/config carried into activation; failures have
named causes. Sealed artifact rules, Discard custody and all original bounds
remain in force. The readiness fixture selector is
`test:qualification:workspace-private:macos-desktop-readiness`.

Fresh D0–D3 and both version preflights passed. The one 2026-11-11 session at
`e66ba085c` applied Compose with exit 0 and reached a running owned container,
then failed during publication/selection with no selected session committed.
No matching lock-directory/backend lines were observed. The retained owner's
stop request remained pending for 30 seconds; after two identity checks its
exact supervisor PID was killed. Exact-ID session cleanup and D4 then completed.
Final shared-engine inventories equal task start, all four owners are closed
with complete cleanup, and none of twelve process identities remains live.
The [retained result and hashes](desktop-866/README.md) distinguish the empty raw
activation CLI stderr from the apply child's intentionally unavailable Discard
stream. There was no session retry or engine restart. Successful Desktop
full-session exploration remains outstanding; this run gives no sealed credit.

After review `5851771381`, the operator amended task #866's held brief to
version 2: Discard remains a custody pin, and the recorded apply receipt,
caller/selector stderr, preflight receipts and backend-window observation
replace raw apply-child stderr. An apply failure additionally requires a
labelled read-only reproduction; this apply exited 0. The amendment changes
evidence acceptance for the original attempt, with no replay or custody change.
