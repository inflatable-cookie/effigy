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

## Setup adapter status (source-only, revision 2)

Pure controls, no effect. Files: `linux_setup.py` (probe v2 parser, Stage1 request
and post-observation, fail-closed disk verifier, `setup_plan` rows, `execute_plan`
executor, `ssh_argv`), `linux_supervisor.py` (transient SERVICE argv, supervised
run, `SystemdOps`, root entry point), `linux-setup-probe.sh` (read-only v2 probe,
hash bound in the manifest), `linux-setup-stages.json` (ledger; checked against
the plan), `linux_setup_tests.py` (54 injected controls; `test:qualification:linux-setup`).

- **Probe v2** requires root uid, boot ID, `running`, systemd ≥ 250, `cgroup2fs`,
  `effigy531` absent, sshd `expiry-time` present, numeric endpoint, host key
  fingerprint and the exact `/dev/sda2` ← `sda` ext4 root chain. Any miss refuses.
- **Stage1** is one start, one read-only probe, one graceful stop, 600 s, no
  snapshot, no `--kill`. Pre-start it needs the retained config and descriptor bytes,
  bundle and config identities and the complete extent inventory for the descriptor
  top. Post-stop `stage1_verify_post` refuses on any config-byte change (no volatile
  field is exempt), identity change, snapshot change, descriptor change, extent
  substitution or extra extent, and growth above 1 GiB.
- **Setup plan.** Rows E3a–E7c, E-SUP, E9a–E9b, E8 and E19 are built with exact
  argv. A row with a missing live fact (GO, public key, expiry, disk association,
  staged supervisor, pinned ssh config, archive, placement hash) carries that fact in
  `missing` and an empty argv. The executor halts on the first missing row. A stdin
  hash that differs from the bound hash halts before any intent is written.
- **Executor.** Each row writes its intent before the injected runner is called,
  retains streams, applies the post-observation expectation, and advances only under
  the basis: `vm-waited-exit-postobserved` and `ssh-transfer-postobserved` need the
  waited exit plus a post-observation; `supervised-contained` needs closure credit.
- **Supervisor.** Root runs it through `prlctl exec`; the payload runs as `effigy531`
  via `systemd-run --uid=effigy531 --service-type=exec --property=ExitType=cgroup
  --property=KillMode=control-group --property=RuntimeMaxSec=<n> --remain-after-exit
  --pipe -- <argv>`. Pre-start the unit must be absent (nonce fresh). Post-run it must
  be loaded with the same new InvocationID, `inactive`, `Result=success`,
  `ExecMainStatus=0` and no control group. A live `populated 1` sample for that
  InvocationID vetoes credit. Anything else is unknown.

Decisions taken here (reviewer checks):

- **K-1** Streams come from `--pipe`, not `StandardOutput=file:`. The `file:` option
  was not confirmed in the upstream text read, so it stays unused.
- **K-2** The 686 closure contract accepts a second terminal basis,
  `unit-inactive-exittype-cgroup`. A successful real service has an empty
  `ControlGroup` after it goes inactive, so the existing `cgroup-populated` basis
  could never credit it. The new basis keeps the InvocationID, nonce, unit and
  supervisor checks, and forbids a control group on either side. The existing basis
  is unchanged.
- **K-3** Root one-shot rows (E3a–E7c) take `vm-waited-exit-postobserved`. This is
  not contained closure. A leftover child of such a one-shot is not positively
  excluded. The supervisor cannot be the first root step, since its own install
  needs a closed root op.
- **K-4** Disk association fails closed. No mechanism is in the supported set. The
  Parallels CLI reference could not be decoded from the PDF, so `--iface` and
  `--position` remain a starting point. Size, model, path and derived serials are refused.
- **K-5** InvocationID is required observed state. The manuals read do not list it as
  a documented unit property, so a missing value fails closed.
- **K-6** A failed payload gets no closure credit, matching the existing contract's
  refusal of non-zero contained exits.
- **K-7** The root supervisor is invoked over the VM route, not SSH. A sshd
  `ForceCommand` runs as the login account, and systemd-run cannot start a unit as
  another account without root. The SSH rows are file placements with a remote
  sha256 post-observation, not payload runs.
- **K-8** A post-run unit state confirms the terminal result. It is not the same as
  observing the cgroup empty. Positive cgroup emptiness is only a veto input.

Still required before any activation (not in this change): a supported controller-to-guest
disk association with fresh identifiers on both sides; a staged supervisor and a bound
staging route for it, then its root install as a separate GO; the H archive bytes at a
pinned path, a pinned ssh config under the control root, and fresh E2 key bytes with an
expiry; the Stage1 probe v2 result and post-observation bound to this plan; the reviewed
driver, inputs, GO index and GO receipt placements; and each live row's own GO. E10,
E13k and E14 stay held as before.
