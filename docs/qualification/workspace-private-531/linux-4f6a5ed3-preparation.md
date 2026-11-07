# Linux producer and native readiness kit for candidate 4f6a5ed3 — planning only

Status: **preparation only**. Nothing in this document has been executed beyond
the Stage0 host reads listed in §2. No guest command, VM start/stop, SSH, copy,
install, image pull, build, account, key, share, snapshot or config change has
run or is authorized by it. This document is the kit revision K. K is not a
replacement candidate, not an artifact source, and needs no candidate rebuild or
fullQA. Candidate H (`4f6a5ed3078bc6e354c8a7738ea5663c8ffefa02`, fullQA
`5449c102` green) and its reviewed executable hashes stay fixed.

Authority: Queue approval "Tom2026-11-07" covers preparation, source-only work and
read-only host VM configuration checks. It does not cover native start, SSH,
copy, install or build. Each of those needs a concrete reviewed stage and its own
operator GO.

Related records: lead `4dd38cd0-d7a0-42c7-8d08-e4e7f5cdfa44` keeps the RecipeE
obligation open. This kit does not satisfy it. Held: 621, 622 and 623 native
activation; 624 aggregate. Historical 531 Linux state (Vault/Builds mount,
`10.211.55.8`, old Docker versions, `effigy531` account and keys, old snapshots
and roots) is retained as history. Nothing here adopts it.

## 1. Fixed inputs

| Input | Value | Check |
| --- | --- | --- |
| Candidate H | `4f6a5ed3078bc6e354c8a7738ea5663c8ffefa02` | fullQA `5449c102` green (627 host handoff) |
| 627 host index | `/Volumes/Stash/scratch/effigy/627-artifact-preparation.6mazyj9u/627-evidence-index.r5.json` | sha256 `a451d6c40783cb3d268534f7b4d84d1d3c9e116892a10248ed6a360cba040198`, re-hashed 2026-11-07 19:27 UTC, matches |
| 627 lane artifact | v79 | sha256 `c606bdb92ff18e48c420fff4758651a37e892046eecf481208bb71e8b34f59e5` (Queue-held; not re-read here) |
| 627 planner verification | `/Volumes/Stash/scratch/effigy/milestone-qualification-fixture-integrated.h39f1dw8/627-planner-host-delivery-verification.json` | Not reopened here |

The 627 host handoff is complete. It is not native readiness. Its host proofs
are not rebuilt or replayed by this kit.

## 2. Stage0 — observed host facts

Method: `prlctl` 27.1.0 (58756) read-only subcommands (`list --all --json`,
`list --info <UUID> --json`, `snapshot-list <UUID> --json`), plus `stat`, `df`
and `shasum` on the exact named VM bundle paths, and one read of the bundle's
`DiskDescriptor.xml` structure. No guest command ran. Raw outputs are in the
run scratch and copied to the retained evidence location in §8.

### 2.1 VM and configuration (2026-11-07 ~19:27 UTC)

| Item | Observed | Notes |
| --- | --- | --- |
| UUID / name | `433c9e5b-c7b9-469f-9633-24b4addcd3b5` / `Ubuntu 26.04 ARM64` | Matches planner |
| State | `stopped` | Autostart off, Autostop `suspend`, Undo disks absent, Restore Image empty, Expiration disabled |
| Home / config | `/Volumes/Stash/parallels/Ubuntu 26.04 ARM64.pvm/` / `config.pvs` | |
| Description | `Effigy Linux proof VM on Vault` | Stale label. Not backing proof. The bundle is on Stash |
| config.pvs | sha256 `1d76ac9c57b29938de5ad2a3d641ca92165c11623c4e8b5487c5059f9dcf68b6`, dev `16777245`, inode `1989129`, uid `501`, mode `0744` | Matches planner |
| .pvm directory | dev `16777245`, inode `1988635`, uid `501`, mode `0755` | Matches planner |
| Disk | `hdd0`, `sata:0`, expanded, `131072Mb`, image `harddisk1.hdd` | |
| Disk descriptor | `harddisk1.hdd/DiskDescriptor.xml` sha256 `974876501b91274d3e433d19f7ee5347fa23f5562b6062c8ccb8c8be194c09a0`, disk UID `{44b5d35e-b053-4aae-9140-266a00bbef9a}`, name `harddisk1 SSD` | Mtime 2026-11-04 20:34 |
| CPU / RAM | 6 vCPU (`arm`), 16384 Mb | |
| Sharing | Host Shared Folders disabled; Host defined sharing `Off`; SmartMount disabled; Shared Profile disabled; Shared Applications enabled with host↔guest app sharing `off` | Keep as is |
| Guest tools | installed, `27.1.0-58756` | Required for `prlctl exec` |
| Other VMs | Windows 11 suspended; macOS pristine stopped | Protected. Not touched |

