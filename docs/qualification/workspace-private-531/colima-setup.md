# Dedicated Colima setup request — unarmed

A separate operator GO and fresh owner-bound run are required. This source-only
correction grants no start, creation, download or build authority, even if the
pending board decision is answered during its preparation. Do not change
`effigy`, Acow, `effigy-release`, Desktop or the Linux VM.

## Failed first attempt and corrected layout

The approved first attempt at packet head `8c1a2d15` failed once: Colima 0.10.3 /
Lima 2.2.1 rejected the generated socket below `531-colima-control/colima`.
Its full path was **120 UTF-8 bytes**; Lima requires **less than 104**. Start
exited 1; runtime image builds and tools checks were both zero. No qualification
ran. Preserve that failed receipt and the exact old roots, cache and datadisk.
Do not move, alias, delete, adopt or repair them; no retry is implied.

The retained proposal `short-layout-proposal.json` SHA-256
`35816e83eced9e89ac90068259658d3df4786837666ddb5f2e130c539f3a20c4`
at `/Volumes/Builds/scratch/effigy/531-colima-setup.3hzi8ybw/` changes only the
future canonical control layout. It predicts a **101-byte** socket, preserving
the profile and shared-root identity. This prediction is not a successful start.

| Item | Proposed bound |
| --- | --- |
| Profile / input `engine_identity` | `effigy-531-qualification` |
| Existing shared root | `/Volumes/Builds/scratch/effigy/531-colima-qualification` |
| New unshared control root | `/Volumes/Builds/scratch/effigy/531c` |
| Input `colima_home` | `/Volumes/Builds/scratch/effigy/531c/c` |
| Cache / config / private registry config | Control root + `/cache`, `/config`, `/docker-config` |
| VM | aarch64, VZ, containerd; 2 CPUs, 2 GiB RAM, 20 GiB runtime + 20 GiB root disk |
| Remaining setup cap requested | **1516 seconds**, within existing task budget and smaller phase deadlines |
| Setup workloads | One separately approved normal start, one private image build, one disposable tools check |
| Proof workloads | None during setup; the later aggregate retains its original limits |

The first preparation consumed 283.44 of the original 1800 seconds. The proposed
1516-second cap rounds the remainder down: no refund, reset, extension or fresh
1800-second allowance. Measure the new total from the first preparation step,
including admission, and retain the original cancellation and smaller deadlines.

## Admission before any setup effect

Run the read-only [socket preflight](setup_preflight.py) **before any directory
creation, profile command, download or start**. It computes the full future
`COLIMA_HOME/_lima/colima-<profile>/ssh.sock.1234567890123456` path, counts UTF-8
bytes, and refuses 104 or more. Canonical absolute paths strictly below approved
scratch are required; symlink ancestors, lexical aliases and a changed profile
refuse. It creates nothing and grants no execution authority.

```sh
python3 -I -B docs/qualification/workspace-private-531/setup_preflight.py \
  --colima-home /Volumes/Builds/scratch/effigy/531c/c \
  --profile effigy-531-qualification
```

Pin the actual Colima/Lima executable hashes and versions before relying on this
observed naming rule. A different naming rule/tool version needs a fresh path
calculation and reviewed plan, not a shorter alias or disabled length guard.

After the separate GO, require the new control root/profile to be absent; refuse
foreign or unreceipted existing identities. Exclusively create canonical,
executing-user-owned `0700` directories without symlink ancestors. The existing
shared root is reusable **only** with the original matching owner receipt and
fresh exact path/device/inode/uid, `0700`, empty-directory revalidation. An
unknown/replaced/nonempty root is retained/refused, never recreated or rebound.
Leave it empty for later native input/GO hashing. Keep all owner receipts, keys,
VM state, cache and context under the new **unshared** control root.

