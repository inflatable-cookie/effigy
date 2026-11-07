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
| Old source (Recipe D) | `823d859dace2cbaa848c8ad5a0b5bdfd8959fdf1` | Full SHA resolved in this repository. Its own tree carries the same selector names |
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
- Historical base `clean-base` `{a4b2b55c-875e-40a3-82f2-3dc3351d8339}`, `poweroff`.
  `cargo-routes.md` names this as the VM's snapshot identity for Linux proof
  builds. It is recorded as identity only. It is not a restore target (D-1).
- Current writable extent `harddisk1.hdd.0.{4d475daf-…}.hds`: 5,324,668,928
  bytes, mtime 2026-11-01 18:21. Its `.cache` sibling is 4,207,104 bytes.
- All 37 `.hds` extents total 135,735,017,472 bytes apparent (about 126.4 GiB).
- Bundle entries not inspected further: `harddisk1.hdd` (0-byte file inside the
  bundle, unexplained) and `harddisk1.hdd.drh`. Both are left untouched.
- Stash volume `/Volumes/Stash`: 3.6 Ti size, 2.9 Ti available.

### 2.3 Other host state

- `/Volumes/Builds`: absent (retired mount).
- `/Volumes/Stash/scratch/effigy/531-linux-control`: absent. Stage2 creates it
  under GO. The name keeps "531" because `common.py` binds that path. If it
  exists at Stage2 time, Stage2 refuses. Nothing is adopted.
- `/Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam`: present,
  mode `0700`, owner `tom`. Original planner observations are intact there.

### 2.4 Custody and disposition after Stage0

| Resource | State after Stage0 | Disposition owner |
| --- | --- | --- |
| VM `433c9e5b…` and its bundle | Unchanged | Operator. Stage1 changes power state only, and only under GO |
| Snapshot tree (36) and extents | Unchanged | Operator. No snapshot change in Stage1 (decision D-1) |
| Guest disk identity | Not yet correlated | Stage1 observes; Stage2 pins |
| Host control dir | Not created | Stage2 under GO |
| Evidence root `l0tx7tam` | Original files untouched; this kit adds new subdirectories per revision | Retained outside Queue cleanup |
| Run scratch `fba20bf8` and `2d13fe17` | Hold raw Stage0 copies | Nucleus removes after release. The retained copy is in `l0tx7tam` |

Capacity: host volume headroom is large. The binding limits are the guest's own
disk, unknown until running, and the 128 GiB virtual disk ceiling (§6.3).

## 3. Source readings at H — gaps the kit must cover

Each gap names the file and fact the kit relies on. None is changed here. Gaps
that need code are listed as source-owner proposals in §6.4.

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
  equals H. Guest source integrity holds only if a pre-positioning step proves
  it (SO-1).
- **G4 Remote timeout does not stop the guest.** `launch.py` runs the Linux
  driver with `run(command, timeout=budget_seconds)`. That kills only the local
  `ssh` client. Without a TTY, the guest `runtime.py` can keep running and hold
  the qualification root and one-shot marker. No remote kill route exists (SO-3).
- **G5 Remote hash verification exists, placement does not.** Before each Linux
  run, `launch.py` verifies over SSH that the guest's `common.py`, `runtime.py`
  and `probe.py` (under `remote.packet`), the inputs file (`remote.inputs`) and
  the GO index (`remote.go_bindings`) match the host's bytes. It then runs the
  guest `runtime.py` and retrieves the receipt by hash. Nothing in source places
  those files on the guest, and nothing verifies the guest source tree or the
  guest GO receipt (SO-1). The placement is the open gap, not the hash check.
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
- **G9 Guest has no Oxide, and the repo already sanctions an escape for it.**
  `scripts/oxide-selector.sh` exits 127 when `oxide` is not on PATH. The
  "Vault-backed Linux proof build" section of `docs/knowledge/cargo-routes.md`
  covers this guest. It sanctions `EFFIGY_OXIDE_ESCAPE=cargo effigy <selector>`
  for that disposable guest and that exact selector, with the recorded need
  "guest without Oxide". It forbids installing Oxide in the guest and forbids a
  blanket Cargo fallback. The kit uses this escape per selector (D-7). It
  names no raw `cargo` command. Oxide's pool model is Darwin-specific, so
  Oxide is not made to work in the guest.
- **G10 Metadata is host-bound.** `build:workspace:private-qualification:cargo-metadata`
  (`oxide metadata --format-version 1 --no-deps`) and the nextest binaries
  metadata both carry absolute workspace/target paths and a host triple. Darwin
  output is unusable on the Linux guest. Metadata must be generated natively
  from the guest source root. `runtime.py` `paths()` checks only the set of
  `binary-path` values, so the copy-and-rewrite step from `README.md` is still
  unexercised for native use (SO-6). The lead asks for this to be covered
  explicitly.
