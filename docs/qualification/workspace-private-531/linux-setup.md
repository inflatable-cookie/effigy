# Linux transport and Vault-backed placement proposal — unarmed

No VM/SSH/account/key/storage inspection or effect is authorized here. A later
fresh Queue-bound stage needs exact operator effects, numeric time/storage bounds
and disposition. Keep the reviewed **SSH** transport; Parallels authenticates
preparation facts, not a replacement proof transport.

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

## Setup adapter status (source-only, revision 9)

Pure controls, no effect. Files: `linux_setup.py` (Stage1 entry bound to the registered
home, admission verifiers, setup plan, executor, host credit, `ssh_argv`),
`linux_supervisor.py` (transient SERVICE argv, gate, outside-payload monitor, guest
candidate producer, root entry point), `linux_transfer.py` (bounded local processes, the
contained stdin transfer), `linux_receipts.py` (OpenSSH-signed admission receipts),
`linux-setup-probe.sh` (read-only probe, hash bound), `linux-setup-stages.json` (ledger
checked against the plan), `linux_setup_tests.py` (161 controls, including real-process flows;
`test:qualification:linux-setup`).

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
- **Stage1.** One start, one read-only probe, one graceful stop, 600 s, no snapshot, no
  `--kill`. The request reads the registered home itself. Config, descriptor and extent
  inventory are recomputed from `PVM_HOME`, and the probe script must hash to the reviewed
  value. Post-stop, `stage1_verify_post` refuses on drift in config bytes, identities,
  descriptor, extents, snapshot tree or growth above 1 GiB. Post-run bytes are retained into the stage before any check, and the
  writable top is an observed image, never a file name (K-25).
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
  top: the file-name derivation is removed; a top needs an observation from a supported
  mechanism, and none is established, so Stage1 refuses before any effect (K-25). Custody:
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

Still unavailable: supported writable-top and controller-to-guest association,
source-verified kernel hard limits, fresh signed Stage1 evidence, exact activation
anchors and root installation authorization. No VM, SSH, account, service, build,
proof or production effect ran here. E10/E13k/E14 stay held; measurement692 grants
no longer stage. Source controls confer no native proof credit.


### Diagnostic observation preparation

The separate `qualification:linux:diagnostic-proof-only` entry prepares an observation handoff, not setup or placement admission. A future execution requires an actual Nucleus proof-only launch at the clean pinned preparation head, after a native operator GO; this source work grants no GO. The entry cannot authenticate Nucleus provenance against another process of the same user. A request or caller environment alone is not authority.

A prospective request is printed by `effigy qualification:linux:diagnostic-proof-only -- plan ROOT ACTIVATION_ID HEAD`. Provision an owned canonical 0700 root under Stash scratch first. Save the exact JSON as an owned read-only request, hash it, and bind those bytes and inputs to the later Nucleus launch. The proof selector accepts only that request path and hash. It claims the activation exclusively, keeps a 600-second absolute budget including a 120-second report reserve, and records command streams and waited exits. The fixed probe is read-only; neither a successful transport nor the observation receipt qualifies Linux readiness.

The observation retains pre- and post-config and descriptor bytes independently and accounts all descriptor extents without naming a writable top. It records observed package growth, which is not a hard quota. Changed or unavailable facts remain findings; writable-top and guest association remain unknown. A positively owned start gets only the budgeted graceful stop, with uncertainty left unknown. The separately signed fresh Stage1/setup admission and its supported-mechanism gates remain required and unchanged.