### 2.2 Snapshot tree and disk extents

- 36 snapshots. Current snapshot `{4d475daf-8aa1-497a-9a38-355f21fcd867}`
  (`effigy-520-r13-exit-427540b`, `poweroff`, `current: true`).
- Current writable extent `harddisk1.hdd.0.{4d475daf-…}.hds`: 5,324,668,928
  bytes, mtime 2026-11-01 18:21. Its `.cache` sibling is 4,207,104 bytes.
- All 37 `.hds` extents total 135,735,017,472 bytes apparent (about 126.4 GiB).
- Bundle entries not inspected further: `harddisk1.hdd` (0-byte file inside the
  bundle, unexplained) and `harddisk1.hdd.drh`. Both are left untouched.
- Stash volume `/Volumes/Stash`: 3.6 Ti size, 2.9 Ti available.

### 2.3 Other host state

- `/Volumes/Builds`: absent (retired mount).
- `/Volumes/Stash/scratch/effigy/531-linux-control`: absent. Stage2 creates it
  under GO. The name keeps "531" because `common.py` binds that path.
- `/Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam`: present,
  mode `0700`, owner `tom`. Original planner observations are intact there.

### 2.4 Custody and disposition after Stage0

| Resource | State after Stage0 | Disposition owner |
| --- | --- | --- |
| VM `433c9e5b…` and its bundle | Unchanged | Operator. Stage1 changes power state only, and only under GO |
| Snapshot tree (36) and extents | Unchanged | Operator. No snapshot change in Stage1 (decision D-1) |
| Guest disk identity | Not yet correlated | Stage1 observes; Stage2 pins |
| Host control dir | Not created | Stage2 under GO |
| Evidence root `l0tx7tam` | Original files untouched; this kit adds a new subdirectory | Retained outside Queue cleanup |
| Run scratch `2d13fe17` | Holds Stage0 raw copies | Nucleus removes after release. The retained copy is in `l0tx7tam` |

Capacity: host volume headroom is large. The binding limits are the guest's own
disk, unknown until running, and the 128 GiB virtual disk ceiling (§6.3).

## 3. Source readings at H — gaps the kit must cover

Each gap names the file and line-level fact the kit relies on. None is changed
here. Gaps that need code are listed as source-owner proposals in §6.4.

- **G1 Canonical guest path is fixed.** `common.py` hard-codes
  `SCRATCH = /Volumes/Stash/scratch/effigy`. `qualification_path` requires an
  exact absolute child of it. `runtime.py` `main` requires
  `SCRATCH.resolve(strict=True) == SCRATCH` inside the guest. The guest must
  therefore present that exact directory. Spelling is not physical proof (G2).
- **G2 Placement provenance is not implemented.** `validate_inputs` checks
  hashes, identities and sizes only. No code checks Stash/Vault backing, device
  correlation or filesystem controls. `linux-setup.md` says so. Its chain is
  proposed, not implemented (SO-2).
- **G3 Guest source identity is not verified.** `launch.py` checks the host repo
  head only (`source_head(root)`). `runtime.py` never checks that the guest tree
  equals H. Guest source integrity holds only if the pre-positioning step proves
  it (SO-1).
- **G4 Remote timeout does not stop the guest.** `launch.py` runs the Linux
  driver with `run(command, timeout=budget_seconds)`. That kills only the local
  `ssh` client. Without a TTY, the guest `runtime.py` can keep running and hold
  the qualification root and one-shot marker. No remote kill route exists (SO-3).
- **G5 Remote pre-positioning is outside source.** `launch.py` assumes
  `common.py`, `runtime.py`, `probe.py`, the inputs file and the GO index are
  already on the guest and hash-verified. No source command places or verifies
  them (SO-1).
- **G6 Producer predicates are partial.** `scripts/build-linux-workspace-artifact.rhai`:
  - checks the triple string, non-empty bytes, the `.active-version` sidecar and a
    `sha256` key in the receipt;
  - checks no ELF class, endianness or `e_machine`;
  - records no toolchain identity;
  - builds in `infra/release/linux/Dockerfile`, which is `ubuntu:22.04` plus
    `rustup … --default-toolchain stable`. The repo's `rust-toolchain.toml` pins
    `1.97.1`, so rustup fetches that toolchain at build time from a floating
    image. Nothing records what was fetched (SO-4).