- **G11 Recipe C's kernel helper route is a native test run.**
  `test:workspace:private-qualification:kernel` (`tasks.toml`) builds the
  `kernel-custody-fixture` helper with `--features fault-injection,workspace-private-qualification`
  (timeout 600 s) and then runs nextest over `custody::tests::`, `run::lifecycle::tests::`
  and `workspace_private_` (timeout 900 s). Freezing the helper through that
  selector therefore runs native tests in the guest. The kit makes that an
  explicit effect (E13k) with its own GO. It is not hidden inside a build step.
  A build-only route would replace it (SO-5).
- **G12 Recipe D needs a native old-source build.** Commit
  `823d859dace2cbaa848c8ad5a0b5bdfd8959fdf1` is the bound old source. Its binary
  must be built natively in a guest fixture by its own covering selector (G9
  escape applies). Its protocol-4 admission is a source fact, not a run result.
  A Darwin binary cannot stand in.
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
- **G19 GO receipt path is spelled identically on both sides.** `launch.py`
  resolves `artifact(go_index[backend])` on the host. The guest `runtime.py`
  resolves the same index entry on the guest. The GO receipt therefore must
  exist at the same canonical path, with identical bytes, on host and guest
  (binding table §6.2a).

## 4. Recipe disposition for the Linux backend

| Recipe | Current route | Native Linux route at H | Status / blocker |
| --- | --- | --- | --- |
| A — production binary | `build:runtime-storage` → `oxide build --bin effigy` | Guest build from frozen source archive, via the sanctioned per-selector escape (D-7) | Ready after Stage2 E13a. Blocked only on the GO |
| B — candidate and utility | `build:workspace:private-qualification` | Guest build, same escape | Same as A |
| C — test executables, nextest, metadata | `build:workspace:private-qualification:test-inputs`, `…:cargo-metadata` (builds), `test:workspace:private-qualification:kernel` (builds helper and runs tests) | Guest build and native metadata (E13a, E17). Helper through E13k with its own GO | E13k is a native test run. Its GO must name it. Metadata per SO-6 |
| D — genuine old baseline `823d859da` | That commit's own covering selector (`build:runtime-storage`) | Native guest fixture at `823d859da`, its own tree and its own selector text | Launcher question (SO-5a). Default: that commit's own script, no H code |
| E — guest artifact | `workspace:linux:artifact` (Darwin host, Colima `effigy-release`) | Same route at H with `EFFIGY_LINUX_RELEASE_TRIPLE=aarch64-unknown-linux-gnu`, copied into the guest source at `.effigy/linux-release/artifacts/` | Producer route exists. ELF and toolchain predicates added by kit (§4.1). Colima arch unobserved |

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

The RecipeE artifact must be retained unmodified, with its receipt and
active-version sidecar. The kit does not rewrite them or rebuild them from a
hash.

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
  SSH action, no package, build, pull, service start, or write in the guest.
  Docker daemon reads run only if `docker.service` is already active (§5.6).

### 5.2 Pre-state gate (refuse before start on any mismatch)

- `state` = `stopped`; `Home` and config path as §2.1.
- config.pvs sha256 `1d76ac9c…f68b6`, inode `1989129`.
- DiskDescriptor.xml sha256 `974876501b…c09a0`.
- Snapshot count 36; current `{4d475daf-…}`; `clean-base` `{a4b2b55c-…}` present.
- Shared Folders disabled; Host defined sharing `Off`.
- Host `/Volumes/Stash` mounted; evidence directory writable.

### 5.3 Steps

| Step | Exact argv | Deadline | Output limit | Success condition |
| --- | --- | --- | --- | --- |
| S1.1 pre-observe | `/usr/local/bin/prlctl list --info 433c9e5b-c7b9-469f-9633-24b4addcd3b5 --json` | 10 s | 64 KiB | Pre-state gate (§5.2) |
| S1.1b pre-snapshots | `/usr/local/bin/prlctl snapshot-list 433c9e5b-c7b9-469f-9633-24b4addcd3b5 --json` | 10 s | 256 KiB | Count 36, current `{4d475daf-…}`, `clean-base` present |
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
reads no user home or credential file. Docker reads use an empty `DOCKER_CONFIG`
so root's Docker config is not read. Daemon reads run only when `docker.service`
is already active, because a Docker client call can socket-activate the daemon.

