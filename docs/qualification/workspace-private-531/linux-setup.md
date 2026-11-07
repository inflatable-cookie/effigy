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

## Setup adapter status (source-only, revision 4)

Pure controls, no effect. Files: `linux_setup.py` (Stage1 entry bound to the registered
home, admission verifiers, setup plan, transfer runner, executor, host credit,
`ssh_argv`), `linux_supervisor.py` (transient SERVICE argv, outside-payload monitor,
guest candidate producer, `SystemdOps`, root entry point), `linux-setup-probe.sh`
(read-only probe, hash bound), `linux-setup-stages.json` (ledger checked against the
plan), `linux_setup_tests.py` (95 injected controls; `test:qualification:linux-setup`).

- **Supervisor lifecycle.** `systemd-run --uid=effigy531 --service-type=exec
  --property=ExitType=cgroup --property=KillMode=control-group
  --property=RuntimeMaxSec=<n> --remain-after-exit --pipe -- <argv>`. A threaded monitor
  samples `systemctl show` (InvocationID, ControlGroup, ActiveState) and the cgroup's
  `populated` bit while the unit is loaded. InvocationID is parsed as contiguous hex,
  the form systemctl prints (bus-print-properties.c, the byte-array branch); a
  hyphenated value folds to the same 32 hex. Candidate eligibility: unit absent before
  start; the monitor saw this InvocationID active with one ControlGroup; no sample with
  `populated 1`; post-run loaded, same InvocationID, inactive, Result=success,
  ExecMainStatus=0, empty ControlGroup. A failed read, vanished unit or changed
  InvocationID is unknown.
- **Host credit.** The host reads the unit over the VM route and credits closure only
  when the candidate equals that observation (boot ID, nonce, unit, InvocationID,
  active state, result), through the 686 `unit-inactive-exittype-cgroup` basis.
- **Stage1.** One start, one read-only probe, one graceful stop, 600 s, no snapshot,
  no `--kill`. The request reads the registered home itself: config, descriptor and
  extent inventory are recomputed from `PVM_HOME/config.pvs` and
  `harddisk1.hdd`, never supplied as bytes. The probe script must hash to the reviewed
  value. Post-stop, `stage1_verify_post` refuses on drift in config bytes, identities,
  descriptor, extents, snapshot tree or growth above 1 GiB.
- **Admission.** The plan is rebuilt from facts by the executor, never from a caller's
  rows. Gates:
  - the probe output is hash-bound and parsed: `effigy531` absent (E3 account/home
    absence), the docker group present (E4), a verified root chain and host key;
  - GO receipts are files bound by sha256 to their gate and to the exact plan digest
    (stale plan, wrong gate, unapproved or fabricated receipts refuse);
  - E6 accepts only one restricted ssh-ed25519 line whose key is the E2 key, whose
    embedded expiry equals the bound expiry, and which expires within 24 hours;
  - disk association must pass `verify_disk_association` with a supported mechanism;
  - a hard limit must pass `verify_hard_limit`: kernel-enforced and covering the row's
    cap. Polling counters never qualify;
  - the bootstrap allowlist is by row id and exact argv (`BOOTSTRAP_EXACT`), and the
    E-SUP and E6 argv embed bound values only.
- **Transfers.** `run_transfer` runs a local process (ssh or prlctl) with stdin from a
  bound file, capped streams, a live counter monitor and a group kill at the deadline.
  Exit status is the waited result, never closure. The remote command is one quoted
  string (`ssh_argv`, `shlex.join`). E9b extracts the archive and prints the sha256 of
  its sorted per-file digests. The host computes the same digest from the tar, and the
  control runs the remote script locally to check that they agree.
- **Expectations.** Every non-supervised row requires an expectation (exact stdout, a
  regex, or a sha256). An absent expectation never passes.

Decisions taken here (reviewer checks):

- **K-1** Streams come from `--pipe`, not `StandardOutput=file:`, which was not
  confirmed in the upstream text read.
- **K-2** The 686 contract accepts `unit-inactive-exittype-cgroup` for loaded transient
  services. The existing `cgroup-populated` basis is unchanged, with its own control.
- **K-3** Closure credit is host-side only. The guest candidate is never
  self-authenticated, and the runner cannot grant credit.
- **K-4** Root one-shot rows (E3a-E7c, E-SUP) take `vm-waited-exit-postobserved`, with
  an exact expectation per row. Not contained closure; a leftover child of a one-shot is
  not positively excluded.
- **K-5** Disk association and hard limits fail closed. Neither has a source-verified
  mechanism yet. The Parallels reference could not be decoded; polling counters are not
  quotas.
- **K-6** InvocationID is required observed state. The manuals read do not list it as a
  documented unit property, so a missing value fails closed.
- **K-7** The root supervisor and E8 run over the VM route (`prlctl exec`). An sshd
  `ForceCommand` runs as the login account, and systemd-run cannot start a unit as
  another account without root. SSH carries file placement and the tree extraction, not
  the root-supervised payload. This is a design boundary the planner may want to rule on.
- **K-8** A failed payload gets no candidate and so no credit.
- **K-9** The guest request binder hashes `linux_setup.py` and `linux_supervisor.py`.
- **K-10** ssh-transfer rows need a containment receipt bound to the plan digest, and a
  verified hard limit. Neither is established, so E9a, E9b, E19 and E8 stay blocked.

Still required before any activation (not in this change): a source-verified controller-to-guest disk association with fresh identifiers on both sides; a source-verified kernel hard-limit mechanism; the VM-route identity re-observation and the terminal-state observation wired to the real route; the staged supervisor and its root install as a separate GO; the containment receipt for stdin transfer; the H archive at its pinned path with the pinned ssh config and known_hosts under the control root; the E2 key line and its expiry; the Stage1 probe script and output bound to this plan; the reviewed placements; and each live row's own GO. The positive closure path is exercised here only by injected timelines. It still needs a live systemd check of the real InvocationID and terminal sequence. E10, E13k and E14 stay held as before.
