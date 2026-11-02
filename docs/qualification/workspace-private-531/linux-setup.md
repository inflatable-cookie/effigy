# Linux transport and placement proposal — unarmed

No inspection, account/key/config change, mount, transfer, build or proof is
authorized by this document. A later scoped GO and fresh owner-bound run must
name exact effects, numeric time/storage bounds and retained-resource disposition.
This preserves the packet's existing **SSH** driver; Parallels authenticates
preparation facts only and does not replace the native proof transport.

## Evidence and bounded prerequisite

The approved `b1a9018c` v2 readiness turn observed UUID
`433c9e5b-c7b9-469f-9633-24b4addcd3b5`, package
`/Volumes/Vault/Parallels/Ubuntu 26.04 ARM64.pvm`, retained running. Parallels
execution worked as root: Ubuntu 26.04.1/aarch64, Python 3.14.4, Docker 29.9.2,
Compose 5.6.1, engine `56fee8f8-1a38-42d6-acf9-d446ecff279c`. SSH service/socket
and port 22 were listening at observed IP `10.211.55.8`; **no SSH connection,
alias, authentication or host-key discovery occurred**. Sharing was disabled and
`/Volumes/Builds` absent. Rust tools were unavailable in that root context;
other users were not inspected. Only `hello-world` was resident, not a qualified
runtime. These are historical observations, not current readiness assertions.

Request a **600-second maximum preparation stage**, within remaining task budget,
with smaller host/guest timeouts. Its first read-only admission must revalidate
mounted Vault, canonical package, UUID/Home/description and running state, using
existing Parallels execution. No start/resume/repair is included. Establish only:

- Whether proposed account `effigy531` is absent, current numeric uid/gid
  allocation, existing Docker group/socket permissions and relevant SSH public
  settings/version. No existing account, key, credential or user-content reads.
- Current IP, public SSH host key/fingerprint via that authenticated channel,
  password-disabled public-key login and key-expiry support. A network scan or
  unauthenticated `ssh-keyscan` is not a pin.
- Host sharing declarations and guest `prl_fs` driver/mount-helper support for
  **one exact named share** at a canonical target. Existing shares must not
  become newly exposed. Exact mount syntax/options remain unverified.
- Whether a new exact host alias can be isolated from existing SSH `Include`,
  wildcard, `Match exec`, proxy/local-command and agent settings without reading
  credentials or executing configuration hooks. If this cannot be established
  safely, retain/refuse and present the concrete config delta for approval.

Missing support/collisions/changed identities refuse the affected step; no
fallback account, transport, broad share or guessed alias. Do not claim the
following plan executable until these facts and exact GO effects are frozen.

## Proposed dedicated identity and pinned SSH route

After that admission, the GO may authorize creating **only a new** test account
`effigy531`, with password login disabled, owned private home and one dedicated
`authorized_keys` file. Refuse if the account already exists. No modification of
existing accounts or keys. Access to the existing Docker socket would require
membership in its actual Docker group: **root-equivalent authority inside this
selected proof VM**, explicitly included in the GO; no sudo or other VM access.
If that authority is not approved, this proposal cannot run the reviewed packet.

Generate one ephemeral Ed25519 key pair with a bounded `ssh-keygen -t ed25519`
client, into exclusively created canonical host-only `0700` control root
`/Volumes/Builds/scratch/effigy/531-linux-control`. Keep its key, known-hosts,
config/owner receipts and inspection logs **outside the shared placement root**;
refuse any foreign/unreceipted control root. Never mount or transfer this control
root into the VM. Never
print/commit the private key or load it into an existing agent. Place only its
public key through authenticated Parallels execution in the new account's file,
with `restrict` (no forwarding/PTY/user rc) and an absolute `expiry-time` of at
most **24 hours**, also bounded by the operator-approved lifetime. Unsupported
expiry/password-disabled login refuses rather than weakening authentication.
Pin the observed VM public host key into a private control `known_hosts` file;
re-observe the key over Parallels before the first bounded SSH connection.

Proposed alias `effigy-531-vault` is **not an observed existing alias**. A later
GO must approve the exact new host-config block because the reviewed launcher
calls `/usr/bin/ssh -o BatchMode=yes <alias>` without a `-F` option. Preserve all
existing entries/keys; refuse alias collision and unsafe inherited hooks. The
new block must select the observed IP, user `effigy531`, this exact external
`IdentityFile`/`UserKnownHostsFile`, `IdentitiesOnly yes`, `IdentityAgent none`,
`StrictHostKeyChecking yes`, `UpdateHostKeys no`, no global known-host fallback,
no proxy/local/remote command, no forwarding/agent/PTY, and a 10-second connect
timeout. Do not implement an alternate launcher, wrapper or transport here.
Record the approved config delta/hash and effective nonsecret settings. A single
bounded authentication check may confirm uid/architecture; it is not qualification.

## Narrow removable placement

Propose one exclusively created host-owned `0700` removable root
`/Volumes/Builds/scratch/effigy/531-linux-stage2`, with separate owned
source, artifacts and **empty** qualification children. It contains no host
credentials, private keys or host control files. Freeze device/inode/uid
pins and share metadata before effects. Do not share home, Vault, all Builds,
parent scratch or siblings. No symlinks, copied substitute on the guest system
disk, global automatic share, or adoption of an existing foreign root.

Only if the prerequisite proves it possible, authorize one named Parallels
share of that exact root and its explicit guest mount at the **same absolute
path**. Create only the missing guest parent directories; no persistent fstab
edit, VM restart, snapshot/disk repair or other share reconfiguration. Use only
verified driver options enforcing the dedicated uid/gid and private access.
Record actual source/target/fstype and mount/ownership identities. A bounded
owned temporary filesystem check must establish permissions, stable identity,
locking, Unix sockets and atomic/no-clobber publication needed by custody;
unsupported semantics refuse. Such preparation checks do not replace the real
native custody tests. Leave the later qualification child empty and pin its
actual native identity; host inode values cannot stand in for guest values.

Retain the VM, new account, mount/share, control files and metadata under the
exact recorded disposition. Key expiry limits authentication; it does **not**
prove deletion, account/mount cleanup, VM shutdown or capacity release. No cleanup
runs without a separate exact authorized disposition. Protect existing services,
accounts, keys, engine resources and all other VMs/runtimes.

## Later artifact preparation, not part of this stage

After #574 source stabilization and retained review, separately authorize a
numeric-bounded immutable source/archive transfer and actual native Linux build
bundle through approved Effigy selectors, with exact toolchain/nextest/Oxide and
package/download/cache/image bounds. Missing Rust and non-GNU host coreutils are
observations to reconcile, not installation authority. The reviewed runtime
recipe remains a separately approved image build; no cross-profile image import
or producer-image substitution. Freeze genuine old-binary provenance, real guest
producer tuple, prebuilt tests/metadata and candidate/source binding at that exact
head. Never rebind historical e51 bytes to a later commit.

Persist nonsecret command/PID/exit/deadline records, public-key fingerprints,
identity/mount/source hashes and failures outside ephemeral Queue cleanup and
embed small receipts in PR686. All three backend inputs/GO-index remain
**UNARMED** pending their actual GOs and planner-bound aggregate route; no native
proof, consumer, install, production-enable or merge authority is granted.