```sh
LC_ALL=C
export LC_ALL
DOCKER_CONFIG=/var/empty/effigy-stage1
export DOCKER_CONFIG
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
echo "docker_service=$(systemctl is-active docker.service 2>/dev/null) docker_socket=$(systemctl is-active docker.socket 2>/dev/null)"
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
f=/usr/share/man/man8/sshd.8.gz
if [ -f "$f" ]; then echo "sshd_man_expiry_time_mentions=$(zcat "$f" 2>/dev/null | grep -c 'expiry-time')"; else echo "sshd_man=absent"; fi
section accounts
getent passwd effigy531 >/dev/null 2>&1; echo "effigy531_passwd_exit=$?"
getent group effigy531 >/dev/null 2>&1; echo "effigy531_group_exit=$?"
if [ -e /home/effigy531 ]; then echo "effigy531_home=present"; else echo "effigy531_home=absent"; fi
getent group docker 2>/dev/null | cut -d: -f1-3
section tools
for t in docker docker-compose python3 git cargo rustc rustup oxide sha256sum readelf file sshd zcat; do
  printf '%s=%s\n' "$t" "$(command -v "$t" 2>/dev/null || echo absent)"
done
for t in cargo rustc oxide; do
  if command -v "$t" >/dev/null 2>&1; then printf '%s_version=%s\n' "$t" "$("$t" --version 2>&1 | head -n 1)"; fi
done
section docker_client
docker compose version --short 2>&1
docker context ls --format '{{.Name}} current={{.Current}} endpoint={{.DockerEndpoint}}' 2>&1
section docker_daemon
if [ "$(systemctl is-active docker.service 2>/dev/null)" = active ]; then
  docker version --format 'docker_server={{.Server.Version}} {{.Server.Os}}/{{.Server.Arch}}' 2>&1
  docker info --format 'driver={{.Driver}} root={{.DockerRootDir}} arch={{.Architecture}}' 2>&1
  section engine_inventory
  docker ps -a --no-trunc --format '{{.ID}}' 2>&1
  docker network ls --format '{{.Name}}' 2>&1
  docker volume ls --format '{{.Name}}' 2>&1
  docker image ls --no-trunc --format '{{.ID}} {{.Repository}}:{{.Tag}}' 2>&1 | head -n 200
else
  echo "daemon_reads=skipped docker.service not active; no start attempted"
fi
section end
```

Notes on the probe:

- `sshd -T`, `findmnt` and the `ip`/`ss` reads need uid 0. The probe records
  `uid` first, so a non-root context is visible in the output.
- `docker.service` inactive means the engine baseline is not captured. Starting
  it is an explicit Stage2 effect (E0), not a probe action.
- `docker ps -a`, network and volume lists are the engine baseline inputs
  (E12 pins them). They are observation only.
- `sshd -T` values are public config facts. `ssh-keygen -l` prints only the
  fingerprint, not the key. The man page is read from its file, with no man
  database.
- `expiry-time` support is read from the installed `sshd(8)` text, if present.
  A missing man page or a zero count makes E6 refuse.

### 5.7 What Stage1 can and cannot establish

Can establish, once it runs: guest uid, arch and OS; boot identity; root
device, block devices and their serials (for correlation); whether the guest
root can present `/Volumes/Stash/scratch/effigy` natively; current IP; sshd
config and host key fingerprint; tool presence and versions (`cargo`, `rustc`,
`oxide`, docker client); whether `effigy531` or its group or home already exists;
the Docker client context set; and, only if the daemon is already active, the
daemon version, image IDs and baseline inventory.

Cannot know until running, and must not be invented: the guest's current IP
or host key (Stage2 pins the key via Parallels), a correlation identifier
between the guest block device and disk UID `{44b5d35e-…}` (the guest may not
expose it: refusal if absent), Colima profile arch, guest free space beyond
`df`, whether the installed sshd supports `expiry-time`, and the Docker daemon
state if `docker.service` is inactive.

## 6. Stage2 — contingent setup and build plan (prepared, NOT executed)

Stage2 runs only after Stage1 is accepted and planner-reviewed exact-stage
activation under the standing authority is recorded with its decision ID.
Each effect below carries its numeric bound and its disposition. If a
prerequisite is absent, the step refuses. Nothing is set up automatically.

### 6.1 Preconditions

- Stage1 result accepted: uid 0 observed; `effigy531` absent (E3 otherwise
  refuses); `oxide` absence recorded (E11 route applies) or present (then the
  cargo-routes contradiction rule stops before source transfer); storage
  correlation proven or explicitly refused.
- SO-1 to SO-4 resolved, or the owner names the explicit manual step that
  replaces each one. SO-5a (D launcher) is required before E13b. SO-5b (E13k)
  is covered by E13k's own GO.
- Colima `effigy-release` profile observed as aarch64 (for E14 only).

### 6.2 Effects table

