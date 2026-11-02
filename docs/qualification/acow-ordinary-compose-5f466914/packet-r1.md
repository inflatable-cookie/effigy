# Acow ordinary-Compose consumer qualification packet (candidate 5f466914, r1)

Queue task effigy#540, lane v015-qualification. This packet prepares — and
does not execute — the consumer qualification of the rebuilt ordinary-Compose
candidate on the Acow cohort (Underlay consumer; acowtancy and siblings).
Scenario authority: Acow owner `cb0e6df4-3a97-4c4e-bd81-34857e1a9b9c` message
`ed11a591-6e6a-4e88-8101-b925091e12ad` (source/fixture-only packet
preparation). Execution authority is separate and listed below under "GO
provenance"; planner approval is not a GO.

**Status: PREPARED, NOT EXECUTED.** No warm-up window has run. No
primary-data observation has run. No installation, no native effect, and no
consumer data access has happened for this packet. Actual execution is a
later proof-only task at this packet's merged commit, requiring task
effigy#539 to be green first, the exact installed candidate identity, and
separate operator/consumer GOs.

## Artifacts manifest SHA-256: `1ee937b7ec438a216bae8fd15d7f17abed44a3ec75160bd099c9a13f3e3da826`

The manifest pins the four committed helpers, the frozen candidate identity,
the consumer input pins, the protected-state identities, the engine
identities, the fixture shape rule and the expected primary-data counts with
their tracked-source citations. This directory is the source-immutable proof
head: Queue's proof route executes exactly the packet, manifest and helpers
committed here — not the newest main and not a rebuild.

## Frozen subject and consumer input pins

