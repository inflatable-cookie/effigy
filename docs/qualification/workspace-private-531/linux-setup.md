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

Controller-to-guest association is a fail-closed verifier in `linux_disk_association.py`. `PINS` and
`SUPPORTED_DISK_MECHANISMS` are empty, so every association refuses. Six premises are checked on their own inputs
(`premise_table`):

- `mechanism` and `host_read`: a supported mechanism, one pinned read-only accessor with exact tool bytes and Desktop
  build, bounded output, a boot bracket, and the serial and image UID the output names.
- `config`: exactly one `Hdd` (hdd0, SATA index 0, reviewed device Uuid) that is enabled, connected, not passthrough,
  not remote, and `DiskType` 1. Every `Hardware` child is a reviewed tag. No element outside `Hdd` and `CdRom` carries
  a block-capable name at any depth (`Hdd`, `Disk`, `Scsi`, `Sata`, `VirtIOBlk`, ...). USB holds only the reviewed
  virtual camera, and the connect history names only `VIRTUAL@` devices. `CdRom` is counted, not treated as a disk.
- `descriptor`: selected top, chain and every extent, from the descriptor bytes.
- `guest_blocks`: exactly one whole disk, `/dev/sda`, non-removable, on the platform ATA controller
  `/sys/devices/platform/PRL4010:00/ata…`, with the reviewed root partition on it. Loop and ROM are classified by
  type; anything else refuses.
- `serial`: the host read serial equals the guest parent serial.

**Source status of each value the verifier requires.** A value is source-qualified only where a primary source gives
its meaning and numeric value. Anything else is observed in the retained config and stays unadmitted.

| Element and field | Required value | Source status |
| --- | --- | --- |
| `Hdd` `Passthrough` | 0 | Meaning sourced: `PrlVmDev_SetPassthrough` (C API v19) enables passthrough mode for optical or hard-disk devices. The boolean sense of `IsPassthrough` is by name. |
| `Hdd` `Enabled`, `Connected` | 1 | Accessors `PrlVmDev_IsEnabled` and `IsConnected` exported. `IsConnected` does not say configuration or running state. The 1 is observed. |
| `Hdd` `Remote` | 0 | `PrlVmDev_IsRemote` exported. Meaning not fetched. Observed. |
| `Hdd` `DiskType` | 1 | Numeric enum sourced (C API v17, `_PRL_HARD_DISK_INTERNAL_FORMAT`): `PHD_PLAIN_HARD_DISK` = 0, `PHD_EXPANDING_HARD_DISK` = 1. So the retained disk is expanding, not plain. The XML-to-enum mapping is inferred. |
| `Hdd` `InterfaceType` | 2 (SATA) | Not numerically sourced. The product CLI names `sata`, `ide`, `scsi` for `--iface`. `PrlVmDev_GetIfaceType` uses `PRL_MASS_STORAGE_INTERFACE_TYPE`, whose values were not located. |
| USB `EmulatedType` 3, `UsbType` 2, `SystemName` `VIRTUAL@UVC@` | reviewed camera | Not numerically sourced. The `VIRTUAL@` prefix is observed. |
| `CdRom` `Passthrough` 1 | not a disk | Observed. A passthrough optical drive is not a disk. The guest's `sr0` is ROM and refuses as a disk under `guest_blocks`. |
| `Hardware` child tags (17) | closed set | Observed in the retained config. The element names exist in `libPrlXmlModel.1.dylib`. |

**Replay of the retained 747 originals** (transport observation, outcome sha256 `a99c3ac4…`, boot `6de5909b…`; no new
native read) through `_run`: `config` pass (before config; post-config Hdd and CdRom subtrees identical; the retained
USB camera and `VIRTUAL@` history accepted), `descriptor` pass, `guest_blocks` pass (15 nodes; `/dev/sda`
137438953472 B, removable 0, controller `/sys/devices/platform/PRL4010:00/ata1/...`; `sda2` parent `sda`). `mechanism`,
`host_read` and `serial` refuse: no pinned read exists, and no host artifact carries the guest serial
`BV9VAH2KP2Q4N4A04SN0`. The config device Uuid `{9447f202…}` and the descriptor disk UID `{44b5d35e…}` are different
namespaces, and no documented rule joins them. Guest `ata1` is not mapped to SATA position 0.

