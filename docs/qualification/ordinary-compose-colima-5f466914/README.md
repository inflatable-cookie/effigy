# Ordinary-Compose on Colima — qualification evidence (candidate 5f466914, r17)

Evidence packet for the deferred ordinary-Compose Colima qualification and the
Acow consumer step behind already-merged #524/#535, prepared as Queue task
effigy#537. This directory is the source-immutable proof head: Queue's proof
route executes exactly the packet, manifest and helpers committed here against
the separately frozen candidate — not the newest main and not a rebuild.

Status: **prepared, not executed.** The synthetic Colima cell has never run
for this packet; the entry read-only sweep was incomplete (guest I/O error,
disclosed in the packet). Native execution, any installation, and consumer
proof all remain unproved and unauthorized.

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
- Committed packet `colima-qualification-5f466914-r17.md`, SHA-256
  `a9c83d12e693ba318fc45cb12a55b09da68eacdd27369c066eb1abe9baa0e7c9`.
- Committed artifact manifest `r17-artifacts.json`, SHA-256
  `94bf2d85e9727124d4356b8d0d5ef4d4b8f081af8b8912fff410472faacec898`; it pins
  the frozen binary, the 15-container baseline IDs (scratch
  `r3-baseline-container-ids.txt`, SHA-256
  `d89c846cdb44c0d9669f4db50eb57347d2b322aee26b4b03c7fbd2e000663f10`) and the
  six committed helpers:
  - `launch.py` `cedcd5f2c2da52d2fe7313cd08f10017cca3623166e212256e62f3c90779e12e`
  - `common.py` `098570fce46c59735bdc953812d589da5fb1c447546c8cd49e62a0ee19b1dda2`
  - `prepare.py` `b122bb8f58e7954bc8a2f0c204d888c4ea5fbe9e6433163aa96326ad143514cd`
  - `preflight.py` `ee427a1edb62cff07dd4b645e55d1fba27484fe0cc87196b1a0f6064e7b2e957`
  - `runtime.py` `5723eceab4210e7fa02ba803e363561869f4abc6d3128f04648c4a5772ccc642`
  - `source_estimate.py` `0d2e57308104ed76b1932092760626da89c8e6441fa055784a971e86e0801b52`

Provenance: the starting r17 artifacts were prepared in scratch
(`/Volumes/Builds/scratch/effigy/524/colima-qualification-5f466914-r17.md`
SHA-256 `34c0c9288b1d174e346460b78839524c00882646abba21ed8da5dac0f074046d`,
`r17-artifacts.json` SHA-256
`e710d83ef655e3e67a4f0b42167aa941f6ded05e498de1558096051c8b37b612`). Those
scratch bytes are provenance for the starting artifact and are not executed;
the committed files above are the executed packet.

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

Native execution needs, in order: the complete read-only predicate sweep
(currently incomplete after the Colima guest I/O error), a fresh Supervisor GO
whose exact text names this packet's SHA copied verbatim to
`/Volumes/Builds/scratch/effigy/524/r17-go.txt` (mode 0600), Acow's confirmed
current exclusive quiet window with a start courtesy, and Tom's explicit
live-environment exact-packet GO. Planner approval is not a GO; no automatic
action happens on a missing authorization.

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