Guest base for every guest path below: `B = /Volumes/Stash/scratch/effigy/linux-4f6a5ed3`,
on the guest's native root ext4 (E7). Every path below is created by this stage
and is refused if it already exists with content. Host paths use the same
spelling for the inputs directory and the GO receipt (§6.2a), and a separate
host evidence root, `/Volumes/Stash/scratch/effigy/linux-4f6a5ed3-evidence`.

**Execution order (authoritative; row labels are not order):**
E0 → E1 → E1b → E2 → E3 → E4 → E5 → E6 → E7 → E8 → E9 → E10 → E12 → E14 → E15 →
E13a → E13k → E13b → E16 → E17 → E19 → E18.

- E14 runs before any guest selector, because E15 supplies the launcher that E11 uses.
- E13b runs before E16, because the freeze includes the D old binary. It also
  waits for SO-5a. Without that ruling, Stage2 stops before E16.
- E16 freezes before E17 and E19: metadata names frozen copies, and the inputs
  and GO index carry frozen hashes.
- E11 is a rule that applies to E13a and E13k. It is not a step.

| # | Effect | Bound | Disposition |
| --- | --- | --- | --- |
| E0 | Guest: start `docker.service` only if inactive (`systemctl start docker.service`). Needed only to capture the baseline (E12) | 1 unit start; ≤ 60 s | Stays running until its owner disposition. Stop is a separate effect |
| E1 | Host: create `/Volumes/Stash/scratch/effigy/531-linux-control` mode `0700`, owned by `tom`. **Refuse if the path exists** (no adoption of historical control state) | 1 dir | Retained. Removal needs separate disposition |
| E1b | Host: create `/Volumes/Stash/scratch/effigy/linux-4f6a5ed3/inputs` and `/Volumes/Stash/scratch/effigy/linux-4f6a5ed3-evidence`, both mode `0700`, owned by `tom`. Refuse if either exists. `launch.py` creates each fresh evidence directory beneath the second one, and the directory must not exist yet | 2 dirs | Retained. Removal needs disposition |
| E2 | Host: generate one ephemeral Ed25519 key, mode `0600`, inside E1. No passphrase, because `IdentityAgent none` | 1 key, expiry ≤ 24 h | Deleted at Stage2 end, or retained under owner disposition |
| E3 | Guest: create account `effigy531`, password login locked, home `0700`, no sudo. **Refuse** if `effigy531` already exists as passwd entry, group, or home (Stage1 `effigy531_passwd_exit`/`_group_exit`/`_home`). No reuse, no key adoption, no deletion | 1 account | Retained. Removal needs separate disposition |
| E4 | Guest: add `effigy531` to `docker` only. Verify `id -nG effigy531` shows exactly `effigy531` and `docker` | **root-equivalent**; explicit GO | Retained. Removal needs separate disposition |
| E5 | Guest: sshd drop-in for `Match User effigy531` (`PasswordAuthentication no`, `AllowTcpForwarding no`, `X11Forwarding no`, `PermitTTY no`), then `systemctl reload ssh` after `sshd -t` passes | 1 file, 1 reload | Retained. Revert needs separate disposition |
| E6 | Guest: `authorized_keys` with `restrict,expiry-time=<≤24 h>` only if the Stage1 `sshd_man_expiry_time_mentions` count is non-zero. Otherwise refuse, with no weaker auth | 1 file | Expires. Removal needs separate disposition |
| E7 | Guest: create `/Volumes/Stash/scratch/effigy` (if absent) on the **native root ext4**, then `B` (mode `0700`, owner `effigy531`). No mount, no fstab edit, no share. Inside `B`, create `source`, `qual-root`, `driver`, `inputs/go`, `frozen`, `metadata`, `docker-config`, `toolchain`, `old-823d859d`, `docker-build-ctx`, all `0700` and owned by `effigy531`. `qual-root` and `docker-config` stay empty | ≤ 13 dirs | Retained |
| E8 | Guest: placement and filesystem controls: Unix sockets, descriptor transfer, locks, atomic no-clobber publication, fsync/readback under `B`. Helper per SO-2 | ≤ 60 s, ≤ 64 MiB writes | Faults retained |
| E9 | Host → guest: `git archive 4f6a5ed3078bc6e354c8a7738ea5663c8ffefa02`, pinned sha256, sent over the pinned SSH stdin per SO-1. Guest extracts into `B/source` and verifies a tree manifest against H | Archive ≤ 40 MiB (measured 31.6 MB, 2,140 tracked files); extracted ≤ 500 MiB | Retained as the frozen source root. Not a Queue checkout |
| E10 | Guest: Rust `1.97.1` (repo pin) with `rustup`, with `RUSTUP_HOME`/`CARGO_HOME` under `B/toolchain` | Download ≤ 1.5 GiB, installed ≤ 4 GiB, network allowlist (§6.3) | Retained |
| E11 | Guest: no Oxide install (cargo-routes forbids it). Every selector in E13a and E13k runs as `EFFIGY_OXIDE_ESCAPE=cargo <E15 effigy launcher> <selector>` from `B/source`. E13b is **not** covered by E11; its route is SO-5a. Recorded need: Stage1 `oxide` absent. A contrary Stage1 record stops before E9 | 0 bytes | Recorded in the Stage2 receipt per selector |
| E12 | Guest: runtime image. `docker pull ubuntu:22.04@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11`, then `docker build --pull=false -f B/source/docs/qualification/workspace-private-531/runtime.Dockerfile B/docker-build-ctx`, where `B/docker-build-ctx` is an empty directory (the Dockerfile has no `COPY`). Verify `/lib/ld-linux-aarch64.so.1`, `python3`, arm64. Record the image ID and the baseline inventory (`ps -a`, networks, volumes) | Pull ≤ 1 GiB; apt ≤ 300 MiB; engine disk ≤ 2 GiB | Image retained. Removal needs disposition |
| E13a | Guest builds at H, each through E11: `build:runtime-storage` (A), `build:workspace:private-qualification` (B, utility), `build:workspace:private-qualification:test-inputs` (test executables, listed only), `build:workspace:private-qualification:cargo-metadata` (metadata). Each stays inside its `tasks.toml` timeout (900 s) | ≤ 3,600 s total (4 × 900 s); target dir ≤ 25 GiB; cargo registry ≤ 4 GiB under `B/toolchain` | Frozen copies (E16) kept; `target` kept as evidence |
| E13b | Guest build of D at `823d859dace2cbaa848c8ad5a0b5bdfd8959fdf1` in `B/old-823d859d/source` (separate tree, never inside `B/source`). **Blocked until SO-5a is ruled.** The route must not use any binary built from H. Its selector text is read from that commit's own `config/tasks.toml` first | ≤ 1,800 s | Frozen copy kept; tree kept as evidence |
| E13k | Guest native test run: `test:workspace:private-qualification:kernel` through E11. It builds the helper (600 s) and runs nextest (900 s). **This is a native test execution with its own GO** (G11). Its test results are retained as evidence and are not proof cells | ≤ 1,500 s | Retained. A build-only route, if ruled, replaces it (SO-5b) |
| E14 | Host: Recipe E `EFFIGY_LINUX_RELEASE_TRIPLE=aarch64-unknown-linux-gnu effigy workspace:linux:artifact` at clean H, Colima `effigy-release`. Separate builder GO. Verify §4.1 predicates | ≤ 1,800 s; artifact ≤ 200 MiB | Producer receipt kept |
| E15 | Host → guest: copy E's artifact `effigy-aarch64-unknown-linux-gnu` with its `.receipt` and `.active-version` into `B/source/.effigy/linux-release/artifacts/`, byte-for-byte. Verify sha256 on both sides. The copy is also the guest `effigy` launcher for E11 (D-7) | ≤ 200 MiB | Retained |
| E16 | Freeze into `B/frozen/`, mode `0500`, hash each, write receipts: candidate, production, utility, kernel helper, kernel tests, custody tests, nextest, D old binary, E artifact, `gateway-nextest.toml` copy. Freeze before any later build can reuse its output | ≤ 1 GiB | Retained with manifest |
| E17 | Guest native metadata: `cargo-metadata.json` from E13a's metadata selector, and `binaries.json` derived per SO-6 into `B/metadata/`. Rewrite only the exact `binary-path` values to frozen copies | ≤ 60 s | Hash recorded |
| E18 | Stage2 teardown: stop and remove only what this stage created, by recorded ID or path. Keep frozen copies, receipts and evidence | ≤ 300 s | Per E-row disposition |
| E19 | Pre-position the reviewed driver (`common.py`, `runtime.py`, `probe.py`) into `B/driver`, the inputs file into `B/inputs/inputs.json`, the GO index into `B/inputs/go-index.json`, and the linux GO receipt into `B/inputs/go/linux.json` (path identical on host). Verify sha256 on both sides (SO-1) | ≤ 1 MiB | Retained |

