# Linux transport and Vault-backed placement proposal — unarmed

No VM/SSH/account/key/storage inspection or effect is authorized here. A later
fresh Queue-bound stage needs exact operator effects, numeric time/storage bounds
and disposition. Keep the reviewed **SSH** transport; Parallels authenticates
preparation facts, not a replacement proof transport.

## Fixed diagnostic snapshot binding

`linux_diagnostic_proof.py` binds admission to the canonical JSON digest of the
complete registered snapshot list. The retained 7,911-byte list has raw SHA-256
`b0af61c3dbe419c45d33d54e61261670b11eb4cd803e39bf267b6424d4833a21`; parsing
its 36 entries and encoding sorted compact JSON gives the authoritative pin
`d21610ba279b4faf2d7a62c87ef8f1050943fc8b6c072a04c71ce4d0c953e3ad`. The raw
stream digest is not the admission pin. The earlier 62-character value was
malformed and could never equal a SHA-256 digest.

Focused controls use the retained snapshot-list bytes directly, accept JSON
formatting and key-order changes after canonicalization, and refuse duplicate
keys or changed inventory. Retained registered-VM metadata is replayed through
`check_vm` in stopped/running/stopped states; only the identity, home and disk
fields consumed by that checker are fixture-controlled. This fixes source
admission only: the prior native observation remains refused/spent, and this
change supplies no native proof credit or execution authority.

## Evidence and unresolved facts

The `b1a9018c` v2 readiness turn observed the selected UUID
`433c9e5b-c7b9-469f-9633-24b4addcd3b5`, package
`/Volumes/Vault/Parallels/Ubuntu 26.04 ARM64.pvm`, retained running. Parallels
execution worked as root: aarch64 Ubuntu26.04.1, Python3.14.4, Docker29.9.2,
Compose5.6.1, local engine `56fee8f8-1a38-42d6-acf9-d446ecff279c`. SSH service/socket
and port22 listened at observed `10.211.55.8`; no SSH connection, alias, auth or
host-key discovery occurred. Sharing was disabled and `/Volumes/Builds` absent.
Guest root was `/dev/sda2`, ext4. Rust was unavailable in that root context;
other users were not inspected. These observations are historical, not current
health. Host drive-remount events also forbid assuming old VM handles are healthy.

## Prefer native guest storage with an explicit provenance chain

Path spelling, filesystem semantics and physical backing are different claims.
A canonical `/Volumes/Builds/scratch/effigy/...` inside the guest does not prove
removable storage. Conversely, a native guest ext4 filesystem may be backed by
this approved Vault VM package without using a host shared folder. **Prefer that
candidate**, subject to the following fresh positive chain:

1. Mounted/canonical Vault and exact registered UUID/Home/description; actual
   configured virtual disk path inside that package, no alias/external disk
   substitution. Capture nonsecret host metadata and actual identities/hash.
2. Authenticate the running guest via Parallels and correlate its native block
   device/root partition with that configured disk using available controller /
   disk identity metadata. The historical `/dev/sda2` name alone is insufficient.
   Missing correlation or unhealthy/unknown handles refuse; no start/repair.
3. Record actual native mount source,target,fstype,device and parent ownership;
   establish that the approved backing is Vault and that the exact parent is on
   that filesystem. Only after a placement GO, create fresh owned0700 canonical
   source/artifact/empty qualification directories at the packet's existing
   absolute scratch path. No symlink or copied substitute elsewhere.
4. Run bounded owned filesystem controls for Unix sockets/descriptor transfer,
   locks, atomic/no-clobber publication and fsync/readback on that actual native
   filesystem; retain faults. Ext4's name alone is not proof of these results.

Proposed **placement binding** to freeze with the Linux inputs/GO: host provenance
receipt path/hash naming Vault/package/UUID/configured disk; authenticated guest
correlation receipt/hash; native mount source/target/fstype/device and actual
parent/root device/inode/uid; exact native scope and filesystem-control receipts.
This chain needs retained review before Linux activation. It is a proposed
physical-placement admission, **not an implemented provenance checker** or a
claim that the current packet's lexical root guard proves physical backing.
Keep all existing `qualification_root` path/device/inode/uid/0700/no-alias/empty
checks unchanged. Do not arm Linux until the explicit chain is independently
accepted and bound; no source-only approval supplies it.

A Parallels share is not the default remedy. If native Vault backing cannot be
proved, any separately approved share must expose only one exact owned root,
not home/Vault/Builds/parent scratch/siblings. Its driver, uid/mode mapping,
Unix sockets, locks, atomicity and durability are unproved and may fail custody.
No automatic share/mount, broad sharing, fstab edit or fallback is authorized.

## Concrete bounded identity/transport stage

Propose a **600-second maximum stage within remaining task budget**, with smaller
host and actual guest deadlines, no start/install/build/proof. Read-only admission
first checks exact running VM and storage chain, proposed account `effigy531`
absence/uid allocation, Docker socket/group, relevant SSH public settings/version,
public host key, current IP, password-disabled pubkey login and key-expiry support.
No existing accounts/keys/credentials/user contents are read or changed. Missing
capability/collision/changed identity refuses; do not guess defaults or repair.

A later exact GO can create only the new test account `effigy531`, password login
disabled, with one private home/authorized_keys file. Docker-group membership is
**root-equivalent inside this selected proof VM** and must be explicit; no sudo
or other VM access. Generate one ephemeral Ed25519 key in exclusively owned
canonical host0700 `/Volumes/Builds/scratch/effigy/531-linux-control`, outside all
qualification/source shares. Never print/commit/load its private key in an
existing agent. Transfer only the public key through authenticated Parallels,
with supported `restrict` and absolute expiry at most24hours/operator lifetime.
Unsupported expiry/login refuses without weakening auth. Pin/reobserve the actual
public host key over Parallels, never trust a network scan/ssh-keyscan as a pin.

The packet now uses `/usr/bin/ssh -F <pinned-private-config>` for **every** Linux
hash, execution and receipt request. No user/global SSH config edit or alternate
transport is needed. `ssh_config` and `ssh_known_hosts` are separate hash-bound
input artifacts, rechecked before each request, owner-only0600 beneath the
unshared0700 control root. No qualification root may overlap that control root.
The proposed alias `effigy-531-vault` is not an observed alias: create only in
that private config after actual endpoint/user/key facts exist.

`common.ssh_prefix` accepts one exact Host, observed numeric HostName, dedicated
User, IdentityFile/UserKnownHostsFile under private control, and only this policy:
BatchMode/StrictHostKeyChecking/IdentitiesOnly yes; IdentityAgent none;
GlobalKnownHostsFile /dev/null; UpdateHostKeys/ForwardAgent/PermitLocalCommand/
CanonicalizeHostname no; ClearAllForwardings yes; RequestTTY no; ProxyCommand/
ProxyJump/RemoteCommand/LocalCommand none; ConnectTimeout10. No Include, Match,
wildcard hosts, duplicate directives or command hooks. Missing/wrong hashes,
unsafe paths/permissions/owners or changed policy refuse before backend effects.
No key/config creation or SSH connection occurs in source validation.

## Lifetime, later builds and gates

Retain VM, account, native placement, host control and metadata under recorded
scope. Expiry limits auth, not deletion or cleanup. Any account/key/mount removal,
shutdown or capacity release needs separate exact disposition; existing services,
accounts/keys and other VMs stay protected.

