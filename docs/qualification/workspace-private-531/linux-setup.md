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