- **G7 Default target is x86_64.** The rhai defaults `EFFIGY_LINUX_RELEASE_TRIPLE`
  to `x86_64-unknown-linux-gnu`. The kit must set `aarch64-unknown-linux-gnu`
  explicitly. Launch does not check the receipt's `release_triple` against the
  backend.
- **G8 The producer runs on the Darwin host, not in the VM.**
  `config/containers.toml` `[containers."linux-release"]` is a Colima profile
  `effigy-release`. `container::workspace_artifact_build` writes into the bound
  checkout at `/workspace`. Contract 006 gives no route to build inside the
  Parallels guest. Decision D-2 and SO-7 cover this.
- **G9 Every Cargo selector needs `oxide`.** `scripts/oxide-selector.sh` exits
  127 when `oxide` is not on PATH. Its only bypass is
  `EFFIGY_OXIDE_ESCAPE=cargo`, which is a recorded escape and not used here.
  Oxide's pool model (`OXIDE_POOL`, Builds slot) is Darwin-specific. Guest
  availability is unknown (SO-5).
- **G10 Metadata is host-bound.** `build:workspace:private-qualification:cargo-metadata`
  (`oxide metadata --format-version 1 --no-deps`) and the nextest binaries
  metadata both carry absolute workspace/target paths and a host triple. Darwin
  output is unusable on the Linux guest. Metadata must be generated natively
  from the guest source root. `runtime.py` `paths()` checks only the set of
  `binary-path` values, so the copy-and-rewrite step from `README.md` is still
  unexercised for native use (SO-6). The lead asks for this to be covered
  explicitly.
- **G11 Recipe C's kernel fixture route is a test run.**
  `test:workspace:private-qualification:kernel` builds the helper and then runs
  tests. Freezing the helper therefore may mean running a native test. That needs
  its own GO, or a build-only route (SO-5 ruling).
- **G12 Recipe D needs a native old-source build.** Commit `823d859da` is the
  bound old source. Its binary must be built natively in a guest fixture by its
  own covering selector, with the same oxide dependency (G9). Its protocol-4
  admission is a source fact, not a run result. A Darwin binary cannot stand in.
- **G13 Runtime image.** `runtime.Dockerfile` is pinned to
  `ubuntu:22.04@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11`
  and asserts `/lib/ld-linux-aarch64.so.1`. No selector builds it. The compose
  file uses `pull_policy: never`. Pull or build needs a GO (E12).
- **G14 Docker access is root-equivalent.** `ssh_prefix` requires
  `User effigy531`. Docker access therefore needs `effigy531` in the `docker`
  group. That is an explicit effect (E4). No in-policy alternative exists.
- **G15 Endpoint is a numeric IP.** `ssh_prefix` requires `HostName` to be an IP
  literal. The guest IP is not known until running and may change on a shared
  network. A change refuses (fail-closed). Stage1 records the IP without
  pinning it.
- **G16 SSH policy is exact.** The config key set must be exactly the fixed
  policy plus `hostname`, `user`, `identityfile`, `userknownhostsfile`. Any extra
  directive refuses. The key and known-hosts paths must sit under
  `531-linux-control` and outside the qualification root.
- **G17 Budget ceiling.** `validate_inputs` caps `budget_seconds` at 1800. The
  Linux window stays inside that ceiling.
- **G18 Runtime engine check.** `runtime.py` `Window.__init__` requires
  `shutil.which(tool)` on PATH to hash-match `engine_binary`. For Linux that is the
  `docker` CLI, so its exact bytes must be pinned in Stage2.

## 4. Recipe disposition for the Linux backend

| Recipe | Current route | Native Linux route at H | Status / blocker |
| --- | --- | --- | --- |
| A — production binary | `build:runtime-storage` → `oxide build --bin effigy` | Guest build from frozen source archive | Blocked on G9 (oxide) |
| B — candidate and utility | `build:workspace:private-qualification` | Guest build | Blocked on G9 |
| C — test executables, kernel helper, nextest, metadata | `build:workspace:private-qualification:test-inputs`, `…:cargo-metadata`, `test:workspace:private-qualification:kernel` (builds helper) | Guest build and native metadata. Helper via build-only route if one exists | Blocked on G9, G10, G11 |
| D — genuine old baseline `823d859da` | Its own covering selector | Native guest fixture at `823d859da` | Blocked on G9, G12. Old commit's own selector must be confirmed at that commit |
| E — guest artifact | `workspace:linux:artifact` (Darwin host, Colima `effigy-release`) | Same route at H with `EFFIGY_LINUX_RELEASE_TRIPLE=aarch64-unknown-linux-gnu`, copied into guest source at `.effigy/linux-release/artifacts/` | Producer route exists. ELF and toolchain predicates added by kit (§4.1). Colima arch unobserved |