After #574 and #580 source integration and retained review, separately bind a
numeric-bounded immutable source/archive transfer and actual native build bundle
through approved Effigy selectors, with exact Rust/nextest/Oxide, package/download/
cache/image bounds. Do not infer install authority from missing tools or rebind
e51 receipts. Record command/PID/exits, public fingerprints, actual placement /
source hashes, failures and retained resources outside ephemeral Queue cleanup.
All three backend inputs/GO-index stay **UNARMED** pending actual GOs, accepted
placement and planner-bound aggregate execution. Consumer/install/production/
merge authority is unchanged.

## Setup adapter status (source-only, revision 12)

Pure controls, no effect. Files: `linux_setup.py` (Stage1 entry bound to the registered
home, admission verifiers, setup plan, executor, host credit, `ssh_argv`),
`linux_supervisor.py` (transient SERVICE argv, gate, outside-payload monitor, guest
candidate producer, root entry point), `linux_transfer.py` (bounded local processes, the
contained stdin transfer), `linux_receipts.py` (OpenSSH-signed admission receipts),
`linux-setup-probe.sh` (read-only probe, hash bound), `linux-setup-stages.json` (ledger
checked against the plan), `linux_setup_tests.py` (192 controls, including real-process flows;
`test:qualification:linux-setup`).

Preparation request hashing and Linux guest placement share the explicit
`preparation_requests.REQUIRED_MODULE_NAMES` inventory. It covers the staged
preparation modules and the setup adapter's local imports, including
`linux_disk_identity.py`, `linux_disk_association.py`, transfer and receipt helpers, Stage1, and the
observation caller. Requests hash each exact source file; placement requires
the same exact guest/host map and current byte hashes. The isolated import
control stages only this declared closure, and confirms the setup adapter
cannot import when the disk identity parser is omitted. This closes a source
packaging gap; it does not activate a request or establish any setup gate.

- **Gate and positive path.** The payload is wrapped so it blocks on a FIFO in the stage
  directory before running. The outside monitor samples the unit while it is loaded and
  releases the gate on the first sample that shows it active in a cgroup. The unit is
  therefore provably active, with a control group, before the payload does any work. A
  payload never observed active is never released and ends at its RuntimeMaxSec deadline,
  which is unknown. Candidate eligibility needs a pre-release active sample for this
  InvocationID in one cgroup, no failed read or release sample, and a post-run loaded
  unit with the same InvocationID, active(exited) under RemainAfterExit, Result=success, ExecMainStatus=0 and an empty
  ControlGroup. A populated=1 sample while the unit runs is no veto (K-16). InvocationID is
  parsed as the contiguous hex systemctl prints.
- **Host credit.** The host reads the unit over the VM route and credits closure only when
  the candidate equals that observation (boot ID, nonce, unit, InvocationID, active state,
  result), through the 686 `unit-exited-exittype-cgroup` basis (K-22).
- **Contained transfer.** A transfer row starts the supervisor over pinned SSH (`sudo -n` of the one installed program, request
  inline) and, at once, an SSH writer that waits for the stage FIFO and streams the
  archive or placement into it. The payload reads the FIFO and writes the destination. SSH
  exit is only the writer's precondition (exit 0). Closure comes from the supervisor
  candidate and host credit, and the payload's sha256 line is the expectation.
- **Installed supervisor is standalone (K-15).** `linux_supervisor.py` starts with
  `#!/usr/bin/python3 -I`, imports only the standard library, and mirrors the two
  constants it needs from the preparation modules. A control parses its imports and
  pins the mirrors. The install writes that one file and it runs from its installed
  path. A boot-ID path seam (`EFFIGY_BOOT_ID_PATH`) lets the control run the installed
  file here. It cannot forge credit, because the host compares the candidate's boot ID
  with its own VM-route identity.
- **Supervisor install (E-SUP).** The root row's argv embeds the reviewed
  `linux_supervisor.py` as base64 and writes it with `base64 -d`, then `chmod` and
  `sha256sum`. No staging route is needed. The executor compares the argv with the
  recomputed one before the row runs. The install itself is still a separate GO.
- **Real command flow.** Because systemd is absent here, `systemd-run` and `systemctl`
  are Python stand-ins on PATH. They run the real payload as a real process group,
  keep the unit active while the group lives (ExitType=cgroup, KillMode=control-group),
  enforce RuntimeMaxSec, and print `systemctl show` text. The supervisor, monitor, gate,
  transfer runner and host credit run unmodified against them. These tests show the code
  path works end to end with real processes. They do not verify systemd's InvocationID
  or terminal timing, which still needs a live check.
- **Supervised payloads.** E8 (controls) and E9b (archive extraction and tree digest) run
  as supervised payloads. Their request is inline in argv, so the plan digest covers its
  exact bytes, and each has a row nonce from the facts.
- **Receipts.** GO and admission receipts are OpenSSH signatures (`ssh-keygen -Y verify`,
  namespace `effigy-go-receipt`) checked against an allowed-signers file bound by sha256.
  Each receipt must name its gate, be approved, and be bound to this plan digest. Tampered,
  wrong-signer, unsigned and stale receipts refuse. The trust anchor is an operator key.
- **Stage1.** One start, one systemd 259 `systemctl is-system-running --wait` readiness
  (exactly `running`/exit 0; degraded, unknown, timeout, transport, nonzero or missing
  refuse), one read-only probe on that same boot and VM, one graceful stop, 600 s including
  120 s reserve, no snapshot, no `--kill`. Occurrence identity is required; omitting it
  refuses before any step. A launched start that fails still attempts the graceful stop
  and retains post-state. The request reads the registered home itself.
  Config, descriptor and extent inventory are recomputed from `PVM_HOME`, and the probe
  script must hash to the reviewed value. Host timeout for wait is min(step, remaining
  absolute − reserve) and never extends. A transport or client exit is not VM closure.
  Post-stop, `stage1_verify_post` compares retained config semantically: structure, order,
  attributes and tails stay exact; originals and hashes are preserved. Only source-reviewed
  leaf transitions may change (`Identification/VmUptimeInSeconds` and
  `UsbConnectHistory/USBPort/Timestamp`, both ASCII non-negative integers, later value not
  smaller). Unicode numeric lookalikes and unknown XML encodings refuse. A `NetworkAdapter/VMNetUuid` rewrite is unadmitted: the published Parallels
  NetworkAdapter schema does not define that field or state whether a rewrite is a
  per-session diagnostic or a virtual-network identity change. Identities, descriptor,
  extents, snapshot tree or growth above 1 GiB still refuse. Post-run bytes are retained
  into the stage before any check, and the writable top is selected from the exact
  descriptor bytes, never a file name (K-25).
- **Stage root (revision 8).** Row stages are direct children of the reviewed root
  `/Volumes/Stash/scratch/effigy/linux-4f6a5ed3/stage`. The root is created mode 0711 and
  owned by root. Each row directory is mode 0700 and owned by the payload account. The
  entry refuses any other parent or a name outside `[A-Za-z0-9._-]{1,32}`.
- **Lifetime (revision 8).** Eligibility no longer vetoes a populated=1 sample while the
  unit runs. Only failed reads, failed releases and a post-run state other than loaded,
  inactive, success, status 0 and an empty cgroup stay Unknown.
- **Head and trust binding (revision 8).** The plan digest covers the reviewed head and
  the executable rows. Receipts must name that head and that digest. The EGO rows are
  left out because the receipts sign the digest.
- **Placement map (revision 8).** Every module, driver, GO receipt, GO signature and the
  allowed-signers file needs exactly one placement with a pinned host path and sha256.
  Missing, unexpected, duplicate and mismatched entries each refuse. The B archive (E9c)
  uses the same contained transfer as the H archive (E9a).
