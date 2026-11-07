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

## Setup adapter status (source-only, revision 8)

Pure controls, no effect. Files: `linux_setup.py` (Stage1 entry bound to the registered
home, admission verifiers, setup plan, executor, host credit, `ssh_argv`),
`linux_supervisor.py` (transient SERVICE argv, gate, outside-payload monitor, guest
candidate producer, root entry point), `linux_transfer.py` (bounded local processes, the
contained stdin transfer), `linux_receipts.py` (OpenSSH-signed admission receipts),
`linux-setup-probe.sh` (read-only probe, hash bound), `linux-setup-stages.json` (ledger
checked against the plan), `linux_setup_tests.py` (134 controls, including real-process flows;
`test:qualification:linux-setup`).

- **Gate and positive path.** The payload is wrapped so it blocks on a FIFO in the stage
  directory before running. The outside monitor samples the unit while it is loaded and
  releases the gate on the first sample that shows it active in a cgroup. The unit is
  therefore provably active, with a control group, before the payload does any work. A
  payload never observed active is never released and ends at its RuntimeMaxSec deadline,
  which is unknown. Candidate eligibility needs a pre-release active sample for this
  InvocationID in one cgroup, no failed read or release sample, and a post-run loaded
  unit with the same InvocationID, inactive, Result=success, ExecMainStatus=0 and an empty
  ControlGroup. A populated=1 sample while the unit runs is no veto (K-16). InvocationID is
  parsed as the contiguous hex systemctl prints.
- **Host credit.** The host reads the unit over the VM route and credits closure only when
  the candidate equals that observation (boot ID, nonce, unit, InvocationID, active state,
  result), through the 686 `unit-inactive-exittype-cgroup` basis.
- **Contained transfer.** A transfer row starts the supervisor (`prlctl exec`, request
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
  writable top is derived from the descriptor (K-18, K-19).
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
- **Admission.** The executor rebuilds the plan from facts, never from a caller's rows.
  Gates: the probe output is hash-bound (E3 account absence, E4 docker group); GO receipts
  are signed and plan-bound; E6 accepts only one restricted ssh-ed25519 line for the E2 key
  with the bound expiry within 24 hours; disk association and hard limits pass source-
  verified verifiers (none established yet, so they refuse); bootstrap rows match exact
  argv; payload rows need the effigy531 uid observed after E3a and their row nonce.

Decisions taken here (reviewer checks):

- **K-1** Streams come from `--pipe`, not `StandardOutput=file:`, which was not confirmed.
- **K-2** The 686 contract accepts `unit-inactive-exittype-cgroup` for loaded transient
  services. The `cgroup-populated` basis is unchanged, with its own control.
- **K-3** Closure credit is host-side only. The guest candidate is never self-authenticated.
- **K-4** Root one-shot rows (E3a-E7c, E-SUP) take `vm-waited-exit-postobserved` with an
  exact expectation. Not contained closure; a leftover child of a one-shot is not excluded.
- **K-5** Disk association and hard limits fail closed. No source-verified mechanism yet.
  Polling counters are not quotas.
- **K-6** InvocationID is required observed state; a missing value fails closed.
- **K-7** The root supervisor and the payloads run over the VM route (`prlctl exec`), not
  SSH. An sshd `ForceCommand` runs as the login account, and systemd-run cannot start a
  unit as another account without root. SSH carries the stdin into the FIFO only. The
  planner may want to rule on this boundary.
- **K-8** A failed payload, or one with no candidate, gets no credit.
- **K-9** The guest request binder hashes `linux_setup.py` and `linux_supervisor.py`.
- **K-10** Admission receipts are OpenSSH signatures, so a receipt is authenticated by an
  operator-held key and not by a path or hash alone.
- **K-11** The gate makes the positive path reachable for any payload duration: the
  release happens only after the monitor has observed the unit active.
- **K-12** The SSH containment ruling is removed. A transfer is contained by being a
  supervised payload with host credit, so no truthy ruling can stand in for closure.
- **K-13** Network and disk rows need a source-verified kernel hard limit. Polling counters
  never qualify, so E8, E9a, E9b and E19 stay blocked until one exists.
- **K-14** Row stages live under the reviewed stage root, a direct child only. The stage
  path is `STAGE_ROOT/<row>`, and the entry creates the root once, mode 0711.
- **K-15** `linux_supervisor.py` is a standalone executable with no sibling imports.
- **K-16** The unit lifetime rule is revised. A populated=1 sample while running is not a
  veto, since the gate already proves the unit was active. A failed read or release is
  Unknown, and so is any post-run state other than the expected terminal one.
- **K-17** The plan digest is bound to the reviewed head. Each signed receipt must name that
  head as well as the digest. A receipt for another head is refused.
- **K-18** Stage1 retains the post-run config and descriptor bytes into the stage before any
  check runs. Retention is exclusive, so production verifies once per stage. Tests make a
  fresh stage per check.
- **K-19** The writable top is derived from the descriptor. It must be the only image whose
  file name is the current snapshot and whose GUID is that snapshot. Zero or several refuse.
  The chain and extents still come from `observations`.
- **K-20** The placement map is exact. Module and driver placements, GO receipts, GO
  signatures and the allowed-signers file are each pinned by host path and sha256. The
  E9c B archive is pinned the same way as E9a. Anything missing or extra refuses.
- **K-21** The planner asked for an authenticated SSH payload and observation flow. This
  change does not do that. SSH still carries only the stdin into the FIFO, as in K-7. The
  payload start and terminal observation stay on the VM route, and closure comes from host
  credit plus the signed receipts. Moving the payload itself onto SSH would run it as the
  login account, not through systemd. Reviewer check requested on this boundary.

Still required before any activation (not in this change): a source-verified controller-to-guest disk association with fresh identifiers on both sides; a source-verified kernel hard-limit mechanism; the VM-route identity re-observation and terminal-state observation wired to the real route; the staged supervisor and its root install as a separate GO; the operator's signing key and allowed-signers file, with signed GO receipts for each gate; the H archive at its pinned path with the pinned ssh config and known_hosts under the control root; the E2 key line and expiry; the Stage1 probe script and output bound to this plan; the reviewed placements and the GO placements (E19.N, EGO.N); the B archive at its pinned path
(E9c); the controls payload argv; and each live row's own GO. The gate, the positive path and the contained transfer are exercised here with real local processes and FIFOs, but not with real systemd, real prlctl or a real SSH server. A live systemd check of the InvocationID and terminal sequence is still needed. E10, E13k and E14 stay held as before.