### 6.2a Binding table — what the Linux cell names

Every Linux cell field resolves to this layout. Runtime and launch rules are
the ones named. Nothing is an approved default: each row is a Stage2 binding
that must match exactly.

| Cell key or remote key | Guest or host path | Owner and mode | Check at Stage2 and at run |
| --- | --- | --- | --- |
| `source_root` | `B/source` (guest) | `effigy531`, `0700` | Tree manifest equals H (SO-1). Contains `tests/config/gateway-nextest.toml`. It is an extracted archive, not a git checkout: the `source_head` check in `launch.py` runs on the host only |
| top-level `evidence` (host, `launch.py`) | `/Volumes/Stash/scratch/effigy/linux-4f6a5ed3-evidence/<UTC-attempt>` | host `tom`, `0700`, must not exist | Fresh directory under the E1b evidence root. Its parent resolves under SCRATCH. `launch.py` creates it. It is never reused |
| one-shot marker (guest, written by runtime `main`) | `/Volumes/Stash/scratch/effigy/531-<inputs-sha256>-linux.started.json` | guest `effigy531` | Created exclusively before the window. A failed or unknown attempt keeps it, so the same inputs cannot replay |
| `qualification_root` | `B/qual-root` (guest), created empty | `effigy531`, `0700`, owner equal to the SSH euid | Pin path, device, inode and uid after creation. Runtime refuses any entry in it (`os.listdir`) and any alias. Never reused |
| `remote.packet` | `B/driver` (guest) | `effigy531`, `0600`/`0400` files | `common.py`, `runtime.py`, `probe.py` sha256 equal host `PACKET` digests (launch verifies) |
| `remote.inputs` | `B/inputs/inputs.json` (guest) | `effigy531`, `0400` | sha256 equal host input file (launch verifies) |
| `remote.go_bindings` | `B/inputs/go-index.json` (guest) | `effigy531`, `0400` | sha256 equal host GO index (launch verifies). Its `linux` entry is `B/inputs/go/linux.json` |
| GO receipt | `B/inputs/go/linux.json` (guest) and the same path on host | `0400` | Identical bytes on both sides (G19). Runtime and launch both `artifact()` it |
| `guest_artifact` | `B/source/.effigy/linux-release/artifacts/effigy-aarch64-unknown-linux-gnu` | — | Under `source_root` (runtime check). Sha equal to E14 receipt |
| `guest_producer_receipt` | `…/effigy-aarch64-unknown-linux-gnu.receipt` | — | Sha pinned; unmodified (§4.1) |
| `guest_active_version` | `…/effigy-aarch64-unknown-linux-gnu.active-version` | — | Sha pinned; unmodified |
| `candidate`, `production`, `utility`, `kernel_fixture`, `kernel_tests`, `custody_tests`, `nextest_binary`, `old_binary` | `B/frozen/*` | `effigy531`, `0500` | Each sha256 pinned. `old_binary` ≠ `candidate` ≠ `production` (three distinct digests). `kernel_tests` and `custody_tests` must be named by `binaries_metadata` |
| `binaries_metadata` | `B/metadata/binaries.json` | `0400` | Derived per SO-6. Lists exactly the frozen test executables |
| `cargo_metadata` | `B/metadata/cargo-metadata.json` | `0400` | Generated natively from `B/source`. Workspace root is the guest path |
| `nextest_config` | `B/source/tests/config/gateway-nextest.toml` | — | Sha pinned |
| `engine_binary` | guest `command -v docker` target | root-owned system file | Sha pinned. Runtime `shutil.which('docker')` must match it |
| `engine_identity` | docker context name | — | Observed in Stage1 or Stage2; default context unless ruled otherwise |
| `docker_config` | `B/docker-config` | `effigy531`, `0700`, empty | Runtime sets `DOCKER_CONFIG` to it |
| `runtime_image` | `sha256:<image ID>` | — | Present by ID after E12 |
| `baseline` | `ps -a`, networks, volumes | — | Captured immediately before the first window. Runtime refuses drift |
| `old_source_head` | `823d859dace2cbaa848c8ad5a0b5bdfd8959fdf1` | — | Distinct from H |
| `budget_seconds` | ≤ 1,800 | — | Equal to the GO budget |
| `ssh_alias`, `ssh_config`, `ssh_known_hosts` | host, under E1 | host `tom` `0600` | Pinned per SO-1/common `ssh_prefix`. Host key read from Parallels and pinned, not scanned. Numeric `HostName` |