- Frozen ordinary candidate source commit
  `5f4669141d8dabd91b7fbfef28d2bce21bc42ba2`; candidate binary SHA-256
  `cb564fa64f15d080eb2b047454b55103e3e5857de2252de8be05a31a95eb00b1`
  (identical subject to the reviewed #537 Colima packet). The binary is not
  committed and is not installed by this task.
- Consumer (Acow) main pin: `b7f004d74a47f702f4dc3ef5c96390f0bdbf3c0b`
  (tree `88df708155e60deb8c8179cffd3c307a3baf5e01`), read-only at
  `/Users/tom/Dev/projects/acowtancy`; the worktree the warm-up creates
  materializes exactly this pin, detached. The primary checkout itself may
  have moved on; content identity is verified through the pin commit's tree
  and eight pinned input blob hashes (`infra/tasks.toml` — the
  `worktree:container:attach` task at line 16 — `infra/workspace.toml`,
  `effigy.toml`, `scripts/tasks/worktree-container-attach.ts`, and the four
  schema baselines cited for the counts below).
- Bundle pin: `git@github.com:inflatable-cookie/underlay-effigy-bundle.git`
  locked at `61dc5aa8405698ea757f0ff5c481cb6d7cb92314` (the lock observed at
  preparation via `effigy bundle inspect --json`, read-only). A moved lock at
  activation refuses; it needs an explicit reviewed re-pin, like any changed
  Acow/bundle input. A packet for 5f466914 is not final qualification of
  later main.

Vault files, ciphertext, env values, credentials and database row values are
never read by this packet. Acow and bundle repositories are never edited.

## Routes

Two separately armed proof routes plus one committed boundary battery. Each
route runs once from the reviewed task head; no retries, no source edits, no
rebuilds during execution. Exactly one retained Queue reviewer owns the proof
task and judges the actual receipts.

```
python3 -I -B docs/qualification/acow-ordinary-compose-5f466914/warmup.py
python3 -I -B docs/qualification/acow-ordinary-compose-5f466914/primary.py
```

The boundary battery (`boundaries.py`) runs through the installed-Effigy
selector `test:qualification:acow` and uses fake backends and disposable
fixture metadata only. Passing those controls is not consumer qualification.

## GO provenance (per route, in order)

1. Task effigy#539 green: a receipt file copied to
   `/Volumes/Builds/scratch/effigy/540/539-green-receipt.json` (mode 0600)
   whose SHA-256 the GO receipt quotes on a `539 receipt <sha256>` line.
   Machine contract: JSON object with `"task": "effigy#539"`,
   `"status": "green"`, a 40-hex `"head"`, and a non-empty `"selectors"`
   list. Absent, malformed, non-green or hash-drifted receipts refuse the
   warm-up route before any effect.
2. A fresh Supervisor GO naming this packet's SHA, copied verbatim to
   `/Volumes/Builds/scratch/effigy/540/acow-warmup-go.txt` (mode 0600) for
   the warm-up route, or to `/Volumes/Builds/scratch/effigy/540/acow-primary-go.txt`
   for the primary-data route.
3. Acow's confirmed exclusive quiet window with a start courtesy, recorded in
   the GO receipt as `reservation <id>` (Acow owner `cb0e6df4`).
4. Tom's explicit consumer GO. The routes are separately armed: the warm-up
   GO must additionally carry the literal tokens `Supervisor`, `Tom`,
   `colima idle`, `queue unpaused`, `qa fence null`, the candidate binary
   SHA-256, a `version <line>` quoting the exact installed `effigy --version`
   line, and a `binary <path>` line naming the installed binary; the
   primary-data GO must carry `Tom`, `primary-data`, `queue unpaused`,
   `qa fence null`. A GO for one route never arms the other. These tokens
   are machine checks, not a substitute for the required authorizations and
   provenance; a differently worded receipt fails the one permitted execution.

Preconditions verified machine-readably before any effect: green #539
receipt; exact installed identity (PATH `effigy --version` equal to the
GO-quoted line, carrying the `5f46691` source marker, and the GO-named
binary hashing exactly to `cb564fa6…`); clean pinned Acow main with intact
pin/tree/blob content; bundle lock on the pinned revision (warm-up route,
read-only `bundle inspect` against the pinned Acow primary and again after
the window's `bundle sync`); Queue unpaused and QA fence null (GO tokens);
no other Colima activity (engine inventory canonical, shared MariaDB
running, no `acowtancy-dev-wt-*` scope network already present); fresh
protected baseline captured at activation; Acow reservation and Tom
consumer-GO provenance. Every missing or mismatched input refuses before
worktree creation or any native effect.

## Worktree warm-up route (future separate GO)

Fixture: a disposable linked worktree whose path matches the Queue QA shape
`/Volumes/Builds/queue/workspaces/<64-hex>/ns-<uuid>` in exact length
(137 characters) — parent `/Volumes/Builds/scratch/effigy/w` (32
characters), a real exact-owned SHA-256 component, and this task's UUID
`13f69c1b-9a5e-455e-8004-af30ece9685f`. A short path does not qualify the
nerdctl mounts-label size this warm-up exists to observe. The route refuses
if the derived path exists, the parent is not a private owned directory, or
the length computation cannot hit the shape exactly.

Exact owner-approved sequence, in the worktree, under the installed
candidate:

1. `effigy bundle sync` — then the bundle lock must still name
   `61dc5aa8…`; a moved lock refuses (re-pin required).
2. `effigy worktree:container:attach` (Acow `infra/tasks.toml:16`; existing
   task-795 pointer plus hash-verified local-dev ciphertext-copy authority
   only). The route verifies the written fresh-session record against the
   pinned session derivation
   (`wt-<sanitized-basename>-<sha256(path)[:8]>`, project
   `acowtancy-dev-<session>`), requires a copied/verified vault action, and
   refuses `none found` before `up`. Attach and secret operations run only
   in the authorized consumer window, never as preparation or rehearsal.
3. plain `effigy container up --detach` — NEVER `--system`; the route's
   command allowlist is exact and refuses anything else.

Acceptance, recorded with identities and lengths only: exit 0; workspace
container created; scoped service set exactly `{workspace, postgres, silo,
mailpit, dbgate}`, all running; the serialized `nerdctl/mounts` label
length recorded (value never kept) and required `< 4096` bytes; shared
MariaDB container `c47a64b6fc1fe688e3ce503a27704fb978580c22bd4d8c2f7083e509ceb0e6d9`
unchanged and `mysql-data` volume untouched. Full baseline identities differ
only by exact-owned additions.

Teardown: `effigy container down` exit 0; zero containers with the scoped
project label; inventory judgment restores the exact fresh baseline; the
exact owned worktree is removed by `git worktree remove --force` only after
its ownership marker validates, with the copied vault path confirmed gone
with it. ALL volumes are preserved — including this window's own scoped
volumes and every older scoped volume; no volume deletion and no global
cleanup ever runs. Ambiguous ownership, leftover scoped containers or
networks, unrecognized new volumes, or a failed removal stop and report
instead of guessing (`ambiguous-stopped`, exit 4).

## Primary-data selector (separate GO, never implied by the warm-up)

Worktree volumes are fresh and cannot prove primary-data survival, so this
is a separately armed, independently invocable route against the primary
checkout's own stack. Owner-run window:

1. `effigy dev --headless` in `/Users/tom/Dev/projects/acowtancy` (owner).
2. This selector: counts-only observation through the existing postgres
   container (project `acowtancy-dev`, exactly one service-postgres
   container, mounting `acowtancy-dev-postgres-data`; anything else
   refuses before connecting).
3. `effigy dev:down` (owner) and no remaining runs. The selector never
   starts or stops the stack and never runs `dev:down` itself.

Expected owner baseline (qualified names resolved from the pinned tracked
sources at `b7f004d74…`; never invented):

| Qualified table | Expected count | Tracked source citation (at the pin) |
| --- | --- | --- |
| `auth.users` | 57280 | `state/schema/202608221002__baseline_auth.sql:5` |
| `auth.legacy_user_link` | 57279 | `state/schema/202608221002__baseline_auth.sql:33` |
| `billing.orders` | 45540 | `state/schema/202608221011__baseline_billing.sql:357` |
| `engagement.feedback` | 2372 | `state/schema/202608221010__baseline_engagement.sql:5` |
| `engagement.complaint` | 62 | `state/schema/202608221010__baseline_engagement.sql:26` |
| `learning.pathway` | 5 | `state/schema/202608221003__baseline_learning.sql:5` |
| `learning.module` | 65 | `state/schema/202608221003__baseline_learning.sql:40` |
| `learning.outcome` | 6689 | `state/schema/202608221003__baseline_learning.sql:150` |

Observation is exactly one command shape per table —
`psql -U postgres -d acowtancy --no-psqlrc -v ON_ERROR_STOP=1 -Atc
"SELECT count(*) FROM <schema.table>"` inside the postgres container — and
nothing else. Count output must be a bare integer; anything else refuses.
The receipt records counts only (never row values, never raw child output)
against this baseline; any mismatch records the observed counts, marks
`count-mismatch` and fails closed — a drifted baseline needs an explicit
owner re-pin, not an adjusted expectation. The shared MariaDB container and
`effigy-shared-mariadb-0721de3e-mysql-data` identity are verified before and
after; any drift refuses. No warm-up worktree scope may be active. This
route grants no `project.qa.run`; manual project QA remains a later separate
step.

## Refusal inventory (each refuses before any effect)

Default/missing GO receipt; GO at the wrong scratch path or with bad file
hygiene (mode/owner/symlink); GO naming a different packet SHA; GO missing
any required token, reservation line, or route marker; route cross-arming
(a warm-up GO under the primary route or vice versa); absent, malformed,
non-green or hash-drifted #539 receipt; installed identity drift (version
line, source marker, binary hash, symlinked binary); moved Acow pin tree or
input blob; dirty primary checkout; pin no longer an ancestor of primary
HEAD; moved bundle lock; wrong fixture parent length or foreign parent;
existing fixture path; ambiguous engine inventory or malformed identities;
missing or non-running shared MariaDB; missing shared MariaDB volume;
already-active warm-up scope; primary stack down, ambiguous postgres, or
postgres not mounting the primary data volume; non-numeric count output;
count mismatch; leftover scoped containers; unrecognized new volumes;
baseline identity drift; protected volume identity drift; ownership marker
missing or mismatched at teardown.

## Evidence contract

Receipts, invocation records and console logs are 0600 files under
`/Volumes/Builds/scratch/effigy/540/`, created exclusively (no retries,
evidence immutable). Every receipt carries: the task and route; packet and
manifest SHAs; GO receipt SHA and reservation id; #539 receipt SHA; the
candidate and input pins; exact owned identities (fixture path, session,
scoped project, container IDs); statuses; count results only; and a cleanup
inventory with preserved volumes. Row values, vault/ciphertext/env content,
credentials and raw secret-bearing child output are never recorded. The
boundary battery writes only disposable fixture metadata under the same
scratch root and removes it before exiting.

## Future activation requirements

This task delivers packet source and fixture verification only. Installation
is not part of it. A changed product candidate or Acow/bundle input needs an
explicit reviewed re-pin. Exactly one retained Queue reviewer owns the PR;
no extra review or native-reader agents or workspaces. Hosted CI is not a
task gate.
