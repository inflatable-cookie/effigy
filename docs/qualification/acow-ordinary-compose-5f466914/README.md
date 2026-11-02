# Acow ordinary-Compose consumer qualification packets — evidence (candidate 5f466914, r1)

Evidence packet for the Acow ordinary-Compose consumer qualification (the
Underlay bundle cohort: acowtancy and siblings), prepared as Queue task
effigy#540 and amended by effigy#543 (2026-11-02, Acow owner `cb0e6df4`
message `0fcb8b20`): the routes qualify the CURRENT clean consumer checkout
when the relevant inputs are unchanged, and model `bundle sync` as the
approved source-identity transition; further amended by effigy#545
(2026-11-02): the warm-up prerequisite is the ACTUAL effigy#544 r18
ordinary-Compose proof receipt (exact commit/selector/input hashes, a
settled passed exit-0 execution, and the retained reviewer's acceptance of
that same execution), replacing the superseded green-#539 requirement
(zero execution; #539 history retained as historical only). This directory
is the source-immutable proof head: Queue's proof route executes exactly the
packet, manifest and helpers committed here against the separately frozen
candidate — not the newest main and not a rebuild.

Status: **prepared, not executed.** No warm-up window has run, no
primary-data observation has run, nothing is installed, and no consumer data
has been accessed for this packet. Scenario authority: Acow owner
`cb0e6df4` message `ed11a591` (source/fixture-only preparation). Live
execution, installation and consumer data access remain separately GO-bound.

## Routes

| Route | Exact selector | Armed by | Qualifies |
| --- | --- | --- | --- |
| Worktree warm-up | `python3 -I -B docs/qualification/acow-ordinary-compose-5f466914/warmup.py` | `acow-warmup-go.txt` + accepted #544 r18 proof receipt + Supervisor/Acow/Tom GO chain | The rebuilt candidate on a disposable linked worktree at the exact Queue-QA path length (137 chars), the attach + plain `container up --detach` sequence, the nerdctl/mounts label budget, scoped health, and exact teardown that preserves ALL volumes |
| Primary-data counts | `python3 -I -B docs/qualification/acow-ordinary-compose-5f466914/primary.py` | `acow-primary-go.txt` (its own GO; warm-up GO never arms it) | Primary-data survival: counts-only observation of eight qualified tables through the existing postgres container against the owner baseline |
| Boundary battery | installed-Effigy selector `test:qualification:acow` → `boundaries.py` | Always runnable; fake backends only | The packet's own fail-closed refusals (132 committed checks) |

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
- Consumer checkout `/Users/tom/Dev/projects/acowtancy`: the routes select
  the clean primary main HEAD at execution (the selected head) and verify
  twelve relevant tracked inputs by blob hash AT that head — the root
  manifest, its six declared `[manifest] include` files, the attach script,
  and the four schema baselines. The reviewed anchor pin `b7f004d74…`
  (tree `88df7081…`) stays as the lineage floor (resolvable, tree intact,
  ancestor of the selected head); tools/docs-only drift with unchanged
  relevant inputs passes without a new packet review; an altered relevant
  input refuses with the exact path/identity and a changed include list
  needs a reviewed re-pin.
- Bundle sync is an explicit approved source-identity transition: locked
  pre-state revision `61dc5aa8…` / digest `e088a69e…` (exact, read-only
  from the primary's source lock), approved target digest `28c92040…`
  (exact, from the cached known-version inventory) with revision approved
  to the owner short form `808adad` (the route records the full resolved
  revision in the activation receipt; a reviewed re-pin can tighten the
  prefix). Only the approved old->new transition or an already-settled
  target passes; after sync the exact expected source/content state is
  required before attach/up.

## Prepared-but-blocked facts

The routes refuse today, as designed: no GO receipts exist, the effigy#544
r18 ordinary-Compose qualification has not executed (so no #544 proof
receipt exists), and the installed Effigy is not the candidate. Nothing here
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
- Not independent of the GO chain: the accepted #544 r18 proof receipt,
  exact installed identity, Queue unpaused, QA fence null, Acow reservation,
  and Tom's consumer GO are each machine-checked preconditions; missing
  inputs refuse before effects.
- Not a grant of `project.qa.run`: milestone QA stays a separate planner-run
  step on `main`.