### 6.3 Capacity and time bounds (Stage2 total)

- Wall clock: the 22 listed step caps sum to **14,820 s**. Breakdown: E0–E8 with
  E1b = 540 s (E0 60, E1 60, E1b 60, E2 60, E3 60, E4 30, E5 60, E6 30, E7 60,
  E8 60); E9 900; E10 1,800; E12 1,800; E13a 3,600; E13b 1,800; E13k 1,500; E14
  1,800; E15 300; E16 300; E17 120; E18 300; E19 60. The Stage2 total cap is
  **18,000 s (5 h)**: 14,820 s of step caps plus 3,180 s reserve. Steps run one
  at a time.
- Host writes: ≤ 5 GiB (E9 archive, E14 artifact, evidence).
- Guest writes: ≤ 40 GiB total (E10 toolchain 4 GiB, E12 engine 2 GiB, E13a and
  E13b build trees 25 GiB, cargo registry 4 GiB, E9 source 0.5 GiB, E16 frozen
  1 GiB). Refuse above this, or when `df` shows less than 20% free.
- Network: static.rust-lang.org; index.crates.io and static.crates.io; Ubuntu archive
  or ports mirror; registry-1.docker.io for the one pinned digest; nothing else.
  Host side: the Colima builder's own pull, under E14 only.