### 4.1 Recipe E predicates

The existing producer meets:

- receipt keys `binary`, `copied_binary`, `release_triple`, `sha256`,
  `build_kind=workspace-artifact`, `completed_at` (Recipe E schema);
- active-version sidecar equal to the host `effigy::active_version()`.

The kit adds these, read-only, on the frozen copy before freezing:

- ELF header: magic `\x7fELF`, `EI_CLASS=ELFCLASS64` (2), `EI_DATA=ELFDATA2LSB` (1),
  `e_machine=EM_AARCH64` (183), `e_type` ET_EXEC (2) or ET_DYN (3);
- `sha256` of the copied bytes equals the receipt `sha256`;
- receipt `release_triple` equals `aarch64-unknown-linux-gnu`;
- Colima `effigy-release` profile observed as aarch64 (Stage2 precondition;
  an emulated x86_64 builder refuses);
- toolchain identity: `cargo -V` and `rustc -Vv` from inside the builder, if the
  existing owned-builder exec can run them under the E14 GO. If not, toolchain
  identity stays recorded missing and SO-4 carries it.

Decision D-2. Recipe E is built by the Darwin-side Colima aarch64 builder, not
inside the Parallels guest. Reason: it is the only supported producer route
(G8). The target is an aarch64 Linux ELF built in an aarch64 Linux container.
That is native Linux target output, not host-target (Darwin) compilation. It is
not an emulated or cross-target label. The guest executes it natively. If the
owner rules that the guest itself must build it, that needs SO-7.

## 5. Stage1 — one bounded VM start and read-only guest readiness (prepared, NOT executed)

### 5.1 Scope

- Exact UUID only: `433c9e5b-c7b9-469f-9633-24b4addcd3b5`.
- Total budget: 600 s. Per-step deadlines below. Each host subprocess runs in its
  own session; its PID and PGID are recorded before waiting. On deadline only
  that process group is killed. No pattern kills.
- No `--user`, `--password`, `--kill`, `--acpi`, `--drop-state` or `--current-user`
  flags. No password or credential payload. The probe runs as whatever default
  guest context `prlctl exec` gives. Its first line records `id -u`.
- No snapshot change, no config change, no shared-folder change, no network or
  SSH action, no package, build, pull or write in the guest.

### 5.2 Pre-state gate (refuse before start on any mismatch)

- `state` = `stopped`; `Home` and config path as §2.1.
- config.pvs sha256 `1d76ac9c…f68b6`, inode `1989129`.
- DiskDescriptor.xml sha256 `974876501b…c09a0`.
- Snapshot count 36; current `{4d475daf-…}`.
- Shared Folders disabled; Host defined sharing `Off`.
- Host `/Volumes/Stash` mounted; evidence directory writable.

### 5.3 Steps

| Step | Exact argv | Deadline | Output limit | Success condition |
| --- | --- | --- | --- | --- |
| S1.1 pre-observe | `/usr/local/bin/prlctl list --info 433c9e5b-c7b9-469f-9633-24b4addcd3b5 --json` | 10 s | 64 KiB | Pre-state gate (§5.2) |
| S1.1b pre-snapshots | `/usr/local/bin/prlctl snapshot-list 433c9e5b-c7b9-469f-9633-24b4addcd3b5 --json` | 10 s | 256 KiB | Count 36, current `{4d475daf-…}` |
| S1.2 start | `/usr/local/bin/prlctl start 433c9e5b-c7b9-469f-9633-24b4addcd3b5`, then poll S1.1 every 5 s | 145 s including polls | 16 KiB | `State` = `running` |
| S1.3 readiness | `/usr/local/bin/prlctl exec 433c9e5b-c7b9-469f-9633-24b4addcd3b5 /bin/echo ready`, retried every 5 s | 90 s | 1 KiB | stdout exactly `ready` |
| S1.4 probe | `/usr/local/bin/prlctl exec 433c9e5b-c7b9-469f-9633-24b4addcd3b5 /bin/sh -c <PROBE>` | 75 s | 256 KiB stdout, 16 KiB stderr | Exit 0; the probe's `uid=` line is recorded |
| S1.5 graceful stop | `/usr/local/bin/prlctl stop 433c9e5b-c7b9-469f-9633-24b4addcd3b5`, then poll S1.1 every 5 s | 120 s | 16 KiB | `State` = `stopped` |
| S1.6 post-observe | S1.1, S1.1b, `stat` of config.pvs and current `.hds` (sizes and mtimes only) | 30 s | 64 KiB | State `stopped`; config sha and inode unchanged; snapshot tree unchanged; `.hds` growth recorded |
| S1.7 retain | Write every raw output, argv, PID/PGID, exit, timestamps into a fresh 0700 directory below `l0tx7tam` | within reserve | — | Manifest with sha256 of each file |

