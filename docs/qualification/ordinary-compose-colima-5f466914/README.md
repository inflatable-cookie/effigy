# Ordinary-Compose on Colima — qualification evidence (candidate 5f466914, r18)

Evidence packet for the deferred ordinary-Compose Colima qualification and the
Acow consumer step behind already-merged #524/#535, prepared as Queue tasks
effigy#537 (committed r17 packet) and effigy#542 (self-enforcing r18 packet).
This directory is the source-immutable proof head: Queue's proof route executes
exactly the packet, manifest and helpers committed here against the separately
frozen candidate — not the newest main and not a rebuild.

Status: **prepared, not executed.** The synthetic Colima cell has never run
for this packet; no fixture or native resource was ever created. The r18
launcher performs the complete read-only entry sweep itself before any fixture
can be created. Native execution, any installation, and consumer proof all
remain unproved and unauthorized.

## Proof route

Exact selector, run once from this reviewed task head, only after the exact
packet GO:

```
python3 -I -B docs/qualification/ordinary-compose-colima-5f466914/launch.py
```

One source-immutable execution through Queue's proof route; no retries, no
source edits, no rebuilds during execution. Exactly one retained Queue
reviewer owns the proof task and judges the actual receipt.

## Bound subject and committed artifacts

- Frozen candidate source commit: `5f4669141d8dabd91b7fbfef28d2bce21bc42ba2`
  (clean detached scratch checkout
  `/Volumes/Builds/scratch/effigy/524/candidate-src-5f466914`).
- Frozen candidate binary:
  `candidate-src-5f466914/target/debug/effigy`, SHA-256
  `cb564fa64f15d080eb2b047454b55103e3e5857de2252de8be05a31a95eb00b1`. The
  binary is not committed and must not be installed.
- Committed packet `colima-qualification-5f466914-r18.md`, SHA-256
  `545566ddd6f48bc14f2f16f1df0db68a4c16476293b0c54cbe58e46331d8e286`. The
  prior committed r17 revision (SHA-256
  `a9c83d12e693ba318fc45cb12a55b09da68eacdd27369c066eb1abe9baa0e7c9`) remains
  immutable history and is not executed.
- Committed artifact manifest `r18-artifacts.json`, SHA-256
  `944419d4517f2b1935e6870989408990cf84412264e999a9771644627f2c1291`; it pins
  the frozen binary, the 15-container baseline IDs (scratch
  `r3-baseline-container-ids.txt`, SHA-256
  `d89c846cdb44c0d9669f4db50eb57347d2b322aee26b4b03c7fbd2e000663f10`) and the
  six committed helpers:
  - `launch.py` `9bbf549e41475697347321d6de496a6a42fe416006ec63b7cc465513bd0c42b4`
  - `common.py` `5e7883525f6304d0a4db90b565ef95c6c5e01fb00bdb10d58425d6e94a4f04c4`
  - `prepare.py` `62883a92d16ec31c82586295c4d8b85447043818fe3a3752e55c0c2e9acd5ab8`
  - `preflight.py` `06d3acb21e0a0b8f54acdb24c5f033ce7263e1cb3a8e65a7c38143ab35e40878`
  - `runtime.py` `43d2884f3adf45dd82277ece37051efe0d8bb25ea5d112bd5b9033531b29b695`
  - `source_estimate.py` `0d2e57308104ed76b1932092760626da89c8e6441fa055784a971e86e0801b52`

The r17 helper hashes (launch `cedcd5f2...`, common `098570fc...`, prepare
`b122bb8f...`, preflight `ee427a1e...`, runtime `5723ecea...`, estimate
`0d2e5730...`) stay pinned by the retained r17 manifest as reviewed history.

Provenance: the starting r17 artifacts were prepared in scratch
(`/Volumes/Builds/scratch/effigy/524/colima-qualification-5f466914-r17.md`
SHA-256 `34c0c9288b1d174e346460b78839524c00882646abba21ed8da5dac0f074046d`,
`r17-artifacts.json` SHA-256
`e710d83ef655e3e67a4f0b42167aa941f6ded05e498de1558096051c8b37b612`). Those
scratch bytes are provenance for the starting artifact and are not executed;
the committed r18 files above are the packet intended for the authorized
execution.

## Entry sweep the launcher enforces (task effigy#542)