Precreate `/c`, `/cache`, `/config`, `/docker-config` and `/context` there before
selecting them: Colima 0.10.3 ignores nonexistent `COLIMA_HOME`. Unset inherited
`LIMA_HOME`; Colima uses `COLIMA_HOME/_lima` (its pinned
[directory implementation](https://github.com/abiosoft/colima/blob/v0.10.3/config/files.go)).
After all admission and GO checks, re-run the preflight as the start's immediate
gate; do not run the second command if it fails:

```sh
python3 -I -B docs/qualification/workspace-private-531/setup_preflight.py \
  --colima-home /Volumes/Builds/scratch/effigy/531c/c \
  --profile effigy-531-qualification &&
env -u LIMA_HOME \
COLIMA_HOME=/Volumes/Builds/scratch/effigy/531c/c \
COLIMA_CACHE_HOME=/Volumes/Builds/scratch/effigy/531c/cache \
XDG_CONFIG_HOME=/Volumes/Builds/scratch/effigy/531c/config \
DOCKER_CONFIG=/Volumes/Builds/scratch/effigy/531c/docker-config \
colima start -p effigy-531-qualification --runtime containerd --arch aarch64 \
  --vm-type vz --cpus 2 --memory 2 --disk 20 --root-disk 20 \
  --mount /Volumes/Builds/scratch/effigy/531-colima-qualification:w \
  --activate=false --ssh-agent=false --ssh-config=false
```

A bounded executor must enforce the remaining 1516-second total and each smaller
phase bound around this command; a shell example is not supervision. Wait every
exact client/PID. A failed or unknown start retains everything with **no second
start, repair or cleanup** under that GO. Before continuing, positively verify
actual VM/engine identity and generated/actual mounts: only the exact shared root,
no home/control/parent/siblings. Freeze complete engine inventories; empty
listing output alone is not absence or verified closure.

## Immutable image and receipt plan

Freeze a private preparation input outside the shared root: reviewed source/tree
and setup/preflight/recipe hashes, actual operator decision/version/expiry,
1516-second cap, exact command argv/environment selection, tool hashes/versions,
new-root identities and original shared-root owner receipt/hash/pins. Record
preflight JSON before effects. No native GO file or final runtime input is made
by this preparation. Recheck source/recipe bytes and roots after completion.

Recipe SHA-256: `d290f70febe8ce9ba4cd3cc252a9786d8ee5fc4e82a6219ec0338c884486f514`.
Copy only the reviewed [recipe](runtime.Dockerfile), byte-for-byte, into private
`/context/Dockerfile`; this hash is source text, not a built-image identity.
The GO must cover official Colima/Lima VM/runtime downloads and the pinned
Ubuntu 22.04 base plus signed apt `python3`, `coreutils`, `libc6`,
`ca-certificates` dependencies. No pip, private registry/credential reads,
unrelated packages or cross-profile image import. Use the empty private registry
config, not ambient credentials.

Use the same isolated environment/profile for every subsequent client. Verify
its documented stdin-build interface first; if unavailable, retain/refuse rather
than mount control or introduce another build route. The single build plan is
`tar -C /Volumes/Builds/scratch/effigy/531c/context -cf - Dockerfile` streamed to
`colima -p effigy-531-qualification nerdctl -- build --platform linux/arm64 --tag effigy-531-runtime:r1 -`.
Bound and wait both clients and record both exits. No control-directory share is
needed. Resolve the result to its immutable image/config/platform/base digests;
a tag or recipe alone is insufficient.

Then one disposable `nerdctl run --rm --network none <actual-image-ID>` tools
check, on that same isolated profile, records Python/SSL, root uid, actual GNU
loader, GNU `stat`/`printenv`, and installed package versions via `dpkg-query`.
Use only the resolved image ID, no pulls or fallback. The producer image lacks
Python and is not a runtime substitute. This check supplies setup evidence only.

Persist command/start/end/exit/PID records, root/tool/source/recipe hashes, actual
mount/VM/engine identities, inventories, downloads, package/image receipts and
failure history outside ephemeral Queue cleanup; embed small results/hashes in
PR686. Retain exact profile/image/roots/resources. Stop/delete/prune/cache removal
needs a separate exact disposition; no teardown or capacity release is inferred.

Final host/guest candidates freeze after concrete #574 source stabilization;
earlier e51 producer/build receipts remain historical. All three backend inputs,
separate native GOs, planner-bound aggregate execution, retained receipt review,
consumer verification and final production-enable GO remain required. Inputs and
GO-index stay **UNARMED**; production Discard stays refused and merge held.
Linux transport/placement is a separate [unarmed proposal](linux-setup.md).