Budget: 20 (S1.1, S1.1b) + 145 (S1.2) + 90 (S1.3) + 75 (S1.4) + 120 (S1.5) +
30 (S1.6) = 480 s. Reserve for S1.7 and reporting: 120 s. Total 600 s.

### 5.4 Refusal and retention

- Pre-state mismatch: refuse before start. No effect.
- Start fails or does not reach `running` in 145 s: skip to S1.6, retain, report.
- Readiness fails in 90 s: do not run the probe. Attempt S1.5 only. If stop
  fails, retain running and report.
- Probe deadline, stdout or stderr over cap, or non-zero exit: kill only our
  host `prlctl` process group. **Do not stop the VM.** A guest process may
  still run and cannot be identified safely. Retain running and report to the
  operator.
- Probe succeeds but `uid` is not `0`: record as partial. Stage2 refuses until a
  root context is observed. Still attempt S1.5.
- Graceful stop fails within 120 s: retain running, report. No `--kill` without a
  separate GO.
- Post-observe mismatch (config sha, inode, snapshot tree, state): refuse, retain
  everything, report.

### 5.5 Return to stopped, and what changes (decision D-1)

- Original stopped state: VM `stopped`, config as §2.1, current snapshot
  `{4d475daf-…}` `poweroff`, 36 snapshots.
- Return method: the same VM's graceful stop. No snapshot restore.
- Disk does not return byte-identical. Boot and probe writes land in the current
  `.hds` extent. Post-observe records its size and mtime. Cap: growth at most
  1 GiB. Anything larger refuses. Stage1 takes no pre-start snapshot. A
  pre-start snapshot would add a delta extent of its own, so it would need a
  separate GO. The kit records this divergence rather than hiding it.

### 5.6 Probe script (`<PROBE>`, POSIX sh, read-only)

The script below is the exact text Stage1 passes after `-c`. Its sha256 goes in
the manifest (§8). It writes nothing, starts nothing, makes no network calls and
reads no user home or credential file.

```sh
LC_ALL=C
export LC_ALL
section() { printf '== %s\n' "$1"; }
section identity
echo "uid=$(id -u) user=$(id -un 2>/dev/null)"
echo "arch=$(uname -m) kernel=$(uname -r) dpkg_arch=$(dpkg --print-architecture 2>/dev/null)"
echo "boot_id=$(cat /proc/sys/kernel/random/boot_id 2>/dev/null)"
echo "glibc=$(ldd --version 2>/dev/null | head -n 1)"
section os
sed -n '1,12p' /etc/os-release 2>/dev/null
section systemd
echo "system_state=$(systemctl is-system-running 2>/dev/null)"
echo "ssh_service=$(systemctl is-active ssh.service 2>/dev/null) ssh_socket=$(systemctl is-active ssh.socket 2>/dev/null)"
section mounts_and_storage
findmnt -J -T / 2>/dev/null
echo
findmnt -J -T /Volumes/Stash/scratch/effigy 2>&1
echo
df -PT / 2>/dev/null
section block_devices
lsblk -J -b -o NAME,PATH,KNAME,PKNAME,TYPE,SIZE,FSTYPE,MOUNTPOINTS,UUID,PARTUUID,MODEL,SERIAL,VENDOR,HCTL 2>/dev/null
section network
ip -4 -o addr show scope global 2>/dev/null
ss -H -ltn 2>/dev/null | awk '$4 ~ /:22$/'
section ssh
ssh -V 2>&1
sshd -T 2>/dev/null | grep -E '^(port|listenaddress|pubkeyauthentication|passwordauthentication|kbdinteractiveauthentication|permitrootlogin|usepam|authorizedkeysfile|allowusers|allowgroups|pubkeyacceptedalgorithms) '
ssh-keygen -l -f /etc/ssh/ssh_host_ed25519_key.pub 2>/dev/null
section accounts
getent passwd effigy531 >/dev/null 2>&1; echo "effigy531_present_exit=$?"
getent group docker 2>/dev/null | cut -d: -f1-3
section tools
for t in docker docker-compose python3 git cargo rustc rustup oxide sha256sum readelf file sshd man; do
  printf '%s=%s\n' "$t" "$(command -v "$t" 2>/dev/null || echo absent)"
done
docker version --format 'docker_server={{.Server.Version}} {{.Server.Os}}/{{.Server.Arch}}' 2>&1
docker compose version --short 2>&1
section engine
docker context ls --format '{{.Name}} current={{.Current}} endpoint={{.DockerEndpoint}}' 2>&1
docker info --format 'driver={{.Driver}} root={{.DockerRootDir}} arch={{.Architecture}}' 2>&1
section engine_inventory
docker ps -a --no-trunc --format '{{.ID}}' 2>&1
docker network ls --format '{{.Name}}' 2>&1
docker volume ls --format '{{.Name}}' 2>&1
docker image ls --no-trunc --format '{{.ID}} {{.Repository}}:{{.Tag}}' 2>&1 | head -n 200
section end
```

