# Docker Desktop workspace-sessions qualification packet — candidate a00394012, r1

Artifacts manifest SHA-256: `3a25ffad93508b175df1d46803ce988e01699c3836ef14478a3e7d219f4187f7`

Subject: the merged workspace-session cut at commit
`a00394012ae8dafe52981c36d459ec0491ec0552`, executed against Docker Desktop
(context `desktop-linux`, docker CLI `/usr/local/bin/docker`). Docker Desktop
must already be running inside the window: starting it is a separately scoped
approval, never a packet effect. The read-only entry sweep for this backend is
environment-gated — Docker Desktop is down outside windows, so the baseline
and resident-image predicates are captured and pinned by prepare inside the
window and compared at every later step boundary. Blocked-by-cut
prerequisites, the exercisable subset, the proof route and the protected-state
rules are stated in `../README.md` and apply to this packet verbatim.

## Exact synthetic effects this packet may create under its GO

- one disposable build container `effigy530-build-5jwq2d` from a toolchain
  image resident in the Desktop store (prepare pins the exact reference and
  ID; the default candidate is `rust:1.97.1-slim-bookworm`), bind-mounting
  `/Users/tom/.effigy-scratch/530-qual-20261102-desktop/build-src` at `/work`,
  with bridge networking for the Cargo.lock-pinned crate fetch, removed by
  exact ID before the preparation receipt is written;
- two identity containers `effigy530-ident-5jwq2da` and
  `effigy530-ident-5jwq2db` from a resident runtime image (default candidate
  `alpine:3.19`), `--net none`, `--pull never`, each carrying the staged
  guest binary, removed by exact ID in the runtime cleanup;
- the fixture checkout `/Users/tom/.effigy-scratch/530-qual-20261102-desktop`
  with exactly the three manifest-pinned files, removed by exact manifest.

If a required image is not resident, the packet stops and the pull is named in
a fresh GO request — never performed unasked. No VM start/stop, stack,
install, shared-resource or consumer action is authorized by this packet.

## Cells

Identical in kind to the Colima packet (`../colima/packet-r1.md`): C1 refusal
with exact texts and zero effects; C2 identity with schema, exact engine
container ID, UUID boot ID, staged-binary digest, stability and distinctness;
C3 resolve refusal by name; C4 within-window baseline equality and exact-ID /
exact-manifest cleanup. The fixture-ancestor mountpoint check covers host
mounts only (Docker Desktop has no documented host-to-VM SSH route); the
boundary is disclosed here and in the receipt.

## Blocked prerequisites on this candidate (not native credit)

Identical to the Colima packet: publication, custody, TLS credential, dial,
streaming cells, down/fresh-up, attach and consumer verification remain
blocked by the production cut on this candidate.

## GO provenance and receipts

Complete read-only sweep of what is observable before the window (the host
side: candidate identity, fixture refusal cells with engines down, host
negative probes — recorded 2026-11-02), then Supervisor GO naming this
packet's SHA copied verbatim to
`/Volumes/Builds/scratch/effigy/530/530-desktop-r1-go.txt` (0600), Docker
Desktop already running, the Acow owner's confirmed exclusive quiet window,
and Tom's explicit live-environment GO. One execution; receipts and logs are
0600 O_EXCL files under `/Volumes/Builds/scratch/effigy/530/`. No retry after
any failure.
