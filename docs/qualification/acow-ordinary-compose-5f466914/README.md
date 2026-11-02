# Acow ordinary-Compose consumer qualification packets — evidence (candidate 5f466914, r1)

Evidence packet for the Acow ordinary-Compose consumer qualification (the
Underlay bundle cohort: acowtancy and siblings), prepared as Queue task
effigy#540. This directory is the source-immutable proof head: Queue's proof
route executes exactly the packet, manifest and helpers committed here
against the separately frozen candidate — not the newest main and not a
rebuild.

Status: **prepared, not executed.** No warm-up window has run, no
primary-data observation has run, nothing is installed, and no consumer data
has been accessed for this packet. Scenario authority: Acow owner
`cb0e6df4` message `ed11a591` (source/fixture-only preparation). Live
execution, installation and consumer data access remain separately GO-bound.

## Routes

| Route | Exact selector | Armed by | Qualifies |
| --- | --- | --- | --- |
| Worktree warm-up | `python3 -I -B docs/qualification/acow-ordinary-compose-5f466914/warmup.py` | `acow-warmup-go.txt` + green #539 receipt + Supervisor/Acow/Tom GO chain | The rebuilt candidate on a disposable linked worktree at the exact Queue-QA path length (137 chars), the attach + plain `container up --detach` sequence, the nerdctl/mounts label budget, scoped health, and exact teardown that preserves ALL volumes |
| Primary-data counts | `python3 -I -B docs/qualification/acow-ordinary-compose-5f466914/primary.py` | `acow-primary-go.txt` (its own GO; warm-up GO never arms it) | Primary-data survival: counts-only observation of eight qualified tables through the existing postgres container against the owner baseline |
| Boundary battery | installed-Effigy selector `test:qualification:acow` → `boundaries.py` | Always runnable; fake backends only | The packet's own fail-closed refusals (62 committed checks) |

Full route contracts, acceptance criteria, the GO token contracts, the
refusal inventory and the evidence contract are in `packet-r1.md`; the
pinned identities (candidate, consumer pins, bundle lock, protected state,
engine, expected counts with tracked-source citations) are in
`manifest-r1.json`.

## Frozen subject

- Candidate source `5f4669141d8dabd91b7fbfef28d2bce21bc42ba2`, binary SHA-256
  `cb564fa64f15d080eb2b047454b55103e3e5857de2252de8be05a31a95eb00b1` (same
  subject as the #537 Colima packet). The binary is not committed and is not
  installed by this task; activation requires the exact installed version
  and binary SHA in the GO receipt.
- Consumer main pin `b7f004d74a47f702f4dc3ef5c96390f0bdbf3c0b` (tree
  `88df7081…`) with eight pinned input blob hashes, including
  `infra/tasks.toml:16` (`worktree:container:attach`) and the
  attach script (`scripts/tasks/worktree-container-attach.ts`, task-795
  pointer + hash-verified local-dev copy authority). Bundle lock
  `61dc5aa8405698ea757f0ff5c481cb6d7cb92314` (underlay-effigy-bundle), read
  via `effigy bundle inspect --json`; a moved lock refuses and needs a
  reviewed re-pin.

## Prepared-but-blocked facts

The routes refuse today, as designed: no GO receipts exist, no green #539
receipt exists, and the installed Effigy is not the candidate. Nothing here
is a failure; it is the fail-closed preparation state. The primary-data
route additionally refuses while the primary dev stack is down.

## Protected state

The shared MariaDB project (container
`c47a64b6fc1fe688e3ce503a27704fb978580c22bd4d8c2f7083e509ceb0e6d9`,
`effigy-shared-mariadb-0721de3e-mysql-data`), the Acow
`acowtancy-dev-postgres-data` / `acowtancy-dev-silo-data` /
`acowtancy-dev-dbgate-data` volumes, every older scoped volume, and every
baseline container, network and volume identity are never pruned, recreated,
repaired, imported, reset, inspected for secret values, or re-mounted.
Teardown preserves ALL volumes; no volume deletion and no global cleanup.

## What this packet is not

- Not an execution, an installation, or consumer qualification: passing the
  boundary battery is not consumer proof, and synthetic success alone is
  never task completion.
- Not independent of the GO chain: #539 green, exact installed identity,
  Queue unpaused, QA fence null, Acow reservation, and Tom's consumer GO are
  each machine-checked preconditions; missing inputs refuse before effects.
- Not a grant of `project.qa.run`: milestone QA stays a separate planner-run
  step on `main`.