Notes on the probe:

- `sshd -T`, `docker`, `findmnt` and the `ip`/`ss` reads need uid 0. The probe
  records `uid` first, so a non-root context is visible in the output.
- `docker ps -a`, network and volume lists are the engine baseline inventory
  inputs (E12 pins them). They are observation only.
- `sshd -T` values are public config facts. `ssh-keygen -l` prints only the
  fingerprint, not the key.
- Whether `expiry-time` works in authorized_keys is not answered by the probe. The
  kit requires an `sshd_config(5)` / `authorized_keys(5)` check first. See E6.

### 5.7 What Stage1 can and cannot establish

Can establish, once it runs: guest uid, arch and OS; boot identity; root
device, block devices and their serials (for correlation); whether the guest
root can present `/Volumes/Stash/scratch/effigy` natively; current IP; sshd
config and host key fingerprint; tool presence (oxide, rustup, cargo, docker
versions); engine context names, image IDs and baseline inventory.

Cannot know until running, and must not be invented: the guest's current IP
or host key (Stage2 pins the key via Parallels), a correlation identifier
between the guest block device and disk UID `{44b5d35e-…}` (the guest may not
expose it: refusal if absent), Colima profile arch, guest free space beyond
`df`, whether `oxide` can run on Linux, and whether the installed sshd supports
`expiry-time`.

## 6. Stage2 — contingent setup and build plan (prepared, NOT executed)

Stage2 runs only after Stage1 is accepted and a separate Stage2 GO names its
decision ID. Each effect below carries its numeric bound and its disposition.
If a prerequisite is absent, the step refuses. Nothing is set up automatically.

### 6.1 Preconditions

- Stage1 result accepted: uid 0 observed; storage correlation proven or
  explicitly refused; tools as needed.
- SO-1 to SO-6 resolved, or the owner names the explicit manual step that
  replaces them. This is required for SO-3 and SO-5 in particular.
- Colima `effigy-release` profile observed as aarch64 (for E14 only).

### 6.2 Effects table