- **Revision 9 changes.** Lifecycle: RemainAfterExit=yes stays, so the terminal state is
  active(exited), and the host stops the unit after credit (K-22). Transport: supervised
  payloads reach the root supervisor over pinned SSH through one sudo rule, installed
  by the E-SUDO row (K-23). Anchors: the preparation head, candidate H, probe transcript
  sha and signer pin come from `EXTERNAL_ANCHORS` and never from facts (K-24). Writable
  top (K-25): the file-name derivation is removed. `linux_disk_identity.py` selects the top
  from the descriptor bytes under the Parallels Disk Format below. Stage1 and post-run
  verification re-derive it and refuse any caller observation whose GUID or complete
  result digest differs, so a stale or fabricated value cannot pass and no caller can add a
  mechanism. Custody:
  the stage parent is canonical, held by descriptor, not writable by others, and rows are
  created through it and never reused (K-26). BASE is root-owned 0755; archives sit under
  `inputs/`; the GO index is placed and hash-checked (K-27).
- **Admission.** The executor rebuilds the plan from facts, never from a caller's rows.
  Gates: the probe output is hash-bound (E3 account absence, E4 docker group); GO receipts
  are signed and plan-bound; E6 accepts only one restricted ssh-ed25519 line for the E2 key
  with the bound expiry within 24 hours; disk association and hard limits pass source-
  verified verifiers (none established yet, so they refuse); bootstrap rows match exact
  argv; payload rows need the effigy531 uid observed after E3a and their row nonce.

Decisions for the current adapter:

- The outside-payload monitor observes the gate-held cgroup and InvocationID before
  release. After positive active(exited), success/status zero and empty cgroup, it
  stops the empty service so `systemd-run --pipe` can return. Credit requires the
  same loaded InvocationID inactive/dead, empty cgroup and waited exit zero. A
  collected unit, disconnect, failed read or partial stream remains Unknown.
- The full payload deadline starts at gate release. Gate setup uses 30 seconds of
  admission reserve; cleanup and waited transport use the existing report reserve.
  RuntimeMaxSec includes the gate reserve. Payload bounds stay 900/300 seconds and
  total stages stay within 1200 seconds; polling is never a hard quota.
- E-SUDO installs no general grant. After host GO, identity, credential, resources,
  hard-limit and one-use intent checks, the VM root route installs exact request
  bytes under root-owned0700 `/var/lib/effigy531-approved` and one exact digest sudo
  rule. The SSH root entry accepts only `--approved-request <digest>`, opens that
  root-owned0600 record and spends both its digest and nonce exclusively before effects. Guest supplied
  hashes cannot grant authority. Every root row access uses a held directory fd;
  payloads cannot replace root-owned stage rows or root stream paths. The gate remains
  supervisor-owned0444; only its private, non-inherited pre-opened writer fd can
  release it. The payload can read but cannot write or chmod the gate. Invalid
  release lines and EOF refuse execution. Newly created object modes are set and
  verified through held descriptors, independently of inherited umask (077/027
  covered); existing parent checks are never relaxed.
- The request binds the explicit guest input/hash map, resource counters, SSH file
  hashes, expiry, VM boot identity and payload argv. Host request retention reuses
  the 686 request writer. Placements and streams retain their independent hashes.
- `linux_observation_caller.execute` is the actual caller: bounded VM-route public
  boot/key identity, then pinned authenticated SSH boot plus systemctl terminal
  observation. It retains separate request/stdout/stderr/waited exit evidence.
  Callback interfaces remain injection seams for source controls only. Every SSH
  payload, including E8/E9b, checks the isolated config, private key, pinned host
  key, numeric probe endpoint and expiry before spending its intent.
- Stage1 admission authenticates a separate signed occurrence from the reviewed
  proof-entry activation boundary: nonce/time window, waited one-start/graceful
  stop, probe/identity and all retained before/after bytes and extent inventories.
  Complete post-state enters the signed setup-plan digest. A setup GO cannot
  substitute for an observation receipt. Activation anchors are unset in source.