**SDK surface.** Installed 27.1.0 build 58756, `ParallelsVirtualizationSDK` sha256 `464af6b9…`, 1270 exported symbols
(`nm -gU`).

- *Disk serial:* none. Exports whose name contains "serial" (case-insensitive) number 10, all VM, server or serial-port:
  `PrlDispCfg_IsAddSerialPortOutputToReport`, `PrlDispCfg_SetAddSerialPortOutputToReport`,
  `PrlSrvCfg_GetSerialPort`, `PrlSrvCfg_GetSerialPortsCount`, `PrlVmCfg_GetSerialPort`,
  `PrlVmCfg_GetSerialPortsCount`, `PrlVmCfg_GetSmbiosSystemSerialNumber`, `PrlVmCfg_SetSmbiosSystemSerialNumber`,
  `PrlVmDevSerial_GetSocketMode`, `PrlVmDevSerial_SetSocketMode`.
- *Configured-device accessors* (documented or named as the configuration's): `PrlVmCfg_GetAllDevices` (C API v19: "Obtains
  a handles list of all virtual devices in a virtual machine", on a `PHT_VM_CONFIGURATION` handle), `PrlVmCfg_GetDevsCount`,
  `PrlVmCfg_GetHardDisksCount`, `PrlVmCfg_GetHardDisk`, `PrlVmCfg_GetOpticalDisksCount`, `PrlVmCfg_GetUsbDevicesCount`,
  and the `PrlVmDev_*` and `PrlVmDevHd_*` accessors on those handles.
- *Runtime candidates, none usable:*
  - `PrlVm_GetState`: VM state only, no device-level content.
  - `PrlVm_InitiateDevStateNotifications`: C API v19 says it "starts sending notifications about devices states by VM".
    It is asynchronous, returns a job whose result is read with `PrlJob_GetRetCode`, and its notifications "have to be
    same as notifications after changing a device state". The page does not say whether they describe device presence in
    the running guest or the configuration. The event grammar is not in the permitted sources.
  - `PrlVm_GetStatistics` and `PrlVm_GetStatisticsEx`: the `flags` argument is "from enum PRL_API_COMMAND_FLAGS". Per-device
    content is not stated.
  - `PrlVmDev_IsConnected`: C API v19 says "Checks if the specified virtual device is connected", without saying whether it
    reads configuration or running state.
  - `PrlVmInfo_GetState`, `PrlVmInfo_GetVmProcessId`: VM-level.
- *Not located in permitted sources:* numeric values of `PRL_MASS_STORAGE_INTERFACE_TYPE`, `PRL_VM_DEV_EMULATION_TYPE`
  and `PRL_API_COMMAND_FLAGS`. The bundle ships no SDK headers.

**Closed enumeration and the missing premise.** A count of one hard disk on each side is not a bijection. The link needs
a premise the sources do not give: the guest presents no block device the registered config does not declare. Host
closure is established by the retained originals (the Hdd and CdRom subtrees, the 17 reviewed tags, and no block-capable
element under an allowed tag). Presentation is not established. The one counterexample shape the retained bytes cannot
refuse is an image-backed config `Hdd` beside a guest disk the config does not declare. `test_bare_count_of_one_is_not_the_disk`
pins the physical-disk and disabled-disk shapes, which the verifier does refuse.

**External limitation (delivered; no executable read proposed).** No permitted source pins a runtime-presentation accessor
with its grammar, scope and limits. Configured-device accessors answer for the configuration, by their own docs. The one
candidate with a runtime name, `PrlVm_InitiateDevStateNotifications`, lacks its event grammar and scope. So no read is
source-ready for presentation. Design choices left to the planner:

1. Accept presentation as a threat-contract assumption (the hypervisor presents what the configuration declares, under the
   same-user boundary). Then the host inventory read below admits the host premise.
2. Obtain the event grammar and scope of `PrlVm_InitiateDevStateNotifications`, or the SDK headers, from a permitted source.
   Then a runtime notification read could be pinned.
3. Keep refusing. `SUPPORTED_DISK_MECHANISMS` stays empty and 623 is not ready.

The host inventory read for choice 1, not run here (native; needs its own authorization). Every call is read-only.
Registration calls are excluded: `PrlVm_RegEx` (v19: "Creates a new virtual machine and registers it with the Parallels Service") and `PrlVm_Reg` change server state, so they are not used.

1. `PrlSrv_LoginLocal`: opens the local service session. Its state effect is not fetched from the docs.
2. `PrlSrv_GetVmListEx` with `nFlags` 0 (obsolete, per the v19 page): lists registered VMs as `PHT_VIRTUAL_MACHINE`
   handles through a job. The v19 page does not state a state change; read-only is inferred from its description.
3. For each listed handle: `PrlVm_GetConfig` (v19: "Obtains a handle of type PHT_VM_CONFIGURATION"; no state change
   described) and `PrlVmCfg_GetUuid`, `PrlVmCfg_GetHomePath`. Select the handle whose UUID is the reviewed VM. A VM
   that does not match, or two matches, refuses.
4. On the selected config handle: `PrlVmCfg_GetAllDevices` (v19: "Obtains a handles list of all virtual devices in a
   virtual machine"), `PrlVmCfg_GetDevsCount`, then per device `PrlVmDev_GetType`, `GetIfaceType`, `GetIndex`,
   `GetSysName`, `GetImagePath`, `IsEnabled`, `IsConnected`, `IsRemote`, `IsPassthrough`, and `PrlVmDevHd_GetDiskType`
   for hard disks. `IsConnected`'s config-or-running sense is not stated by its page.
5. `PrlVm_GetState` before and after, bracketed with the guest boot id.

Output: one `key=value` line per device, at most 64 KiB, exit 0. Calls: no `Set*`, `Connect`, `Disconnect`, `Reg*`
or `Unreg*`. It closes the host inventory on the SDK's own parser, not the XML one. It does not close presentation.

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

`linux_allocation_bound.py` verifies the one mechanism that can bound retained allocation on the native root: ext4 project quota (`quota` and `project` features, one reserved project id on the stage tree `BASE`, inherited). The kernel refuses allocation past the project's hard block limit, for every writer that creates objects in the tree, root supervisor included. The verifier takes five route originals (`mountinfo`, `dumpe2fs -h`, `by-uuid`, the fixed `quota` helper output with Q_GETFMT and Q_GETQUOTA, and the fixed `tree` helper output with lstat and FS_IOC_FSGETXATTR per entry). It checks each original's hash and exit, then matches them against exact request facts: boot id, owned uid, `BASE` and the exact entry set. Accepted witnesses must show a writable native ext4 root, a quota record whose `valid` bits mark both block limit and space usage, an active project limit covering the row with headroom, one device on every entry, and the project id on every directory and file, with no hardlinked file, symlink or foreign object. The supervisor's gate and transfer FIFOs under `BASE/stage` are admitted as blockless single-link entries owned by the payload uid or root: they allocate nothing, so they add no charge, and their project id is not read because opening a FIFO can block. Kernel charge must be at least the tree's bytes. `route_plan` binds the fixed argv and the helper's SHA-256; nothing here runs them.

Observed 2026-11-08 (747 originals, `observation-filesystem.raw`): `/dev/sda2` ext4 UUID `d488be7b-…`, mounted `rw,relatime` with `user_xattr,acl`, features without `project` or `quota`, and no `repquota`, `quota`, `quotacheck`, `quotaon` or `edquota`. That is absence. A test feeds the retained features line in and expects refusal (`lacks project,quota; enforcement is not provisioned`). Production admits nothing: `SUPPORTED_HARD_LIMITS` stays empty, and `verify_hard_limit` delegates the new mechanism only when it is admitted and its facts are complete.

What the mechanism bounds, per row. The earlier wording "≤ N MiB writes" is not one metric, so each row is qualified by what it can actually bound:

| Row | Written wording | Metric this mechanism can bound | Not bounded here |
| --- | --- | --- | --- |
| E8 | ≤ 60 s, ≤ 64 MiB writes | retained allocation ≤ 64 MiB in `BASE` | cumulative writes: overwrites and delete-and-rewrite allocate nothing new |
| E9a / E9c | archive ≤ 40 MiB | retained archive bytes in `BASE/inputs` | the transfer stream itself; partial output past EDQUOT is a failed transfer, not a success |
| E9b | extracted ≤ 500 MiB | retained extracted tree in `BASE/source` | temporary create-and-delete traffic during extraction |
| E19 / EGO | placements ≤ 40 MiB each | retained placement bytes | — |

A per-file `RLIMIT_FSIZE`, a sampled counter, a post-growth census, `MemoryMax` and `IOWriteBandwidthMax` are not this bound. Cumulative I/O volume has no admitted mechanism. If an operator needs "≤ 64 MiB writes" literally, that is a separate metric decision, not something this mechanism gives.

Residual writers outside the project. Objects created under `BASE` (including the root supervisor's stage files, since `BASE/stage` inherits the project) are charged. Writes outside `BASE` are not bounded by this mechanism: root-owned approval records under `/var/lib/effigy531-approved`, supervisor records, the fixed-size drop-in and sudoers files, and everything E12 writes to Docker storage (pull ≤ 1 GiB, build, engine disk ≤ 2 GiB) or apt. Each needs its own finite bound before its row can rely on this mechanism. The current source does not establish one for the approval record or supervisor records, and E12 stays held.

#### Proposal (not approved; needs an operator ruling)

Goal: make the native root enforce a project quota with no mount change and no data move. Smallest supported change, in this order:

1. Pre-change: create one Parallels snapshot of the registered VM (the VM has 36 snapshots; this is one more). Retain `dumpe2fs -h /dev/sda2` and `mountinfo` before the change.
2. Offline, from a rescue boot with `/dev/sda2` unmounted: `e2fsck -f /dev/sda2`, then `tune2fs -O project -Q prjquota /dev/sda2` (exact flags to be confirmed against the installed e2fsprogs man page before running; `-Q prjquota` should enable only project accounting, not user and group quota), then `e2fsck -f /dev/sda2`. No `fstab` edit, no mount-option change, no remount, no `quota` package.
3. Normal boot. The kernel enables the feature at mount time. Verify with the `quota` helper mode: `getfmt_rc=0`, `fmt` nonzero.
4. Set the stage tree's project: `chattr -R -p 531 BASE` and `chattr +P BASE` (both directories and files), run before any payload writes under `BASE`. At E7 the tree is empty, so no existing object carries another project.
5. Set the hard limit: 1 GiB (1048576 quota blocks of 1 KiB), enough for the E8/E9/E19 caps plus placements. This needs a fixed `Q_SETQLIM` setter as a new helper mode, reviewed separately, so no package install is needed. Later stages (E10 toolchain ≤ 4 GiB, E13 target ≤ 25 GiB) need their own reviewed budgets on the same mechanism before they run.
6. Activation: stop the VM, then the new `tree` and `quota` originals are taken under a fresh boot; `verify` must admit with `remaining_after_bytes` shown.

Affected: the root filesystem superblock feature flags and two hidden quota inodes. No user data is rewritten; `BASE` is empty until E7. Capacity: quota inodes a few MB. Time: an offline `e2fsck` of a 128 GiB ext4 with about 8.3 M inodes, minutes, to confirm on native. Interruption: power loss or a failed `tune2fs` during step 2 can corrupt the root; the step-1 snapshot is the recovery path. Rollback: restore the step-1 snapshot. Removing the feature in place (`tune2fs -O ^quota`) is not assumed to be clean. Retention: keep the snapshot and every pre/post `dumpe2fs`, `mountinfo`, quota and tree original.

Operator rulings required: (a) approve one VM snapshot and the offline `tune2fs`/`e2fsck` on the native root; (b) approve the metric wording above (retained allocation, not cumulative writes) for E8 and the E9/E19 rows; (c) confirm E12 and root-owned records outside `BASE` stay held until their own bounds exist.

Alternative, only if (a) is refused: a dedicated fixed-size ext4 partition or virtual disk for `BASE`. That changes placement, so it is a material change needing its own proposal.

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