- Virtual disk ceiling: 128 GiB (`131072Mb`). The current `.hds` is about 5 GiB.
  The guest free space and the post-Stage1 growth decide headroom.

### 6.4 Source-owner proposals (no code change in this task)

- **SO-1 Remote source, driver and GO placement.** `launch.py` already verifies
  remote hashes (G5). It does not place the files, verify the guest source tree
  against H, or verify the qualification root is empty before SSH. A reviewed
  helper would do the transfer (E9, E19), verify the tree manifest, and write
  the manifest. Closes G3 and the placement part of G5.
- **SO-2 Placement provenance and filesystem controls.** A checker that refuses
  unless the host provenance receipt (Parallels config hash, DiskDescriptor UID),
  guest correlation receipt, native mount source/target/fstype/device, and E8
  controls are all present and bound. Closes G2. Follows `linux-setup.md`.
- **SO-3 Remote timeout and orphan control.** The host budget must reach the
  guest. Options: a guest-side `timeout` with a recorded PID or PGID, or a
  guest-side kill route by that PID. Closes G4.
- **SO-4 Producer predicates.** In the rhai: ELF class and `e_machine` check;
  pinned toolchain identity recorded in the receipt; refuse the default x86_64
  triple for the Linux backend. Closes G6 and G7. This is a producer defect, so
  it goes to the source owner. It is not patched in artifact execution.
- **SO-5a Recipe D launcher (owner ruling, blocks E13b).** `cargo-routes.md`
  names the escape as `EFFIGY_OXIDE_ESCAPE=cargo effigy <selector>`. For D, the
  old tree has no built `effigy`, and H's binary is excluded. The proposed route
  is the old commit's own `scripts/oxide-selector.sh` with
  `EFFIGY_OXIDE_ESCAPE=cargo`, given the arguments of that commit's
  `build:runtime-storage` text, run from `B/old-823d859d/source`. That is not the
  `effigy <selector>` form `cargo-routes.md` names, so it needs a ruling before
  E13b. Until ruled, E13b does not run and Stage2 stops before E16.
- **SO-5b Kernel helper (owner ruling).** E13k is a native test run with its own
  GO. A build-only route for `kernel-custody-fixture` would replace it. Until then,
  E13k is what freezes the helper. Neither ruling installs Oxide.
- **SO-6 Native nextest metadata.** A generator for binaries metadata that
  checks the pinned nextest version's schema. It names only the frozen
  executables, and its source-root paths are the guest's. Closes G10.
- **SO-7 In-VM producer (only if ruled needed).** A route that builds the
  artifact inside the Parallels guest. No such route exists today (G8). Not
  needed if D-2 is accepted.

### 6.5 Network and identity effects are explicit

E0 starts a service. E3 and E4 create a root-equivalent path into the VM. E5
and E6 change sshd. E1 and E2 create host control state. Each needs an explicit
owner GO naming its account, group, service or config effect. A GO for E1 to E2
does not cover E3 to E6. No sudo rights, no other VM access. Removal of any of
these is separate disposition.

## 7. Stage3 — handoff preconditions (no execution)

Held and unchanged: 621 (Colima), 622 (Desktop), 623 (Linux full-session),
624 (aggregate). This kit does not arm any of them.

623 can proceed only when all of these are true:

1. Stage1 accepted, with its raw outputs retained.
2. Stage2 frozen cells recorded, with hashes for A, B, utility, kernel helper,
   test executables, nextest, the D old binary and the E artifact.
3. E's predicates (§4.1) pass, including the aarch64 ELF check.
4. Placement receipts (SO-2) and filesystem controls (E8) retained and reviewed.
5. Source and driver placement verified (SO-1). Remote hashes match the inputs,
   GO index and GO receipt on host and guest. The qualification root is empty
   and pinned.
6. Linux GO receipt covering this exact source head, inputs hash, budget and
   engine identity. The decision ID is the operator's. Its one-use marker is
   unclaimed.
7. The runtime image (E12) is present by ID, with a baseline inventory pinned
   immediately before the window.