The lifecycle is source-qualified against [systemd-run v259 implementation](https://raw.githubusercontent.com/systemd/systemd/v259/src/run/run.c),
[service semantics](https://raw.githubusercontent.com/systemd/systemd/v259/man/systemd.service.xml)
and [kill semantics](https://raw.githubusercontent.com/systemd/systemd/v259/man/systemd.kill.xml).
The stand-in now waits in active(exited) until stop, matching the reviewed wait loop.
Installed systemd capabilities and that lifecycle still need fresh native binding.

Writable-top selection is source-implemented as metadata only. Stage1 still refuses before any effect:
no live producer supplies a top observation (`top_observation` is None), and every row stays gated by
`EXTERNAL_ANCHORS`, which is unset in production.

Controller-to-guest association is a fail-closed verifier in `linux_disk_association.py`, with `PINS` empty, so
every association refuses. It has five premises, each checked on its own inputs (`premise_table`): `host_read`
(one pinned read-only accessor, exact tool bytes and Desktop build, bounded output, boot bracket, and the serial
and image UID the output names), `config` (exactly one hdd0 at SATA index 0 in the reviewed VM; CD-ROMs counted,
not disks), `descriptor` (selected top, chain and every extent, from the descriptor bytes), `guest_blocks` (exactly
one whole disk, `/dev/sda`, with the reviewed root partition on it; loop and ROM classified by type, anything
else refuses) and `serial` (host read serial equals the guest parent serial).

The upstream candidate, `PrlVmDevHd_GetSerialNumber`, is used by libvirt's Virtuozzo driver. It is not exported by the
installed Parallels Desktop 27.1.0 SDK (`libprl_sdk.11.dylib`, sha256 `464af6b9…`); its only serial accessors are
the VM SMBIOS serial and serial ports. So no pinned host read exists. Retained 748 facts establish the config and
guest-block premises and descriptor self-consistency, not the link: the config device Uuid `{9447f202…}` differs
from the descriptor disk UID `{44b5d35e…}`, and no documented rule joins them. Guest `ata1` is not mapped to SATA
position 0, and the guest sysfs serial attribute is absent while the probe reports the parent serial.

Still unavailable: source-verified kernel hard limits, fresh signed Stage1 evidence, exact activation
anchors and root installation authorization. No VM, SSH, account, service, build,
proof or production effect ran here. E10/E13k/E14 stay held; measurement692 grants
no longer stage. Source controls confer no native proof credit.


### Diagnostic observation preparation

The separate `qualification:linux:diagnostic-proof-only` entry prepares an observation handoff, not setup or placement admission. A future execution requires an actual Nucleus proof-only launch at the clean pinned preparation head, after a native operator GO; this source work grants no GO. The entry cannot authenticate Nucleus provenance against another process of the same user. A request or caller environment alone is not authority.

A prospective request is printed by `effigy qualification:linux:diagnostic-proof-only -- plan ROOT ACTIVATION_ID HEAD`. Provision an owned canonical 0700 root under Stash scratch first. Save the exact JSON as an owned read-only request, hash it, and bind those bytes and inputs to the later Nucleus launch. The proof selector accepts only that request path and hash. It claims the activation exclusively, keeps a 600-second absolute budget including a 120-second report reserve, and records command streams and waited exits. Before the fixed read-only probe or writer-bound routes, it first proves guest transport with one fixed bounded read-only echo sentinel (`/bin/echo linux-diagnostic-transport-ready`): only the exact sentinel with a waited exit 0 and empty stderr passes, and a wrong or partial sentinel, a timeout, another stream or any unrecognised error refuses. The only retryable answer is the exact Parallels session refusal observed at step 005 of the 745 occurrence — empty stdout, a nonzero waited exit and the `Unable to open new session in this virtual machine…` stderr — and only while the same readiness window remains; every attempt's step, waited exit and both streams stay retained. The readiness end is derived once before transport, and transport retries, the one `/usr/bin/systemctl is-system-running --wait` wait and the first boot-ID read share it inside the existing occurrence and reserve; no retry, extra start or restarted deadline extends it. Only exact `running` stdout and a waited exit 0 pass systemd readiness. The probe must independently report `system_state=running` and the same valid boot ID observed after readiness. Fixed boot-ID reads bracket the entire writer-route window; a missing, malformed or changed identity keeps the raw observations but makes that occurrence incomplete. Neither a successful transport nor the observation receipt qualifies Linux readiness.

The Parallels program is the bundle CLI `/Applications/Parallels Desktop.app/Contents/MacOS/prlctl`, pinned to SHA-256 `2f61a1c6…`, and the fixed route launches it directly with that bundle path as `argv[0]`. `/usr/local/bin/prlctl` is a symlink to the vendor `parallels_wrapper` (SHA-256 `f3148009…`). The wrapper picks its tool from `basename($0)` and exits 9 for its own name, so the canonical target cannot select `prlctl`; the previous plan launched that realpath and never reached a VM command. The direct route does not run the wrapper's service bootstrap (`inittool2` init, Parallels Service launchd start). The Parallels Desktop dispatcher must already be running. If it is not, the pre-observation `list` fails closed before any start, and the one-use claim is spent. The Stage1 rows in `linux_setup.py` still name the alias and need the same route decision before any setup admission.

Checkout-owned sources and `config/tasks.toml` are identified by repository-relative role and exact SHA-256, so the request does not carry the path of the checkout that printed it. A clean detached checkout at the same head resolves those roles against its own files and refuses a wrong head, dirty tree, changed, missing or extra input, or alias. Machine tools (the Parallels CLI, interpreter, `/usr/bin/git` and installed Effigy) keep canonical paths and hashes. Operator authority for proof-only stages is recorded in [release.md](../../knowledge/contracts/release.md#qualification-sequence-proof-only-authority--2026-11-08). Each launch receives the resolved actual paths and hashes, and each step's `intent.json` retains them. Version-1 requests are refused.

The observation retains pre- and post-config and descriptor bytes independently and accounts all descriptor extents without naming a writable top. It separates stable ownership objects from ancillary metadata. The package and disk roots, and every extent the descriptor names, stay identity-bound and inventory-checked: a changed root, a replaced or aliased extent, a missing descriptor or a changed descriptor *content* is a refusal. Every other regular file in the package is an ancillary metadata object that Parallels rewrites, replaces atomically (same path, new device/inode) or rotates through temporary names while its VM runs, so it is sampled rather than filtered. The diagnostic entry samples it in a narrow read-only inventory mode: the kernel's stat of a regular non-aliased object supplies its logical and allocated bytes and an observed owner, and grants that object no executable, input, config, extent or ownership authority. The entry is never read, never repaired and never promoted, no filename is whitelisted and no owner is assumed, and a different owner proves neither an unauthorised effect nor its actor. The charge is one-sided: an incarnation is charged its greatest observed logical and allocated size, a replacement keeps the previous incarnation's whole charge, and a disappearance refunds nothing, so the observed cap is approached from above rather than refunded. Every identity, counter and owner event (path, device, inode, uid, custody, both counter observations) is retained, and an owner change at a path that kept its identity is its own observation. Symlinks, hardlinks or aliases, non-regular entries, unreadable entries or counters, an entry on another device and any bound-input owner mismatch remain refusals: the config, the descriptor and each extent keep strict custody in every mode, and every other caller keeps the strict default, in which a foreign ancillary owner is also a refusal. A refusal keeps the complete observed stat identity before it refuses — canonical relative path, device, inode, uid, mode, link count, type, logical and allocated bytes, phase and errno — so a failure never discards the fact that explains it; absent facts stay `null` rather than a substituted zero. Each live sample verifies the descriptor (lstat, then its bytes) and every extent once; the package walk accepts those protected entries by listed kind and inode rather than a second pathname lookup, so a descriptor that is present in the sample is not refused by its own walk. A listing or inode that moves under a sample repeats that sample once; a persistent failure refuses, and a definitive custody refusal is recorded once. Every failed attempt is retained with its phase, operation, path and errno. This is sampled accounting with a sampled ceiling, not a hard quota; per-incarnation charges also mean repeated rotation of the same path counts each incarnation, and a path that returns to an earlier device/inode is charged again because a sample cannot tell an atomically replaced file from a reused inode number.

Retained content hashes and metadata identities are reported separately. A descriptor replaced with identical bytes is an identity event, not a content-change claim; a config whose bytes changed (for example the network adapter UUID Parallels rewrites on boot) is recorded as a diagnostic observation and is not setup compatibility. A complete positively executed start, probe and graceful stop may therefore deliver a bounded diagnostic handoff with recorded nonfatal metadata differences. Bound-input ownership, source or input drift, snapshot or extent inventory, unreadable counters, growth beyond the sampled ceiling and command or terminal failures stay fatal, and no difference or observed ancillary owner grants setup, placement, association or hard-quota credit. Writable-top, guest association and readiness remain unknown.

Once a start launcher is recorded as spawned for the verified VM, cleanup observes that same VM's own state inside the existing operational deadline instead of sending a stop while Parallels still reports `starting` (which is refused and closes nothing). It waits in a bounded window for eligible `running` state before the one graceful stop; a VM already observed `stopped` is recorded without claiming any stop command and is reported as a finding, because a successful handoff needs a positively executed graceful stop; an unobserved or non-terminal state stays unknown and no stop is sent. A later `stopped` observation establishes terminal state only and never repairs a timed-out or killed stop client. Launcher exit alone does not establish VM state. If the operational budget cannot fit the windows, the limitation is recorded and the retained originals still run. The separately signed fresh Stage1/setup admission and its supported-mechanism gates remain required and unchanged.

The same boot-complete occurrence also runs seven fixed, read-only writer-bound capability reads inside the one 600-second budget: the sysfs block inventory (`/sys/class/block`, every disk and partition with major:minor, parent and its `/sys/devices` controller path), `lsblk`'s documented JSON inventory as an independent check, `/proc/self/mountinfo` for the root mount source, filesystem, mount options and device major:minor, `dumpe2fs -h` for the documented ext2/3/4 feature set (project and quota included), `/dev/disk/by-uuid` and `by-partuuid` resolution for the root device, `command -v` plus each tool's documented version flag (for example `-V` for `dumpe2fs`) for the documented tools including the read-only quota tools, with the version banner read from stdout or, for tools such as `dumpe2fs` that print it there, from stderr and labelled with that stream, accepted only when that same invocation succeeded and its first line is the tool's own banner; a refused option, usage text or any other line records `version <tool> unavailable`, and the cgroup-v2 controller files with `systemctl show` properties of `system.slice` and `user.slice` (which starts no unit): `MemoryMax`, `MemoryHigh`, `MemorySwapMax`, `TasksMax`, `CPUWeight` and the per-device `IOReadBandwidthMax`, `IOWriteBandwidthMax`, `IOReadIOPSMax` and `IOWriteIOPSMax` lists. `linux_diagnostic_probe.py` holds the exact argv and decoding schema and is part of the hashed complete input inventory; every original is retained as `observation-<route>.raw` beside the outcome with its SHA-256 and waited exit in `guest_writer_bound.routes`. Each read has exactly one status: `observed`, `absent`, `unsupported`, `not-found`, `ambiguous`, `failed`, `malformed` or `not-attempted`. Absence, an unsupported invocation or filesystem, a missing inventory entry and an ambiguous mapper are negative observations of this guest and stay facts; a failed, malformed, oversize, truncated or skipped read is a finding, so an incomplete read leaves the diagnostic findings while complete originals and post-state are retained. A missing fact never becomes a positive observation.

The block inventory records a partition's `removable` as `na`, not as a value: the kernel exposes that attribute on whole disks only (`block/genhd.c` `disk_attrs`; `block/partitions/core.c` `part_attrs` has none), and a parent's property is not a child's. A whole-disk flag that is missing or malformed still refuses the whole inventory. An inventory that is unavailable, malformed or failed leaves the root ancestry `unknown`; only an observed inventory that lacks the root device reads `ambiguous`. Invalid or unobserved cgroup properties are visible findings, so they keep the observation incomplete. The earlier script's partition token `invalid` is accepted only by the legacy replay selector, which records it as `unavailable`; the live route never accepts it.

These are observations, not bounds. `guest_writer_bound.limits` states what the facts cannot establish: `MemoryMax` is a byte bound of one control group and `IOWriteBandwidthMax` (the brief's `IOWriteMax` shorthand) a per-device write rate in bytes per second, so neither is an aggregate allocation; the IOPS properties are per-device rates too; `RLIMIT_FSIZE`, sampled counters and post-growth checks are not a filesystem allocation quota. `SUPPORTED_HARD_LIMITS`, `SUPPORTED_DISK_MECHANISMS`, `EXTERNAL_ANCHORS`, the E8/E9 setup gates and writable-top, readiness, placement and guest-association stay unchanged and unset. A root kernel name may match this guest's history and a serial or size may look alike, but no host/guest serial equality, association or controller proof is inferred: additional disks and partitions stay listed, and a device-mapper or otherwise virtual root is recorded as an ambiguous controller rather than resolved by name. An ambiguous or unresolved root, a cross-inventory disagreement or any failed, malformed or skipped read makes the receipt `complete: false` and `not eligible` with the reason in `findings`; only a resolved, fully observed occurrence can read as eligible for a later reviewed correlation mechanism. The retained 719 evidence stays historical: it carries the earlier probe fields, no writer-bound reads, and is never upgraded to these facts.

### Allocation bound (source-only, unadmitted)

`linux_allocation_bound.py` checks candidate observations for a potential retained-allocation mechanism on the native root: ext4 project quota (`quota` and `project` features, one reserved project id on the stage tree `BASE`, inherited). A hard project limit can bound charged allocation only when enforcement, inheritance and writer custody are established. The current witness does not establish those facts and cannot authorize a native run. The verifier takes five route originals (`mountinfo`, `dumpe2fs -h`, `by-uuid`, the fixed `quota` helper output with Q_GETFMT and Q_GETQUOTA, and the fixed `tree` helper output with lstat and FS_IOC_FSGETXATTR per entry). It checks each original's hash and exit, then matches them against exact request facts: boot id, owned uid, `BASE` and the exact entry set. Accepted witnesses must show a writable native ext4 root, a quota record whose `valid` bits mark both block limit and space usage, a project limit with headroom exactly equal to the row reservation, one device on every entry, the project id on every directory and file, and project inheritance on every directory, with no hardlinked file, symlink or foreign object. Root-owned directories must not be writable by group or others; other entries must match the payload owner. The supervisor's gate and transfer FIFOs under `BASE/stage` are admitted as blockless single-link entries owned by the payload uid or root: they allocate nothing, so they add no charge, and their project id is not read because opening a FIFO can block. Kernel charge must be at least the tree's bytes. `route_plan` binds the fixed argv and the helper's SHA-256; nothing here runs them.

Observed 2026-11-08 (747 originals, `observation-filesystem.raw`): `/dev/sda2` ext4 UUID `d488be7b-…`, mounted `rw,relatime` with `user_xattr,acl`, features without `project` or `quota`, and no `repquota`, `quota`, `quotacheck`, `quotaon` or `edquota`. That is absence. A test feeds the retained features line in and expects refusal (`lacks project,quota; enforcement is not provisioned`). Production admits nothing: `SUPPORTED_HARD_LIMITS` stays empty, and `verify_hard_limit` delegates the new mechanism only when it is admitted and its facts are complete.

What the mechanism bounds, per row. The earlier wording "≤ N MiB writes" is not one metric, so each row is qualified by what it can actually bound:

| Row | Written wording | Metric this mechanism can bound | Not bounded here |
| --- | --- | --- | --- |
| E8 | ≤ 60 s, ≤ 64 MiB writes | retained allocation ≤ 64 MiB in `BASE` | cumulative writes: overwrites and delete-and-rewrite allocate nothing new |
| E9a / E9c | archive ≤ 40 MiB | retained archive bytes in `BASE/inputs` | the transfer stream itself; partial output past EDQUOT is a failed transfer, not a success |
| E9b | extracted ≤ 500 MiB | retained extracted tree in `BASE/source` | temporary create-and-delete traffic during extraction |
| E19 / EGO | E19 placements ≤ 1 MiB in total; existing transfer ceiling ≤ 40 MiB per row | retained placement bytes | the aggregate E19 ceiling still needs its own reservation |

A per-file `RLIMIT_FSIZE`, a sampled counter, a post-growth census, `MemoryMax` and `IOWriteBandwidthMax` are not this bound. The agreed metric is retained allocation; cumulative I/O volume has no admitted mechanism.

Residual writers outside the project. Objects created under an inheriting `BASE` can be charged, but charge is not proof that a privileged writer is stopped at the hard limit. Writes outside `BASE` are not bounded by this mechanism: root-owned approval records under `/var/lib/effigy531-approved`, supervisor records, the fixed-size drop-in and sudoers files, and everything E12 writes to Docker storage (pull ≤ 1 GiB, build, engine disk ≤ 2 GiB) or apt. Each needs its own finite bound before its row can rely on this mechanism. The current source does not establish one for the approval record or supervisor records, and E12 stays held.

#### Source evidence and current refusal

The 747 retained capability original reports `dumpe2fs 1.47.2`, but does not establish the installed `tune2fs` version. The upstream [e2fsprogs v1.47.2 tune2fs source](https://git.kernel.org/pub/scm/fs/ext2/e2fsprogs.git/plain/misc/tune2fs.c?h=v1.47.2) (SHA-256 `ca47af75c719871352d74cb3e2ee1443893b0c6b11d6a7b7e9edf1eda7af55e8`) enables project quota when enabling the project feature and rejects an inode size too small for it. This source is not proof of the installed binary's version or capabilities. A future route must freshly verify both, and verify the actual enforcement separately from accounting and feature bits.

For that future check, the fixed source-only plan uses `tune2fs -l /dev/sda2` and `e2fsck -V`. The v1.47.2 `tune2fs` source prints its banner on stdout, starts with a read-only open for `-l`, and has no `-V` option; the [e2fsck v1.47.2 source](https://git.kernel.org/pub/scm/fs/ext2/e2fsprogs.git/plain/e2fsck/unix.c?h=v1.47.2) (SHA-256 `b13559bf6143b9efb2fa2cfe61d0c850936f4e9f0c6f019aaa8dac5e64b4ea0b`) prints its version on stderr. The candidate guard checks both original streams, their hashes, positive waited exits and the exact 1.47.2 banner; missing or other versions refuse. It does not execute either tool, authenticate its binary or device, or prove that the binary implements the upstream source.

A concrete counterexample exists in upstream Linux v6.1: [VFS ioctl source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/fs/ioctl.c?h=v6.1) (SHA-256 `1fd4674ab14661087f3649178357643925b0e963ca3ff9e3becb73f73054e443`) permits an inode owner to reach the file-attribute setter, and [ext4 ioctl source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/fs/ext4/ioctl.c?h=v6.1) (SHA-256 `5eff7348ae4e4f8770b870989665cac9e26fb8f65824069dab43b3b1e03aa297`) changes an inode's project and transfers its quota charge. Thus an owning writer can attempt to move its inode out of the intended project; a pre-run snapshot of project IDs cannot bound its later writes. This source is a counterexample to the unqualified claim, not an observation of the guest kernel.

The candidate uses the generic ext4 `Q_SETQUOTA` operation (not XFS's `Q_XSETQLIM`), `PRJQUOTA`, and `QIF_BLIMITS`. The [Linux v6.1 quota ABI](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/include/uapi/linux/quota.h?h=v6.1) has SHA-256 `51e02a6f0ec05a3688362e7cfabf20c032709e6dbb43580e488c0dff8d5a4e80`; the [v6.1 syscall implementation](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/fs/quota/quota.c?h=v6.1) has SHA-256 `80f0c29d61d0e39812a898cc03ce42c22ece9000cf238d693f65c6a41de82dfc`. They identify the operation, 1 KiB limit units, valid bits and privilege check. The candidate rejects a non-64-bit pointer, an `if_dqblk` size other than 72 bytes, or a valid-field offset other than 64. These checks do not establish the guest's architecture, ABI, kernel version or permission to invoke it. No privileged entry is wired; a future route must independently bind these facts and its waited child exit before and after setting a limit.

The [Linux v6.1 ext4 mount source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/fs/ext4/super.c?h=v6.1) (SHA-256 `41ab8712409b13c7c32ed99e60a2b7b1d108238158031dcae6b7c18538949d6b`) enables usage tracking for quota inodes separately from limit enforcement, which depends on the corresponding quota mount option. This disproves the earlier assumption that a nonzero `Q_GETFMT` by itself shows an enforced project limit. Guest kernel version and effective enforcement are not established by this source; do not infer them from it.

The [Linux v6.1 quota source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/fs/quota/dquot.c?h=v6.1) (SHA-256 `dfeea00c6f36478b5ca0db480e452cc7c93798e0cf6cc8ca1a3dbbe27c3befc6`) allows a process with `CAP_SYS_RESOURCE` to bypass hard-limit checks in the normal quota format. Root supervisor writes therefore need their own finite allocation and custody proof even inside `BASE`; charging them to the project is insufficient. The guest kernel, quota format and supervisor capability set remain unobserved.

The upstream [Linux file attribute source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/fs/file_attr.c) observed with SHA-256 `08a0b9a207ffc650b98aa9e0a11530fecf8fc41c4d60e65fc2ad1e9c2881de7d` permits the owner in the initial user namespace to request project attribute changes and includes a `file_setattr` syscall in addition to ioctl routes. This moving upstream source does not establish the guest kernel's version or behavior. The current supervised child has no restriction proving those changes impossible, and other same-UID processes are outside its boundary. Therefore the verifier refuses even matching pre-run originals: the current mechanism is not a hard-bound proof. Production remains unadmitted. A fixed child restriction and exclusive writer custody, or a separately reviewed refusal, are required before any native activation can be proposed as ready.

A narrow candidate child restriction is systemd `PrivateUsers=identity` with `NoNewPrivileges=yes`, installed before the gate releases the payload. The [systemd 259.6 execution manual](https://www.freedesktop.org/software/systemd/man/259/systemd.exec.html) observed with SHA-256 `f51d96b6acacb11f87a2bb0136d22d002365d3dae5fe6c120ca1f77cc3277a80` describes an identity mapping for the first 65,536 UIDs/GIDs and no capabilities in the host user namespace. The retained Stage1 reported systemd 259, not the exact manual build. The v6.1 VFS source above rejects project ID and inheritance-flag changes outside the initial user namespace. The [user-namespace manual source](https://git.kernel.org/pub/scm/docs/man-pages/man-pages.git/plain/man/man7/user_namespaces.7?h=man-pages-6.13) (SHA-256 `af283c81488dbc3b6e95151aa1d9c66bffd7e2a638d40a48fdc82835a8eeddd3`) describes fork/clone inheritance and requires capability in the target namespace for `setns`; [v6.1 credential source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/kernel/cred.c?h=v6.1) (SHA-256 `3e2ae98b698262d0d74ac12d13da7995a0882123a0fea776382ddc69623b2933`) carries the user namespace through exec credential preparation. [v6.1 no-new-privileges source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/Documentation/userspace-api/no_new_privs.rst?h=v6.1) (SHA-256 `73cd16bb37fab2678c891dd7d46bd5ea08a89b8534524352ed25f816e7c0da1c`) describes its inheritance across fork, clone and exec.

This is source qualification, not an installed restriction or proof of the guest kernel. A private, unarmed argv builder fixes both properties before the payload separator; it does not launch a service. A fixed implementation must refuse an unsupported systemd/kernel or UID outside the mapped range, fail before gate release when installation or namespace observation is unknown, and bind descendants and positive closure. It must also prevent delegation through privileged sockets and account for the separate same-UID writer. Neither an asserted property nor an operator approval establishes those facts. The candidate has not been wired into the supervisor and the production quota gate remains closed.

The accepted E6 key contract permits `restrict` and expiry only; its source control also refuses a `command=` option. In the [OpenSSH 9.9p2 manual source](https://github.com/openssh/openssh-portable/blob/V_9_9_P2/sshd.8) (SHA-256 `effa773e96d537f09df9ff54925a4d6f5cf7193b1e8667898f77f96290992e9c`), `restrict` disables forwarding, PTY and user rc, while a separate `command=` option forces the command. Thus an otherwise permitted authenticated session can be outside the confined unit; the fixed child properties do not cover it. The installed SSH version, effective server `ForceCommand` and existence of a second session are unobserved. This is a source counterexample to assuming exclusive custody, not a claim that an escape occurred. Changing the existing key contract or proving a different exclusive channel needs separate review.

#### Prospective native-root route (not ready for an operator decision)

No snapshot, boot, filesystem change or restore is authorized by this document. The retained 747 observation is historical; the VM identity, snapshot/extent inventory, free space, filesystem identity, boot and tool capabilities must be bound again before any later action. The existing native-root placement remains the only route considered here.

| Stage | Known and missing prerequisites | Effect, ceiling and interruption |
| --- | --- | --- |
| Quiesce and retain | VM `433c9e5b-c7b9-469f-9633-24b4addcd3b5`, root UUID and 128 GiB disk were observed; fresh ownership, complete extent/snapshot identities, available space and a one-use snapshot boundary are missing. | Stop all writers and retain originals before a single snapshot; no snapshot or space ceiling has yet been established. Do not proceed on unknown identity, space or completion. |
| Offline check and feature change | A trusted rescue boot capable of keeping `/dev/sda2` unmounted, with verified tool provenance and a way back to normal boot, is not established. Obtain and verify that specific capability under a separately bounded acquisition plan before proposing any boot. `dumpe2fs 1.47.2` was observed; the `tune2fs` and `e2fsck` versions and supported options remain unobserved. | Offline checks and feature changes can modify filesystem metadata and repairs can change data. The repair scope, space and time ceilings are not established. An interruption or nonzero/unknown exit leaves the filesystem unqualified; do not resume automatically. |
| Normal boot, project and row limits | Fixed project assignment, quota setter, enforcement observation, escape-resistant child and exclusive writer custody are missing. Feature bits or `Q_GETFMT` alone do not prove enforcement. | Set and independently observe the fixed project and per-row hard limit only through a separately reviewed one-use route. A full quota yields a failed operation, not success. Each row must retain its original deadline and reservation. |
| Closure or rollback | Positive waited exits, independent originals, exact object set and terminal closure are required. A snapshot identity and rollback procedure are not yet bound. | Retain pre/post originals and the snapshot. Restoring it would lose changes since the snapshot; never assume that is harmless. Unknown, interrupted or failed steps remain held for a separate recovery decision. |

The single missing rescue prerequisite is a verified, architecture-compatible boot medium that can access the identified root offline, contains the qualified e2fsprogs tools, and has an observed path back to the normal VM boot configuration. A future acquisition task may inspect at most three official vendor metadata sets, select one compatible candidate, and acquire at most 2 GiB within 600 seconds into an owned fixture after separate authorization. It must retain the publisher's signing-key provenance, signed manifest, image hash, size, tool versions and boot/return instructions; a missing or failed check stops acquisition. No image is selected, downloaded or booted here, and this plan does not establish that a suitable medium exists.

For a future verified v1.47.2 toolset, the candidate offline sequence is `/sbin/e2fsck -f -n /dev/sda2`, `/sbin/tune2fs -O project -Q prjquota /dev/sda2`, `/sbin/tune2fs -E mount_opts=prjquota /dev/sda2`, then `/sbin/e2fsck -f -n /dev/sda2`, with the root unmounted throughout. The extended mount-option command replaces the whole stored option string: it is permitted only if a fresh, complete v1.47.2 `dumpe2fs` original proves that string empty; a nonempty or unknown string refuses. It does not replace the separate default mount-option bits. The [v1.47.2 superblock printer source](https://git.kernel.org/pub/scm/fs/ext2/e2fsprogs.git/plain/lib/e2p/ls.c?h=v1.47.2) has SHA-256 `692a03d286e7a2e7349af6568e3d582f74ed13b353dbf75961682fe2753f2f1d` and prints the extended option line only when nonempty. The historical 747 filesystem original, SHA-256 `27dcdc02341f7a890e32f2eb9066d0fdf3f4ed5bf8e461d968e5a21c09cc3b41`, has no such line; that is not a fresh precondition. Boot arguments or later mounts may override stored options, and the v6.1 kernel may ignore an unparseable stored string, so neither the command exit nor the string alone proves enforcement.

The prospective pre-state additionally requires the complete 1,024-byte primary superblock from byte offset 1,024 of the independently bound device. Its 64 stored mount-option bytes at offsets `0x200..0x23f` must all be zero, not merely have a leading NUL; the UUID, clean state, 256-byte inode size and absent project/quota features and project quota inode must match too. The [v1.47.2 ext2fs layout](https://git.kernel.org/pub/scm/fs/ext2/e2fsprogs.git/plain/lib/ext2fs/ext2_fs.h?h=v1.47.2) has SHA-256 `6acb20acf7ae2c811b3526ff8cd38b0b79ec185bacdd9471b44645b8673b48bc`. The source-only parser checks these fields against fixture bytes; it neither reads the device nor verifies the superblock checksum, and it grants no authority. A later fixed read must retain the original and hash, authenticate the device and checksum independently, and refuse incomplete or unknown results.

The [v1.47.2 e2fsck manual source](https://git.kernel.org/pub/scm/fs/ext2/e2fsprogs.git/plain/e2fsck/e2fsck.8.in?h=v1.47.2) has SHA-256 `b1579b176628725c956bb0f6c63c6f2bf60902df43817c705bcddfe636115461` and specifies `-n` as read-only with negative answers. Proceed only after a positive waited exit of zero at each step; do not authorize repair or retry on failure or unknown. `tune2fs` can require a freshly checked filesystem, which a read-only check may not establish; if it refuses, stop rather than run repair. Even a failed or interrupted feature change may have written metadata. A separately reviewed boot configuration and positive enforcement observation remain necessary.

The v1.47.2 `tune2fs` source calls quota usage computation without checking its return value. Its [quota computation source](https://git.kernel.org/pub/scm/fs/ext2/e2fsprogs.git/plain/lib/support/mkquota.c?h=v1.47.2), SHA-256 `81f9bcec788b67384b92577ac8a36738c320570e985a2aad9619883cc3caaeeb`, can return errors while opening or scanning inodes. A zero `tune2fs` exit is therefore not proof that the full quota scan succeeded. Retain complete stderr and require a separately source-qualified, independent accounting check before any limit can be credited; this check is not yet implemented.

The fixed source-only operation plan and progress guard check exact command order, one rescue-boot identity, both hashed streams and a positive waited exit for every recorded attempt. A started step without its completion, an altered command or a changed boot refuses; no repair or retry is added. Supplied records alone do not establish that every attempt was recorded, that the root is unmounted, or that the snapshot is available. Those require independently authenticated, durable one-use custody before any future operation.

The prospective project assignment must occur after E7b creates root-owned `BASE` and before E7c creates its 11 subdirectories: one held `BASE` directory descriptor on the freshly bound device, with an exact empty object set and an independently established unused project 531. The existing plan has no operation at that boundary, so it cannot currently perform this route. Read its original `FS_IOC_FSGETXATTR`, preserve unrelated fields, set project 531 and `FS_XFLAG_PROJINHERIT` using `FS_IOC_FSSETXATTR`, and retain an independent post-read. The [Linux v6.1 filesystem ABI source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/include/uapi/linux/fs.h?h=v6.1) has SHA-256 `ebeb4fb4685a1b202a0581c32748df216f5b90a173d8324a0b25910fbeef47c2`. A pure candidate builds those fixed attribute bytes only from a project-zero, non-inheriting original; it does not invoke the ioctl. No recursive `chattr` or project assignment entry is wired. The absence of the feature alone does not prove project 531 is unused; that proof and descriptor custody are still missing.

A source-only writer-tool entry now reads only the five fixed executable paths, resolves canonical targets, checks root-owned non-writable path custody and a bounded regular executable, and records identity and SHA-256. The fixed route binds the entry and its local dependencies; independent before/after originals must match exactly. This is not an admission: actual guest tool identities, the outer SSH login shell and its loader/environment are still unobserved, no immutable tool anchors are present, and no production call is wired. The entry cannot prove all incidental system allocation or control other same-UID processes. A later bound preparation route must collect and retain originals under the same absolute deadline, with positive waited exits, before evaluating the writer's regular-file output boundary.

A source-only broker-child candidate sets no-new-privileges, clears ambient capabilities, removes `CAP_SYS_RESOURCE` from its effective, permitted and inheritable sets, and re-reads all of those facts before any proposed allocating write. The [Linux v6.1 capability ABI](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/include/uapi/linux/capability.h?h=v6.1) has SHA-256 `6c7bea828893cc9a10b0e4ca67643f0fcbdd40ae79319018c57908e080e36424`, and the [prctl ABI](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/include/uapi/linux/prctl.h?h=v6.1) has SHA-256 `fa548afed1d7593f53aedd2ba1a849caedef7856167d1d1da0f8d93487a32698`. The [v6.1 capability exec source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/security/commoncap.c?h=v6.1), SHA-256 `1adc9b772a8ee45551d782d740e037244a448c6bcf1d50327df3eea2774ebf2b`, intersects new permitted capabilities with the prior set under no-new-privileges. Source controls inject capability calls; no real capability change is performed here. A source-only fixed child body checks one of four record names, the held directory owner/device and project inheritance, drops and rechecks that capability, rechecks the directory, and only then creates the exclusive bounded record. Its fixed candidate launcher uses one child, the caller's absolute deadline and cleanup reserve, and a positive waited exit. On timeout it kills only that child and attempts a bounded reap; an unreaped PID is retained for reconciliation and never grants closure. Controls use fixture descriptors and injected capability calls; no guest child was run, no production entry is wired, and these controls supply no authority. All earlier allocating opens including directory and FIFO creation, authenticated project custody, charged metadata and the separate one-time installer remain unproved and held.

The [Linux v6.1 ext4 inode-allocation source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/fs/ext4/ialloc.c?h=v6.1), SHA-256 `3fd3159053a8cb89e74a792817c064fd0d290fd08c29ab2a0d62092c4c2c0d2b`, assigns a new inode its parent's project when the project feature and inheritance flag are present. That source supports the candidate's ordering; it does not establish the actual guest kernel or account for directories and FIFOs created earlier by the parent supervisor.

The fixed approval installer now refuses a request larger than 1,048,576 encoded bytes before creating anything; the existing consumer already enforces that read ceiling and root-owned mode-0700 directory, mode-0600 single-link record, hash and one-use markers. For one fixed supervisor request, the approval record is at most 1,048,576 logical bytes, two spent markers are 64 bytes each, and the fixed supervisor sudoers rule is 181 bytes. Its exact command invokes `/usr/bin/python3 -I -B` so the supervisor does not write bytecode caches; interpreter and loader custody remain unproved. The supervisor now refuses any of its four row records (`intent.json`, `stdout`, `stderr`, `supervisor.json`) over 1,048,576 bytes before creating that record; after a start, a write refusal attempts to stop the exact unit and cannot credit closure. Together those fixed files total at most 5,243,189 logical bytes per request. The source now requires the exact nine setup GO receipts; its fixed module/driver map and GO map allow at most 47 supervised or transfer requests, for at most 246,429,883 logical bytes across those records in one complete plan. These are file-content ceilings, not yet a bound on filesystem blocks or directory metadata. The one-time supervisor installation, bootstrap records and incidental service writes need separate custody and allocation evidence. Until those allocation and custody facts are proved, these root-owned writes remain outside the row quota and held.

The retained E8 payload contract is 60 seconds. The setup row and its supervised request now both carry 60 seconds, with the existing gate and cleanup reserves separate; quota and writer-custody refusals still hold E8. Any future setter route must use the same absolute budget and reserves rather than extending that clock.

The global 1 GiB proposal was insufficient to enforce an individual row. The source-only candidate constructs fixed E8 and E9 row reservations under the 1 GiB project ceiling and requires exactly the row's remaining quota headroom. The prospective request validator accepts only a fixed row, nonce and boot UUID; it cannot choose a device, project or limit and grants no authority. The private candidate syscall function fixes the device, project and block-limit fields and has only an injected call; no command entry or production authority invokes it. The controls inject a fake kernel call, and a zero syscall result is not enforcement proof. E19 remains refused until its aggregate reservation is fixed. The arithmetic cannot alone prevent baseline deletion, project reassignment or another writer. E12's engine, apt and downloads remain outside this quota and held separately. Root-owned approval and broker records outside `BASE` still need an exact finite write and custody bound. The smallest unresolved design choice is how to prove both that every payload descendant is prevented from changing project attributes and that no other process with the payload UID can write during the row. A child-only filter cannot prove the latter. The fixed plan adds `effigy531` to the Docker group in E4, and its accepted SSH key uses `restrict` without a forced command; the contained-transfer writer is another same-UID SSH process outside the supervised unit. Its fixed shell waits for the root-owned row FIFO and writes to that FIFO. The candidate command clears the inner environment and uses fixed `/bin/sleep` and `/bin/cat` paths, but the source does not yet bind their canonical identities, the outer SSH shell, dynamic loader or their custody. It does not establish a zero-allocation writer. These source facts do not prove an actual escape occurred, but they prevent the child filter alone from proving exclusive writer custody or denying every privileged broker. A reviewed design must bind the payload's capabilities and prevent or account for those other channels throughout the row; broad account, SSH or Docker changes are outside this source-only extension. The existing entry does not establish either, so the source refuses instead of selecting an unreviewed isolation or placement change.

After those gaps, the rescue prerequisite and the exact snapshot space and interruption bounds have independent source review, the material decision would be whether to quiesce this specific VM, retain one identified snapshot, change the native root's project/quota metadata and boot configuration for enforcement, and accept that restoring the snapshot loses later changes; the alternative is to leave the native root unchanged. No such decision is requested by this source-only work.

## Writable top selection (K-25, source-only)

Primary source: QEMU "Parallels Disk Format" spec, pinned at QEMU tag `v7.2.9` (commit `385095e48acabb2c2416951e2f25fdb7b638a579`,
GitLab `qemu-project/qemu`; GitHub mirror `qemu/qemu`). File `docs/interop/prl-xml.txt`, content sha256
`9ee40bd00979750e0e8569b0e224bebeed5e73a49d1a873e31bfe38a7ece91bc` (6942 bytes), read in full on 2026-11-08.
Secondary, master: `https://www.qemu.org/docs/master/interop/prl-xml.html` (QEMU 11.1.50 docs), rendered-text
sha256 `33036524…`, HTML sha256 `fe1560d0…`, read 2026-11-08. The two agree on every rule below; master is not
the pin.

`linux_disk_identity.py` selects the top from the descriptor bytes:

- Explicit `Snapshots/TopGUID` is the top. The predefined `{5fbaabe3-…}` names it only when TopGUID is absent;
  with an explicit TopGUID it is an ordinary image.
- An explicit TopGUID that is empty, padded, malformed, duplicated, not an image or not a leaf refuses. It never
  falls back to the predefined GUID.
- BackupID `{704718e1-…}` cannot be the top.
- One Storage, Start 0, End = Disk_size, Padding 0, Cylinders×Heads×Sectors = Disk_size, positive Blocksize.
  Padding 1 and split storage refuse as outside the specification.
- Types `Plain` (root only) or `Compressed`. Every overlay must be Compressed (spec: overlays must be expandable).
- Image and Shot GUIDs one-to-one; one root; every image reaches it; no cycle; no dangling parent; File names are
  plain `.hds` names with no separators, aliases or case-only duplicates. The spec allows relative or absolute
  paths; this selector is stricter and fails closed.
- Unspecified nodes and attributes are neither interpreted nor rejected and do not change the selection. The spec
  says they "don't affect functionality" and must be left as they are. Input is bounded at 1 MiB; DTD and entity
  declarations refuse.
- Disk UID: the descriptor's `Disk_Parameters/UID` is an unspecified node. It must be present exactly once. The
  setup verifier compares it to the reviewed disk `DISK_UID` and refuses a mismatch. It is an identity equality
  check only; it establishes no controller, guest or disk correlation.
- `preparation_observations.descriptor_chain` is the reference validator; the selection must agree with it.

Stage1 and post-run verification re-derive the selection and refuse a caller observation whose GUID or complete
`result_digest` differs. The only supported mechanism is `parallels-descriptor-selection-v1`. Caller values cannot
add mechanisms.

Not established by metadata selection: image-body readability, header or Blocksize-to-cluster compatibility
(spec: each Compressed image's cluster size must equal Blocksize, held in its header), encryption, guest or disk
correlation, and setup proof. The retained descriptor's `Encryption` node has a zero engine and empty data and
salt; this selector does not interpret it. Reading any image body needs its own source.

Retained-719 control (local, `EFFIGY_LINUX_719_DESCRIPTOR`, input sha256 `974876…`, not committed): derives the
predefined top `{5fbaabe3-…}`, file `harddisk1.hdd.0.{5fbaabe3-…}.hds`, a four-image chain to zero, 37 images, and
disk UID `{44b5d35e-…}`. The setup verifier accepts the same bytes with the matching observation. This is
metadata selection only. It is not a runtime, header or setup proof, and it grants no admission, correlation or
credit to any prior run.

Not run here: a live Stage1 against the registered VM, and any image-body or header read.
