# Colima workspace-sessions qualification packet — candidate a00394012, r1

Artifacts manifest SHA-256: `3e03dcddc221ae007138465b0a248041580fe35f5f4d57ffa474621ac32a13ee`

Subject: the merged workspace-session cut at commit
`a00394012ae8dafe52981c36d459ec0491ec0552`, executed against the Colima
profile `effigy` (colima 0.10.3, containerd runtime, nerdctl v2.3.1). The
read-only entry sweep for this backend is complete (2026-11-02, digests in
`../README.md`); execution of this packet remains unauthorized until its GO
chain is satisfied. Blocked-by-cut prerequisites, the exercisable subset, the
proof route and the protected-state rules are stated in `../README.md` and
apply to this packet verbatim.

## Exact synthetic effects this packet may create under its GO

- one disposable build container `effigy530-build-5jwq2c` from the resident
  `rust:1.97.1-slim-bookworm` image (ID
  `sha256:2775a09d208ff0d7c1f50490c45b62db929e87ba1dcbc3f2132ac71a704bcdd3`),
  bind-mounting `/Users/tom/.effigy-scratch/530-qual-20261102-colima/build-src`
  at `/work`, with bridge networking for the Cargo.lock-pinned crate fetch,
  removed by exact ID before the preparation receipt is written;
- two identity containers `effigy530-ident-5jwq2ca` and
  `effigy530-ident-5jwq2cb` from the resident `alpine:3.19` image (ID
  `sha256:6baf43584bcb78f2e5847d1de515f23499913ac9f12bdf834811a3145eb11ca1`),
  `--net none`, `--pull=never`, each carrying the staged guest binary, removed
  by exact ID in the runtime cleanup;
- the fixture checkout `/Users/tom/.effigy-scratch/530-qual-20261102-colima`
  with exactly the three manifest-pinned files, removed by exact manifest.

No pull, VM start/stop, stack, install, shared-volume or consumer action is
authorized by this packet. Both required images are resident today (verified
read-only 2026-11-02); if either is missing at window, the packet stops and the
pull is named in a fresh GO request, never performed unasked.

## Cells

1. **C1 refusal (read-only-safe, run in preflight and runtime)** —
   `container up --system dev` refuses with the exact text "workspace private
   preparation output is unavailable; no preparation or workspace apply was
   attempted"; the `--attach` variant refuses "workspace activation requires
   detached persistent ownership; held/attached handoff is unavailable";
   fixture byte-identical; engine inventory identical before and after.
2. **C2 identity** — the frozen candidate's Linux aarch64 binary, built under
   GO inside the disposable build container, executes `__guest-identity`
   inside both identity containers. Each reply must carry schema
   `effigy.guest-probe.v1`, the exact engine container ID, a nonzero UUID boot
   ID, and `sha256:` of the staged binary; repeated calls must agree; the two
   containers must observe distinct container IDs and their boot-ID relation
   (shared VM) is recorded.
3. **C3 resolve refusal** — `__guest-reach --resolve` inside the identity
   container refuses by name (`workspace host address unknown`, or the
   identity refusal that precedes it on a backend whose mountinfo cannot
   prove placement): no host resolution without published selection evidence.
   That recorded outcome is the backend's mount-root qualification result,
   judged by the retained reviewer.
4. **C4 baseline and cleanup** — 15/36/255 inventory equality against the
   pinned baselines at every step boundary; synthetic containers removed by
   exact ID; fixture removed by exact manifest; host-side negative probe
   (`__guest-identity` on the macOS host refuses).

## Blocked prerequisites on this candidate (not native credit)

Positive session publication, retained owner and custody, pinned TLS
credential issue, `__guest-reach --dial` against a live listener, the
integrated Level 1 streaming cells, workspace-session down/fresh-up, guest
attach and consumer verification are refused or unreachable on this cut and
are recorded, not credited. See `../README.md`.

## GO provenance and receipts

Complete read-only sweep (done), Supervisor GO naming this packet's SHA copied
verbatim to `/Volumes/Builds/scratch/effigy/530/530-colima-r1-go.txt` (0600),
the Acow owner's (single planner `cb0e6df4`) confirmed exclusive quiet window,
and Tom's explicit live-environment GO. One execution; receipts and logs are
0600 O_EXCL files under `/Volumes/Builds/scratch/effigy/530/`. No retry after
any failure.
