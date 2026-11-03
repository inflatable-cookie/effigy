# Retained Colima runtime-image preparation — unarmed

This is source preparation, not permission to execute. The planner must bind a
fresh continuation after retained review. Tom's `c9e8c94b` v2 already covers the
**unspent one image build and one disposable check** on the exact retained VM;
choosing the bounded context method below does not ask for a new product outcome
or another start. No new operator question is needed for that routine choice.

## Actual history and remaining bounds

The first start at `8c1a2d15` failed: its Lima socket path was120 UTF-8 bytes,
not less than104. No build/check ran. The corrected `531c/c` layout passed the
[read-only preflight](setup_preflight.py) at101 bytes and its one approved start
at `8092fb609` exited0 in77.12seconds. VM/mount/engine observations were recorded,
not qualification. The reviewed tar-stdin context plan then refused without a
build: installed nerdctl2.3.1 [rejects that context form](https://github.com/containerd/nerdctl/blob/v2.3.1/cmd/nerdctl/builder/builder_build.go).
That impossible command is replaced here; do not retry it or switch builders.

Preparation consumed283.44seconds of the original1800, then457.74 of1516.
The next cap is **1058seconds**, rounded down from1516−457.74, also within the
remaining task budget. Include admission, transfer, build/check, postchecks and
evidence/report work; preserve smaller deadlines/cancellation. No refund,
reset, extension, extra start, install or cleanup allowance follows.

| Retained item | Required identity |
| --- | --- |
| Profile | `effigy-531-qualification` |
| Control / Colima home | `/Volumes/Builds/scratch/effigy/531c` / suffix `/c` |
| Cache / config / private registry config | Control + `/cache`, `/config`, `/docker-config` |
| Sole shared root, still empty | `/Volumes/Builds/scratch/effigy/531-colima-qualification` |
| VM | aarch64 VZ/containerd,2CPUs/2GiB,20GiB runtime+20GiB root |
| Engine / machine | `9ea86682-b930-4da6-9901-73681bd7ba85` / `52aa90d932e84f0382bd2bbde03ba72a` |
| Recorded share | `lima-4661d5013c960385`, virtiofs,rw; no other host share |
| Runtime recipe SHA256 | `d290f70febe8ce9ba4cd3cc252a9786d8ee5fc4e82a6219ec0338c884486f514` |

**These are historical pins, not current health.** Planner DiskArbitration
metadata records Builds disappearing/remounting at23:48:00.680–03.636 and
23:48:18.343–24.998 on2026-11-02. Why it disconnected and the retained VM's old
image-handle health are unknown. Before any later image effect, freshly observe
actual authenticated running state, engine/machine, resources, root identities,
mounts and private storage. Unreachable/changed/error/unknown state refuses;
no start/reset/repair or adoption. Successful metadata reads alone do not prove
intact disk contents. Guest context fsync/readback must also succeed.

## Same builder, public recipe only

[image_context.py](image_context.py) and [image_prepare.py](image_prepare.py) are
narrow source-reviewed helpers, not a CLI toggle, installer or proof route.
Colima0.10.3's maintained [SSH command](https://github.com/abiosoft/colima/blob/v0.10.3/cmd/ssh.go)
uses [Lima shell](https://github.com/abiosoft/colima/blob/v0.10.3/environment/vm/lima/shell.go)
through its interactive stdio interface. Containerd operations use the same
maintained `sudo nerdctl` privilege; no account or privilege changes are added.

1. **Read-only prerequisite.** On the exact isolated profile, run the helper's
   `probe_command` under guest `timeout` and a larger bounded host client. It
   requires actual Python3/standard library and recorded `findmnt` availability;
   missing tools refuse before transfer, with no installation. Observe the
   executing user's canonical private home, actual uid/device/inode and native
   ext4 backing, machine/architecture and all host shares. A guest parent path
   has not been observed or selected yet: do not fill it from a guessed username.
2. **Freeze actual inputs.** Bind the reviewed source/helper/recipe and Colima
   executable hashes, actual Queue decision,1058-second cap/absolute deadline,
   original owner receipts and current control/shared root pins; actual private
   guest-parent binding; fresh32-hex token; exact engine/machine/share/resource
   pins. Seal input hash and argv/environment plan in unshared control. Use only
   isolated `COLIMA_HOME`, cache/config/private registry paths; unset `LIMA_HOME`
   and filter ambient credentials. Final native inputs/GO-index are not armed.
3. **Transfer.** Claim `runtime-image.started.json` exclusively in the verified
   control root before effects. Feed only the exact bounded recipe bytes as stdin
   to `colima ssh -p effigy-531-qualification -- timeout -k 2 20 python3 -I -B -c <reviewed-helper> stage <bound-spec>`.
   No SCP, tar, hash utility or shared control mount is assumed. Guest helper
   rejects missing/wrong bytes before creation, exclusively creates0700
   `effigy-531-image-<token>` below the pinned private native parent and writes
   **only** `Dockerfile` using O_EXCL/O_NOFOLLOW/0600. It fsyncs file/directories,
   reads back SHA256/ownership/identity and retains every partial failure.
4. **One local-path build.** Reobserve runtime/root/share identity and verify the
   exact context/file descriptor identities/hash. Use that directory, never `-`,
   with the same installed `sudo -n nerdctl build --platform linux/arm64
   --provenance=mode=max --tag effigy-531-runtime:r1 <actual-private-guest-path>`.
   No host control path is passed as context. Explicit `DOCKER_CONFIG` selects
   an uncreated child of this exclusively owned context, so private registry
   configuration is not consulted; no login/secret/SSH build option is used.
   Both the actual guest command and supervised host client have bounded
   deadlines, with reserved evidence time. A failed/Unknown transfer never
   reaches build; failed/Unknown build never reaches the check. No retry.
5. **One disposable check.** Inspect the actual built linux/arm64 image ID and
   provenance, freshly reobserve identity and verify unchanged context. Claim
   build/check stages exclusively. Run only that immutable image with
   `--pull=never --rm --network none`, a unique recorded name and inner timeout.
   Check root uid, Python/SSL, actual GNU loader, GNU coreutils and package
   versions. This is the already approved disposable check, not qualification.

The caller of `prepare` must be the fresh Queue-bound approved executor and
provide `call(argv, env, timeout, data)` supervision that records actual PIDs,
start/end/exits, uses EOF unless recipe data is supplied, waits clients and
retains uncertainty. The helper never grants authority from a JSON file. It
holds/rechecks exact host-root descriptors, observes actual runtime identity at
stage boundaries, enforces the remaining cap and consumes a durable one-shot
claim; replay cannot build/check twice. Tests double external engine I/O only;
public context creation/hash/ownership/readback uses real fixture filesystem I/O.

## Receipts, downloads and retention

Keep exact input/command/PID/exit/deadline logs, fresh admission, source/tool
hashes, parent/context/file bindings, transfer/readback receipt, actual image
manifest/config/platform/base materials from BuildKit provenance and package/tools
receipts in unshared control and persistent evidence. The helper's tools digest
is not a complete image receipt: missing provenance/package/output observations
remain missing. Embed small results/hashes in PR686, including all failures.
Never convert a tag, recipe, VM image or partial receipt into a qualified image.

Downloads remain only official Colima/Lima runtime and the recipe's pinned
Ubuntu22.04 base plus signed apt python3/coreutils/libc6/ca-certificates dependencies.
No pip, unrelated packages, private registry reads, extra image build/check,
alternate builder, cross-profile import or new mount. Retain old/new controls,
VM/profile/cache/disks, context and image; no automatic deletion/prune/repair or
capacity credit. Protected effigy/Acow/effigy-release, Desktop/Linux, candidate
builds/transfer, consumer/install/proof/production/merge effects remain excluded.

Final candidate freeze waits #574 and #580 source integration. Historical e51
and8092 bytes are not final candidates. Full aggregate inputs/GO-index remain
**UNARMED**, production Discard refused and merge held.