| # | Effect | Bound | Disposition |
| --- | --- | --- | --- |
| E1 | Host: create `/Volumes/Stash/scratch/effigy/531-linux-control` mode `0700`, owned by `tom` | 1 dir | Retained. Removal needs separate disposition |
| E2 | Host: generate one ephemeral Ed25519 key, mode `0600`, in E1 | 1 key, expiry ≤ 24 h | Deleted at Stage2 end or retained under owner disposition |
| E3 | Guest: create account `effigy531`, password login locked, home mode `0700`, no sudo | 1 account | Retained. Removal needs separate disposition |
| E4 | Guest: add `effigy531` to `docker` | **root-equivalent**; explicit GO | Retained. Removal needs separate disposition |
| E5 | Guest: sshd drop-in for `Match User effigy531` (`PasswordAuthentication no`, `AllowTcpForwarding no`, `X11Forwarding no`, `PermitTTY no`) and `reload` | 1 file, 1 reload | Retained. Revert needs separate disposition |
| E6 | Guest: `authorized_keys` with `restrict,expiry-time=<≤24 h>` only if the installed sshd supports it. Otherwise refuse, with no weaker auth | 1 file | Expires. Removal needs separate disposition |
| E7 | Guest: create `/Volumes/Stash/scratch/effigy` on the **native root ext4**, then `linux-4f6a5ed3/` mode `0700`. No mount, no fstab edit, no share | ≤ 10 dirs | Retained |
| E8 | Guest: placement and filesystem controls: Unix sockets, descriptor transfer, locks, atomic no-clobber publication, fsync/readback. Helper per SO-2 | ≤ 60 s, ≤ 64 MiB writes | Faults retained |
| E9 | Host → guest: `git archive 4f6a5ed3078bc6e354c8a7738ea5663c8ffefa02`, pinned sha256, sent over the pinned SSH stdin per SO-1. Guest extracts into `…/source` (mode `0700`) and verifies a tree manifest | Archive ≤ 40 MiB (measured 31.6 MB, 2,140 tracked files); extracted ≤ 500 MiB | Retained as the frozen source root. Not a Queue checkout |
| E10 | Guest: Rust `1.97.1` (repo pin) with `rustup`, with `RUSTUP_HOME`/`CARGO_HOME` under `…/toolchain` | Download ≤ 1.5 GiB, installed ≤ 4 GiB, network allowlist (§6.5) | Retained |
| E11 | Guest: `oxide` on PATH (SO-5). Absent means refuse. `EFFIGY_OXIDE_ESCAPE=cargo` only by owner ruling and a recorded `cargo-routes.md` entry | 0 extra bytes unless installed | — |
| E12 | Guest: runtime image. `docker pull ubuntu:22.04@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11`, then `docker build --pull=false -f runtime.Dockerfile`. Verify `/lib/ld-linux-aarch64.so.1`, `python3`, arm64. Record the image ID | Pull ≤ 1 GiB; apt ≤ 300 MiB; engine disk ≤ 2 GiB | Image retained. Removal needs disposition |
| E13 | Guest builds, one per commit: A (`build:runtime-storage`), B (`build:workspace:private-qualification`), C (test-inputs, cargo-metadata, helper), D (at `823d859da` in its own fixture). Each selector stays inside its `tasks.toml` timeout (≤ 900 s for build-only selectors). Total ≤ 3,600 s per commit | Target dirs ≤ 25 GiB total; cargo registry ≤ 4 GiB | Frozen copies kept; build trees kept as evidence |
| E14 | Host: Recipe E `EFFIGY_LINUX_RELEASE_TRIPLE=aarch64-unknown-linux-gnu effigy workspace:linux:artifact` at clean H, Colima `effigy-release`. Separate builder GO. Verify §4.1 predicates | ≤ 1,800 s; artifact ≤ 200 MiB | Producer receipt kept |
| E15 | Host → guest: copy E's artifact bytes and its `.receipt` and `.active-version` into the guest source at `.effigy/linux-release/artifacts/`. Verify sha256 on both sides | ≤ 200 MiB | Retained |
| E16 | Freeze: copy every executable (A, B, utility, kernel helper, test binaries, nextest, D old binary, E artifact) into `…/frozen/`, mode `0500`, hash each, write receipts | ≤ 1 GiB | Retained with manifest |
| E17 | Guest: native metadata generation (cargo-metadata, binaries-metadata) from the frozen source root. Rewrite only per SO-6 | ≤ 60 s | Hash recorded |
| E18 | Stage2 teardown: stop and remove only what this stage created, by recorded ID/path. Keep frozen copies, receipts and evidence | — | Per E-row disposition |

### 6.3 Capacity and time bounds (Stage2 total)

- Host writes: ≤ 5 GiB (E9 archive, E14 artifact, evidence).
- Guest writes: ≤ 40 GiB total (E10 toolchain 4 GiB, E12 engine 2 GiB, E13 build trees 25 GiB, registry 4 GiB, E9 source 0.5 GiB, E16 frozen 1 GiB). Refuse above this, or when `df` shows less than 20% free.
- Network: static.rust-lang.org; index.crates.io and static.crates.io; Ubuntu archive or ports mirror; registry-1.docker.io for the one pinned digest; nothing else. Host side: the Colima builder's own pull, under E14 only.
- Wall clock: ≤ 4 h for all of Stage2. Each step's deadline is in §6.2.
- Virtual disk ceiling: 128 GiB (`131072Mb`). The current `.hds` is about 5 GiB. The guest free space and the post-Stage1 growth decide headroom.

### 6.4 Source-owner proposals (no code change in this task)

- **SO-1 Remote source and driver pre-positioning.** A reviewed helper that
  transfers the archive by pinned SSH stdin, verifies its sha256, extracts it
  into a `0700` canonical root and writes a manifest. It also checks the
  driver, inputs and GO index hashes on the guest. Closes G3, G5.
- **SO-2 Placement provenance and filesystem controls.** A checker that refuses
  unless the host provenance receipt (Parallels config hash, DiskDescriptor UID),
  guest correlation receipt, native mount source/target/fstype/device, and E8
  controls are all present and bound. Closes G2. Follows `linux-setup.md`.