The r17 route demanded a manual pre-selector sweep the packet did not itself
enforce; three executor admissions refused before the selector and none
created anything. The r18 launcher runs the complete read-only admission sweep
itself — via `common.entry_sweep`, invoked by `launch.py` before `prepare.py`
can make its first `mkdir` or write, and again inside `prepare.py` before the
same first write — covering: bundle/GO identities, the profile and image pins,
the exact 15-container baseline, absence of every synthetic container, network
and fixture root, the complete 36-network and 255-volume identity baselines
(canonical digests `6e8fe0fe...` and `7168fb1f...` pinned from the immutable
2026-11-02 recovery receipt `5d05e690...` and its inventory `33239d95...`,
never from live state), the protected shared MariaDB container/volume and Acow
dev volume identities, bounded fixture-ancestor mount/symlink safety before
creation (and again before deletion), and prior-evidence absence. `runtime.py`
re-asserts both baseline digests immediately before `compose up`. Drifted,
same-count-replaced, malformed, duplicate, truncated or timed-out inventories
refuse with zero fixture writes and zero backend mutation. The executor's
manual duties are exactly: verify the actual scoped GO evidence and current
Acow reservation, prepare the faithful required private input under that GO,
and run the single selector once.

## Guard change (queue planner a6994b17)

The r17 origin/main-equality predicate (remote `main` must equal the candidate
commit at launch) is replaced by content verification: the exact candidate git
commit and clean source, the binary hash, the packet/manifest/helper hashes
and the artifact identities. Main has moved past the intentionally frozen
candidate; that movement is disclosed in the packet and does not invalidate
the subject. No remote is consulted at proof time. Every other identity,
authorization, baseline, ownership and cleanup predicate is preserved
byte-for-byte from the reviewed r17 helpers.

## Required GO provenance

Native execution needs, in order: the launcher's complete read-only entry
sweep (now self-enforcing — see above), a fresh Supervisor GO whose exact text
names the r18 packet's SHA copied verbatim to
`/Volumes/Builds/scratch/effigy/524/r18-go.txt` (mode 0600), Acow's confirmed
current exclusive quiet window with a start courtesy, and Tom's explicit
live-environment exact-packet GO. Planner approval is not a GO; no automatic
action happens on a missing authorization. A GO naming any other packet
revision (including the r17 hash) refuses with no native effect.

The copied receipt must also satisfy the helpers' exact, case-sensitive text
checks: `launch.py` requires `packet ` followed immediately by the lowercase
64-hex r18 packet SHA-256; `verify_go` requires that SHA and the literal
strings `GO` and `Supervisor` in the receipt. These tokens are machine
checks, not a substitute for the required authorizations and provenance above.
A differently worded receipt can fail the one permitted proof execution.

## Evidence, cleanup and protected state

Execution receipts, renders, logs and cleanup receipts are 0600 files written
to the scratch qualification directory `/Volumes/Builds/scratch/effigy/524/`.
The synthetic fixture roots are exactly
`/Users/tom/.effigy-scratch/524-qual-20261101-lvfx` and
`/Users/tom/.effigy-scratch/524-qual-20261101-mixx`; cleanup is exact-ID and
exact-manifest only. Shared consumer state — the `effigy-shared-mariadb-0721de3e`
project (container `c47a64b6fc1fe688e3ce503a27704fb978580c22bd4d8c2f7083e509ceb0e6d9`),
`acowtancy-dev-postgres-data`, `acowtancy-dev-silo-data`,
`acowtancy-dev-dbgate-data`, and all 15 baseline containers, 36 networks and
255 volumes — is never pruned, recreated, repaired, imported, reset, inspected
for secret values, or re-mounted. Recovery baseline closure:
`/Volumes/Builds/scratch/effigy/colima-recovery-20261102/closure.md`.

## Unproved consumer gates

This packet's synthetic mount cell is not consumer proof. After any synthetic
receipt, consumer verification is a separately activated owner step: Tom
authorizes the installation/outward boundary; Acow receives the exact
installed version and binary SHA plus a fresh baseline; only then may Acow's
approved plain-Compose warm-up run on the unchanged main manifest, comparing
its historical data counts without reading row values. If consumer proof
cannot be obtained, that release gate stays open; synthetic success alone is
not task completion.
