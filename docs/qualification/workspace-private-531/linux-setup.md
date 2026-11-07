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

## Setup adapter status (source-only, revision 3)

Pure controls, no effect. Files: `linux_setup.py` (probe v2 parser, Stage1 request
and post-observation, fail-closed disk verifier, `setup_plan` rows, `execute_plan`
executor with its gates, host credit, `ssh_argv`), `linux_supervisor.py`
(transient SERVICE argv, guest-side candidate producer, `SystemdOps`, root entry
point), `linux-setup-probe.sh` (read-only v2 probe, hash bound in the manifest),
`linux-setup-stages.json` (ledger checked against the plan), `linux_setup_tests.py`
(72 injected controls; `test:qualification:linux-setup`).

- **Probe v2** requires root uid, boot ID, `running`, systemd >= 250, `cgroup2fs`,
  `effigy531` absent, sshd `expiry-time` present, numeric endpoint, host key
  fingerprint and the exact `/dev/sda2` <- `sda` ext4 root chain. Any miss refuses.
- **Stage1** is one start, one read-only probe, one graceful stop, 600 s, no
  snapshot, no `--kill`. It binds retained config and descriptor bytes, bundle and
  config identities and the full extent inventory of the descriptor top.
  `stage1_verify_post` refuses on config-byte drift (no field is exempt), identity
  change, snapshot change, descriptor change, extent substitution or extra extent,
  and growth above 1 GiB.
- **Guest supervisor.** Root runs it through `prlctl exec`; the payload runs as
  `effigy531` via `systemd-run --uid=effigy531 --service-type=exec
  --property=ExitType=cgroup --property=KillMode=control-group
  --property=RuntimeMaxSec=<n> --remain-after-exit --pipe -- <argv>`. The guest
  produces a **candidate** receipt only; it never credits closure itself.
- **Host credit.** The host reads the unit over the VM route
  (`systemctl show`, root, read-only). `host_credit` credits closure only when the
  candidate's boot ID, nonce, unit, InvocationID, active state and result equal that
  independent observation, and the 686 contract (`unit-inactive-exittype-cgroup`
  basis, no control group on either side) holds. A guest receipt alone is unknown.
- **Setup plan.** Rows E3a-E7c, E-SUP, E9a-E9b, E8 and E19 carry an effect class:
  `bootstrap` (root operations on an exact allowlist, E3a-E7c and E-SUP only),
  `supervised` (payload through the supervisor), or `ssh-transfer`. A row with a
  live value missing keeps an empty argv.
- **Executor gates**, in order, each halting with the row id: missing facts;
  allowlist (bootstrap) or installed supervisor (supervised) or a containment ruling
  (ssh-transfer); identity (VM-route re-observation equals the plan's VM, boot ID and
  host key); credential (pinned ssh config and known_hosts are owner-only and match
  their bound hashes; the ephemeral key has not expired); total budget; resource
  accounting (counter files and limits polled before and after the row; counter loss
  or cap refuses); a hard limit for network or disk rows; stdin hash; one-use intent;
  runner; post-observation, or host credit for supervised rows.

Decisions taken here (reviewer checks):

- **K-1** Streams come from `--pipe`, not `StandardOutput=file:`, which was not
  confirmed in the upstream text read.
- **K-2** The 686 contract accepts the terminal basis `unit-inactive-exittype-cgroup`
  for loaded transient services. The existing `cgroup-populated` basis is unchanged.
- **K-3** Closure credit is host-side only (K-12 below). The guest's receipt is a
  candidate, never self-authenticated.
- **K-4** Root one-shot rows E3a-E7c and E-SUP take `vm-waited-exit-postobserved`.
  This is not contained closure; a leftover child of a one-shot is not positively
  excluded. The allowlist limits them to fixed exact operations.
- **K-5** Disk association fails closed. No mechanism is supported. The Parallels
  CLI reference could not be decoded, so `--iface` and `--position` remain a
  starting point. Size, model, path and derived serials are refused.
- **K-6** InvocationID is required observed state. The manuals read do not list it as
  a documented unit property, so a missing value fails closed.
- **K-7** The root supervisor runs over the VM route, not SSH. An sshd `ForceCommand`
  runs as the login account, and systemd-run cannot start a unit as another account
  without root.
- **K-8** Post-run unit state confirms the terminal result. It is not a direct
  observation of an empty cgroup. A live `populated 1` sample is a veto only.
- **K-9** Failed payloads get no candidate and so no credit.
- **K-10** The guest request binder hashes `linux_setup.py` and `linux_supervisor.py`.
- **K-11** ssh-transfer rows (E9a, E9b, E19) need two bound facts before they run: a
  containment ruling for stdin transfer, which is not a contained payload, and an
  independently verified hard limit, since polling counters are not a quota. Both
  are absent, so these rows stay blocked.
- **K-12** Credit is computed by the executor from the host observation, not by the
  runner, so a runner cannot grant closure.

Still required before any activation (not in this change): a supported
controller-to-guest disk association with fresh identifiers on both sides; a staged
supervisor and a bound staging route, then its root install as a separate GO; a
containment ruling for stdin transfer and an independently verified hard limit; the H
archive at a pinned path, the pinned ssh config and known_hosts under the control
root, and fresh E2 key bytes with an expiry; the Stage1 probe and post-observation
bound to this plan; the reviewed driver, inputs, GO index and GO receipt placements;
and each live row's own GO. E10, E13k and E14 stay held as before.
