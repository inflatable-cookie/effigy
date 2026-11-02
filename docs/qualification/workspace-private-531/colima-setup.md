# Dedicated Colima setup request — unarmed

This proposal needs a later scoped operator GO. It grants no runtime authority.
Do not restart/reconfigure `effigy`, its Acow resources or `effigy-release`.

| Item | Proposed bound |
| --- | --- |
| Profile / input `engine_identity` | `effigy-531-qualification` |
| Shared root | `/Volumes/Builds/scratch/effigy/531-colima-qualification` |
| Unshared control root | `/Volumes/Builds/scratch/effigy/531-colima-control` |
| Input `colima_home` | Control root + `/colima` |
| VM | aarch64, VZ, containerd; 2 CPUs, 2 GiB RAM, 20 GiB runtime + 20 GiB root disk |
| Setup window requested | At most 1800 seconds within existing task budget; not granted here |
| Setup workloads | One private image build and one disposable tools check |
| Proof workloads | One primary at a time; no ports/named volumes/shared services; original packet limits |

## Ownership and setup effects

A later setup run must refuse existing roots or profile without their exact
matching owner receipt: no adoption, repair, deletion or recreation. Exclusively
create canonical, executing-user-owned `0700` directories with no symlink
ancestors. Leave the shared root empty; record its actual device/inode/uid in
`qualification_root` before input/GO hashing. Keep owner receipts, credentials,
VM state, caches and build context under the **unshared** control root.

Precreate control `/colima`, `/cache` and `/config` before selecting them:
Colima 0.10.3 ignores a nonexistent `COLIMA_HOME`. Do not inherit `LIMA_HOME`; Colima then uses
`COLIMA_HOME/_lima`. See its pinned [directory implementation](https://github.com/abiosoft/colima/blob/v0.10.3/config/files.go).
After a setup-only GO, with the approved external `TMPDIR`:

```sh
env -u LIMA_HOME \
COLIMA_HOME=/Volumes/Builds/scratch/effigy/531-colima-control/colima \
COLIMA_CACHE_HOME=/Volumes/Builds/scratch/effigy/531-colima-control/cache \
XDG_CONFIG_HOME=/Volumes/Builds/scratch/effigy/531-colima-control/config \
colima start -p effigy-531-qualification --runtime containerd --arch aarch64 \
  --vm-type vz --cpus 2 --memory 2 --disk 20 --root-disk 20 \
  --mount /Volumes/Builds/scratch/effigy/531-colima-qualification:w \
  --activate=false --ssh-agent=false --ssh-config=false
```

Verify generated mounts contain only that exact shared root: no home, control,
parent scratch or sibling mount. Freeze executable hashes, actual VM/engine
identity, versions and complete container/network/volume inventories. Unknown
identity, extra mounts or unexpected effects refuse further work; they grant
no permission to fix an existing runtime. Inspect only this private profile.

## Image, downloads and disposition

Recipe SHA-256: `d290f70febe8ce9ba4cd3cc252a9786d8ee5fc4e82a6219ec0338c884486f514`. This pins source text, not a built image.

Build the reviewed [runtime recipe](runtime.Dockerfile) from a fresh control
context on this profile with `--platform linux/arm64`. It installs Python 3,
GNU core utilities and the actual GNU loader. The maintained producer image
does not install Python and is **not** a qualified runtime image. Record actual
immutable image ID/digest and perform only the GO-covered disposable tools
check. Recipe text and tags are not receipts; no image is verified here.

The GO must cover Colima/Lima's official VM/runtime downloads, the recipe's
digest-pinned Ubuntu base and signed Ubuntu apt packages/dependencies for
`python3`, `coreutils`, `libc6`, `ca-certificates`. Record actual tool, base and
package identities. No pip, private registry credential reads, unrelated
installs or image export/import from other profiles. Preserve existing phase
deadlines, the smaller total GO bound, and the single aggregate proof allowance.

Retain exact profile/image/root/resource metadata after setup. Stop/delete,
prune, cache/root removal need a separate exact disposition; retention proves
neither cleanup nor capacity release. Setup does not execute qualification.

Freeze host/guest candidates after the owner-selected production repair lands.
Earlier e51 builds remain historical; never relabel them as this packet delta
or a later commit. All three backend GOs, frozen input/GO-index hashes, supported
aggregate execution and retained review remain required.

Desktop startup/availability is separate. Linux needs the owner's selected
Vault VM, confirmed storage/guest health, exact SSH alias, native scratch and
separately approved source/build placement. Each backend needs an empty pinned
root; host metadata or a Colima/macOS result cannot substitute for native Linux.
