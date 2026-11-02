# Linux Docker workspace-sessions qualification packet — candidate a00394012, r1

Artifacts manifest SHA-256: `34473433183ac27d92b84b2dd1c8dddeb844f6ded9bd852d94297aae9918fe27`

Subject: the merged workspace-session cut at commit
`a00394012ae8dafe52981c36d459ec0491ec0552`, executed on Tom's Parallels
Ubuntu 26.04 ARM64 VM (`Ubuntu 26.04 ARM64`) as the primary Linux Docker
Engine gate. The VM alias comes from this packet's GO receipt (`vm-alias`
line). Environment prerequisites of record (2026-11-02): the VM currently
reports status `invalid` in `prlctl list`; its `clean-base` snapshot and
package live on Vault, and the Vault drive must be attached and verified
before the window (throughput measured before timing-sensitive work; the VM
shut down before removing the drive; no unattended runs without drive
confirmation). Until Tom confirms Docker Engine, SSH, Rust and the
`clean-base` snapshot health, this packet cannot run and the gate stays
blocked — a recorded prerequisite, not native credit.

## Route

The refusal cells (C1) run the host binary against the host-side fixture
`/Users/tom/.effigy-scratch/530-qual-20261102-linuxvm-fixture` exactly as in
the other packets. The guest binary builds natively inside the VM with the
VM's own toolchain — plain Cargo in the disposable build directory
`/home/ubuntu/530-qual-build`, per the Vault-backed Linux proof build rules in
`docs/knowledge/cargo-routes.md`: prepare records the VM and snapshot
identity, the actual `cargo`/`rustc` versions and Oxide's presence or absence
before any source transfer, performs no privileged step, and records the
aarch64 binary checksum. The identity containers (C2) run in the VM's Docker
store from an image resident there, pinned by prepare; an absent image stops
the packet and names the pull in a fresh GO request. The fixture-ancestor
mountpoint check covers host mounts (the fixture lives on the host); the VM
build directory is removed by its exact manifest path in cleanup.

## Cells and blocked prerequisites

C1–C4 are identical in kind to the Colima packet (`../colima/packet-r1.md`):
refusal texts with zero effects, guest identity (schema, exact engine
container ID, UUID boot ID, staged-binary digest, stability, distinctness,
boot-ID relation), resolve refusal by name, and baseline/cleanup equality.
Blocked-by-cut prerequisites are identical: publication, custody, TLS
credential, dial, streaming cells, down/fresh-up, attach and consumer
verification remain refused or unreachable on this candidate.

An exploratory current-main Linux pass, if any, is not candidate
qualification and does not credit this packet.

## GO provenance and receipts

Host-side observable sweep (done 2026-11-02), VM health and toolchain record
(inside the window), Supervisor GO naming this packet's SHA copied verbatim to
`/Volumes/Builds/scratch/effigy/530/530-linux-r1-go.txt` (0600) **including
the `vm-alias <name>` line**, and Tom's explicit live-environment GO. One
execution; receipts and logs are 0600 O_EXCL files under
`/Volumes/Builds/scratch/effigy/530/`. No retry after any failure.
