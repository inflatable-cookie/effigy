# Merged workspace sessions on Colima, Docker Desktop and Linux Docker — qualification evidence (candidate a00394012, r1)

Evidence packet for the deferred native full workspace-session qualification of
the merged #512/#528/#529 cut, prepared as Queue task effigy#530. This
directory is the source-immutable proof head: each backend packet is executed
exactly once through its committed helpers against the separately frozen
candidate — not the newest main and not a rebuild.

Status: **prepared; read-only entry sweep complete for Colima; not executed.**
No backend packet has run under GO. No native proof exists yet. The positive
full-session gates are blocked by the candidate's own production cut, recorded
below; nothing here substitutes a fixture for them.

## Frozen subject

- Candidate source commit
  `a00394012ae8dafe52981c36d459ec0491ec0552` (merge of PR #662; the merged
  workspace-session cut of #512 L1 policy, #528 Level 1 non-interactive
  streaming, #529 helper hardening), clean detached scratch checkout
  `/Volumes/Builds/scratch/effigy/530/candidate-src-a00394012`.
- Host (macOS arm64) candidate binary
  `candidate-src-a00394012/target/debug/effigy`, SHA-256
  `58092a16bdab2aa15371ab352c0ab6f60f1baccbda9cd7b9b5e16f021054efb1`, built
  with plain Cargo in the detached scratch checkout (qualification-candidate
  build route; no repository selector runs it). The binary is not committed
  and must not be installed.
- Linux aarch64 guest binary: built inside the window, never before it — in a
  disposable container on the backend from the resident
  `rust:1.97.1-slim-bookworm` toolchain image (Colima; no pull), or natively in
  the Linux VM with plain Cargo per the Vault-backed Linux proof build rules in
  `docs/knowledge/cargo-routes.md`. Its SHA-256 is recorded at prepare and
  pinned into the invocation receipt before any probe runs.

## Blocked-by-cut prerequisites (recorded; gates stay blocked)

The candidate refuses workspace-mode activation by name before any preparation
or apply: `require_private_preparation_output()`
(`src/runner/workspace_client/container_up.rs`) refuses every
`container up --system` invocation, `environment::workspace::up_prepared` has
only test call sites, and #534's production kernel custody handoff, durable
attempt-group binding and recovery sweep are not merged. Until #531/#533 wire
the production private preparation output and #534 lands custody, the
following gates **cannot be exercised natively on this candidate** and are
recorded in every packet as blocked prerequisites, not native credit:

- positive workspace session publication (`up_prepared`);
- the retained session owner, owner-loss custody and settlement;
- pinned TLS credential issue and any `__guest-reach --dial` against a live
  host listener (no credential is minted without publication);
- the integrated Level 1 streaming cells (#528: no PTY, stdin EOF, distinct
  incremental stdout/stderr, exact real nonzero/signal outcomes, owner
  cancellation, fail-closed gap/rotated-output handling);
- workspace-session `down` and fresh `up` of a published session, and guest
  attach;
- consumer verification (Acow/CBS Level 1 flows) with its owner.

No fixture substitutes for these. A later candidate that wires the production
seam re-gates them; that is the planner-owned release gate, not this packet's
credit.

## What the packets qualify natively (exercisable subset)

Per backend — Colima (profile `effigy`), Docker Desktop (context
`desktop-linux`), Linux Docker Engine (Tom's Parallels Ubuntu 26.04 ARM64 VM):

1. **Guarded activation refusal** — `container up --system` refuses with the
   exact production-cut text before preparation or apply, in detached and
   attached variants, with the fixture byte-identical and the engine
   inventory identical before and after (zero effects).
2. **Guest identity observation** — the frozen candidate's Linux binary
   executed inside real backend containers: `__guest-identity` must report
   schema `effigy.guest-probe.v1`, the exact engine container ID, a UUID boot
   ID and the staged binary's SHA-256, stable across calls and distinct per
   container. Where a backend's mountinfo cannot prove placement the named
   refusal is recorded verbatim — per §8.11 that outcome is the backend's
   mount-root qualification result, judged by the retained reviewer.
3. **Host-resolve refusal** — `__guest-reach --resolve` inside a plain
   container refuses by name (`workspace host address unknown` or the
   identity refusal that precedes it): no host resolution without published
   selection evidence.
4. **Host-side negative probes** (macOS host, offline-safe): the host binary
   refuses `__guest-identity` outside a container.
5. **Baseline and cleanup** — exact-ID container/network/volume inventory
   before and after; synthetic containers removed by exact ID; fixture trees
   removed by exact manifest.

## Proof route

Exact selector per backend, run once from this reviewed task head, only after
that packet's exact GO:

```
python3 -I -B docs/qualification/workspace-sessions-a00394012/launch.py --backend colima
python3 -I -B docs/qualification/workspace-sessions-a00394012/launch.py --backend desktop
python3 -I -B docs/qualification/workspace-sessions-a00394012/launch.py --backend linux
```

One source-immutable execution per packet; no retries, no source edits, no
rebuilds during execution. Exactly one retained Queue reviewer owns this task
and judges the actual receipts.

## Required GO provenance (per backend, in order)

1. The complete read-only predicate sweep recorded here (complete for Colima
   on 2026-11-02; Desktop and Linux predicates are environment-gated and
   recorded in their packets).
2. A fresh Supervisor GO whose exact text names the packet SHA, copied
   verbatim to `/Volumes/Builds/scratch/effigy/530/<backend>-r1-go.txt`
   (mode 0600); the Linux receipt must additionally carry a `vm-alias <name>`
   line naming the confirmed VM SSH alias.
3. The Acow owner's (single planner, `cb0e6df4`) confirmed exclusive quiet
   window covering the shared Colima runtime for the window's duration.
4. Tom's explicit live-environment exact-packet GO.

Planner approval is not a GO; no automatic action happens on a missing
authorization. The copied receipt must satisfy the helpers' exact,
case-sensitive checks: `launch.py` requires `packet ` followed immediately by
the lowercase 64-hex packet SHA-256; `verify_go` requires the packet SHA and
the literal strings `GO` and `Supervisor` in the receipt. These tokens are
machine checks, not a substitute for the required authorizations.

Each window needs: a resident guest-toolchain image (named in the packet; for
Colima both `rust:1.97.1-slim-bookworm` and `alpine:3.19` are resident today —
no pull), or the pull named explicitly in that packet's GO request. No packet
pulls anything by itself. `effigy workspace` attach cells are window-only and
never read-only-rehearsed: the attach path attempts a Colima profile start
before its refusal (observed 2026-11-02, zero effect, recorded in the
rehearsal notes).

## Evidence, cleanup and protected state

Receipts, renders, logs and cleanup receipts are 0600 files under
`/Volumes/Builds/scratch/effigy/530/`. The synthetic fixture roots are exactly
`/Users/tom/.effigy-scratch/530-qual-20261102-colima`,
`/Users/tom/.effigy-scratch/530-qual-20261102-desktop` and
`/Users/tom/.effigy-scratch/530-qual-20261102-linuxvm-fixture`; cleanup is exact-ID and
exact-manifest only. Shared state is never pruned, recreated, repaired,
imported, reset, inspected for secret values or re-mounted: the
`effigy-shared-mariadb-0721de3e` project (container
`c47a64b6fc1fe688e3ce503a27704fb978580c22bd4d8c2f7083e509ceb0e6d9`), the
Acow `acowtancy-dev-postgres-data`, `acowtancy-dev-silo-data`,
`acowtancy-dev-dbgate-data` volumes, and every baseline container, network and
volume identity. Colima baseline of record (2026-11-02 read-only sweep, digests
of record in `rehearsal/colima-sweep/`): 15 containers identical to the r3
baseline, 36 networks and 255 volumes identical to the recovery inventory
(`recovery-receipt.json` network digest `6e8fe0fe…`, volume digest
`7168fb1f…`). Colima was recovered only under board decision 2bba81cd; that
recovery grants no window, install or stack authority.

## Rehearsal of record (read-only, 2026-11-02)

- Candidate build: 369 crates, exit 0, binary hash above; source clean at the
  pinned commit.
- Refusal cells against the live Colima engine and with the fixture checkout:
  exact detached and attached refusal texts, fixture zero-write proven by
  per-file SHA-256 before/after, engine inventory identical before/after
  (15/36/255).
- Host negative probes: `__guest-identity` and `__guest-reach --resolve` on
  the macOS host refuse `guest-local identity is unproven`.
- Colima inventory of record captured and matched against the recovery
  baseline (containers == r3 baseline IDs; networks and volumes == recovery
  inventory names).
- Disclosed near-miss: one `effigy workspace` attach attempt outside any
  window tried to start Colima profile `pinned-profile` and failed before any
  effect (`limactl` absent from the scrubbed PATH); `colima list` and the
  engine inventory verified unchanged immediately after. The attach cell class
  is window-only from now on.

Desktop and Linux entry sweeps cannot complete until their backends are up
inside their windows; their packets record the environment-gated predicates and
stop fail-closed on any mismatch.