8. SO-3 timeout control resolved, or an explicit owner-accepted manual control.

624 follows 623 and reads its original receipts. Nothing here grants production
or consumer credit.

**Next action in the original kit revision:** after the author and retained
reviewer find this plan ready, Stage1 activation is reviewed against §5. The
Nov. 7 prospective authority note below supersedes the fresh generic operator
GO request: planner-reviewed exact-stage activation is required, and #684 owns
the actual Stage1 execution. Stage1 remains exactly one start of UUID
`433c9e5b-c7b9-469f-9633-24b4addcd3b5`, one read-only probe, one graceful stop,
600 s, no snapshot, no `--kill`, no SSH.

## 8. Retained copy, manifest and kit hash

- Retained kit copy and manifest: a new 0700 subdirectory per revision below
  `/Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam/`. Each holds
  the K document bytes, the extracted probe script, the Stage0 raw captures and
  a manifest with each sha256. Original planner files stay untouched, and earlier
  revision directories stay as history.
- Top-level evidence output: host `/Volumes/Stash/scratch/effigy/linux-4f6a5ed3-evidence/<UTC-attempt>`
  (fresh, §6.2a). The retained copy of each attempt's receipts goes beside it under
  the same root, never into `l0tx7tam`.
- K document sha256: recorded in the PR body and the manifest. It is computed
  from the committed file.
- Probe script sha256: recorded in the manifest, computed from the extracted
  §5.6 block.

## 9. Remaining unknowns and decisions

Unknowns, carried to Stage1 or Stage2 and not guessed here: current IP and host
key; guest disk-correlation identifier; guest free space; `oxide` absence and
the sanctioned escape's recorded need; Colima `effigy-release` arch; installed
sshd `expiry-time` support; whether `docker.service` is active; runtime image
presence; docker context names; the nextest schema at its pinned version;
toolchain identity inside the builder; whether `effigy531` already exists; whether
`harddisk1.hdd` (zero-byte) is referenced.

Decisions taken in this kit (reviewer checks these):

- **D-1** Stage1 takes no pre-start snapshot. Return is a graceful stop of the same VM. The disk diverges and the divergence is recorded.
- **D-2** Recipe E is built by the existing Colima aarch64 producer on the host, not in the guest. The target is explicit `aarch64-unknown-linux-gnu`.
- **D-3** Recipes A to D for Linux are built natively in the guest from the frozen source archive, never from Darwin binaries.
- **D-4** Stage1 uses one `prlctl exec` read-only probe script, root context required, no password. Docker daemon reads run only when the daemon is already active.
- **D-5** Stage2 SSH uses a passphrase-less ephemeral Ed25519 key with expiry ≤ 24 h and restrictions. If expiry is unsupported, refuse.
- **D-6** No `--kill` in any stage without its own GO.
- **D-7** The guest runs selectors with the sanctioned per-selector escape from `cargo-routes.md` (`EFFIGY_OXIDE_ESCAPE=cargo <selector>`). It does not install Oxide and types no raw `cargo`. The launcher is the Recipe E artifact (H source, receipt-verified), so no build runs before the first selector. The launcher covers the H tree only (E13a, E13k). It does not apply to D (SO-5a).
- **D-8** The kernel test run (E13k) is named as a native test effect with its own GO. It is not folded into a build step.

## 10. Prospective authority note — 2026-11-07

Tom's exact standing instruction is: “Go for it. Do what you need to do to
work through the tasks on the board without needing my go on every step, you
have approval to get it done.” It is recorded in answered operator ruling
`63c338e2-1c50-435e-9783-e78e9d8558e6` v2 and answered concrete Stage1 decision
`6715adf6-dc88-41ff-9cfa-cc483888358d` v2. This prospective note supplements
the earlier kit authority and next-action language; it does not rewrite that
historical text or authorize any execution by this preparation document.

Within the existing approved board product and threat scope, a stage may be
activated after planner review of its exact stage under the standing authority;
a fresh generic operator GO for every step is not required. Stage1 actual
execution remains owned by #684. This note performs no observation, start,
stop, replay or other Stage1 action.

Each native window still requires fresh GO receipt/data bound to the actual
decision, source head, program and inputs, resource identities, budget, start
and end bounds, one-use marker and cleanup disposition. All existing
preconditions, refusal rules, exact argv and probe bytes, numeric bounds,
qualification assertions and independent review remain unchanged. No automatic
retry or refunded allowance is implied; an actual failure or unknown must be
dispositioned before another occurrence. This approval does not permit forced
stop, snapshot restore, destructive cleanup, data loss, use of unrelated
resources, material decisions outside the existing board product/threat scope,
frontier/by-request profiles, or real release, tag or publication.