- **SO-3 Remote timeout and orphan control.** The host budget must reach the
  guest. Options: a guest-side `timeout` with a recorded PID or PGID, or a
  guest-side kill route by that PID. Closes G4.
- **SO-4 Producer predicates.** In the rhai: ELF class and `e_machine` check;
  pinned toolchain identity recorded in the receipt; refuse the default x86_64
  triple for the Linux backend. Closes G6, G7. This is a producer defect, so it
  goes to the source owner. It is not patched in artifact execution.
- **SO-5 Oxide on Linux or a recorded escape.** Either oxide runs in the guest
  with a guest-local pool, or the owner rules on a recorded
  `EFFIGY_OXIDE_ESCAPE=cargo` route. Also a ruling on whether Recipe C's helper
  needs a build-only route. Closes G9, G11.
- **SO-6 Native nextest metadata.** A generator for binaries metadata that
  checks the pinned nextest version's schema. It names only the frozen
  executables, and its source-root paths are the guest's. Closes G10.
- **SO-7 In-VM producer (only if ruled needed).** A route that builds the
  artifact inside the Parallels guest. No such route exists today (G8). Not
  needed if D-2 is accepted.

### 6.5 Network and identity effects are explicit

E3 and E4 create a root-equivalent path into the VM, and E5 and E6 change sshd.
Each needs an explicit owner GO naming the account, group and config effect.
A GO for E1 to E2 does not cover E3 to E6. No sudo rights, no other VM access.
Removal of any of these is separate disposition.

## 7. Stage3 — handoff preconditions (no execution)

Held and unchanged: 621 (Colima), 622 (Desktop), 623 (Linux full-session),
624 (aggregate). This kit does not arm any of them.

623 can proceed only when all of these are true:

1. Stage1 accepted, with its raw outputs retained.
2. Stage2 frozen cells recorded, with hashes for A, B, utility, kernel helper,
   test executables, nextest, the D old binary and the E artifact.
3. E's predicates (§4.1) pass, including the aarch64 ELF check.
4. Placement receipts (SO-2) and filesystem controls (E8) retained and reviewed.
5. Source and driver pre-positioning verified (SO-1). Remote hashes match the
   inputs and GO index.
6. Linux GO receipt covering this exact source head, inputs hash, budget and
   engine identity. The decision ID is the operator's. Its one-use marker is
   unclaimed.
7. The runtime image (E12) is present by ID.
8. SO-3 timeout control resolved, or an explicit owner-accepted manual control.

624 follows 623 and reads its original receipts. Nothing here grants production
or consumer credit.

**Next operator action** (asked only after the author and the retained reviewer
both find this plan ready): approve Stage1 exactly as §5. That is one start of
UUID `433c9e5b-c7b9-469f-9633-24b4addcd3b5`, one read-only probe, one graceful
stop, 600 s, no snapshot, no `--kill`, no SSH. Reply with the decision ID.

## 8. Retained copy, manifest and kit hash

- Retained kit copy and manifest: a new 0700 subdirectory below
  `/Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam/`. It holds
  the K document bytes, the extracted probe script, the Stage0 raw captures and
  a manifest with each sha256. Original planner files there stay untouched.
- K document sha256: recorded in the PR body and the manifest. It is computed
  from the committed file.
- Probe script sha256: recorded in the manifest, computed from the extracted
  §5.6 block.

## 9. Remaining unknowns and decisions

Unknowns, carried to Stage1 or Stage2 and not guessed here: current IP and host
key; guest disk-correlation identifier; guest free space; oxide availability on
Linux; Colima `effigy-release` arch; installed sshd `expiry-time` support;
whether a docker daemon already runs; runtime image presence; docker context
names; the nextest schema at its pinned version; toolchain identity inside the
builder; whether `harddisk1.hdd` (zero-byte) is referenced.

Decisions taken in this kit (reviewer checks these):

- **D-1** Stage1 takes no pre-start snapshot. Return is a graceful stop of the same VM. The disk diverges and the divergence is recorded.
- **D-2** Recipe E is built by the existing Colima aarch64 producer on the host, not in the guest. The target is explicit `aarch64-unknown-linux-gnu`.
- **D-3** Recipes A to D for Linux are built natively in the guest from the frozen source archive, never from Darwin binaries.
- **D-4** Stage1 uses one `prlctl exec` read-only probe script, root context required, no password.
- **D-5** Stage2 SSH uses a passphrase-less ephemeral Ed25519 key with expiry ≤ 24 h and restrictions. If expiry is unsupported, refuse.
- **D-6** No `--kill` in any stage without its own GO.
