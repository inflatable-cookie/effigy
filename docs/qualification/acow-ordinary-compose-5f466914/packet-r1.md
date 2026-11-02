# Acow ordinary-Compose consumer qualification packet (candidate 5f466914, r1)

Queue task effigy#540, lane v015-qualification; amended by Queue task
effigy#543 (2026-11-02, Acow owner `cb0e6df4` message `0fcb8b20`): the packet
qualifies the CURRENT clean consumer checkout when the relevant inputs are
unchanged, and models `bundle sync` as the approved source-identity
transition instead of demanding the historical lock on both sides; further
amended by Queue task effigy#545 (2026-11-02): the warm-up prerequisite is
the ACTUAL effigy#544 r18 ordinary-Compose proof receipt — exact
commit/selector/input hashes, a settled passed exit-0 execution, and the
retained reviewer's acceptance of that same execution — replacing the
superseded green-#539 requirement (zero execution; #539 history retained as
historical only). This
packet prepares — and does not execute — the consumer qualification of the
rebuilt ordinary-Compose candidate on the Acow cohort (Underlay consumer;
acowtancy and siblings). Scenario authority: Acow owner
`cb0e6df4-3a97-4c4e-bd81-34857e1a9b9c` message
`ed11a591-6e6a-4e88-8101-b925091e12ad` (source/fixture-only packet
preparation). Execution authority is separate and listed below under "GO
provenance"; planner approval is not a GO.

**Status: PREPARED, NOT EXECUTED.** No warm-up window has run. No
primary-data observation has run. No installation, no native effect, and no
consumer data access has happened for this packet. Actual execution is a
later proof-only task at this packet's merged commit, requiring the accepted
effigy#544 r18 ordinary-Compose proof receipt first, the exact installed
candidate identity, and separate operator/consumer GOs.

The primary lock is a preflight authority check. The disposable worktree must have no lock before sync; its receipt records that absence, and the exact approved target is required after sync before attach.

## Artifacts manifest SHA-256: `57dcd430ef1cc92b0976a88cec614fbe8deb520d141af30d7de2798f6596ff05`

The manifest pins the four committed helpers, the frozen candidate identity,
the relevant-input closure with its selection policy, the reviewed anchor
pin, the approved bundle transition, the protected-state identities, the
engine identities, the fixture shape rule and the expected primary-data
counts with their tracked-source citations. This directory is the
source-immutable proof head: Queue's proof route executes exactly the
packet, manifest and helpers committed here — not the newest main and not a
rebuild.

## Frozen subject and consumer inputs

- Frozen ordinary candidate source commit
  `5f4669141d8dabd91b7fbfef28d2bce21bc42ba2`; candidate binary SHA-256
  `cb564fa64f15d080eb2b047454b55103e3e5857de2252de8be05a31a95eb00b1`
  (identical subject to the reviewed #537 Colima packet). The binary is not
  committed and is not installed by this task.
- Consumer (Acow) checkout: read-only at `/Users/tom/Dev/projects/acowtancy`.
  The routes SELECT the clean primary main HEAD at execution (the "selected
  head") and materialize the disposable warm-up worktree exactly at that
  recorded commit — never at the historical anchor and never at a silently
  substituted checkout; the selected head is re-verified immediately before
  the first effect and carried into the receipts. The reviewed anchor pin
  `b7f004d74a47f702f4dc3ef5c96390f0bdbf3c0b` (tree `88df7081…`) stays as the
  lineage floor: it must still resolve, its tree must still match, and it
  must remain an ancestor of the selected head. Whole-repository
  tools/docs-only drift with unchanged relevant inputs passes without a new
  packet review (observed history: the anchor moved to
  `759e29ca0…` through PR1075 and to `ee037b045…` through PR1076, both
  tools/-only, all twelve relevant inputs identical throughout, verified
  2026-11-02).
- Relevant inputs: twelve tracked nonsecret files pinned by blob hash and
  verified AT the selected head — the root `effigy.toml`, its six declared
  `[manifest] include` files (`infra/workspace.toml`, `infra/tasks.toml`,
  `infra/deploy.toml`, `infra/secrets.toml`, `infra/bootstrap.toml`,
  `state/acowtancy.state.toml`; the transitive tracked includes the stack
  configuration actually loads), the attach task's script
  (`scripts/tasks/worktree-container-attach.ts`, task-795 pointer plus
  hash-verified local-dev ciphertext-copy authority), and the four schema
  baselines cited for the primary-data counts. An altered relevant input
  refuses with the exact changed path and both identities; a declared
  include list other than the reviewed list (order included) is a new
  relevant input shape that requires an explicit reviewed re-pin. A dirty,
  unreadable or replaced primary head refuses closed.
- Bundle transition: source
  `git@github.com:inflatable-cookie/underlay-effigy-bundle.git` declared as
  `{type: git, url, ref: null}`. The LOCKED PRE-STATE is exact — revision
  `61dc5aa8405698ea757f0ff5c481cb6d7cb92314` with content digest
  `sha256:e088a69e937fe03d044dac0aa95297c6e0d4ee5c14cef6a244fd6bf6daacc38e`
  (read from the primary checkout's `.effigy/sources.lock.json`, read-only).
  The APPROVED TARGET is content-exact — digest
  `sha256:28c920404c34cc46d223b061b694903634b592f99a9f10f06f4f9f8f0058975d`
  (resolved from the cached known-version inventory under the canonical
  source key `fc8e886a…`, observed as `available digest 28c92040` by
  `effigy bundle inspect`, 2026-11-02) — with the git revision known to the
  owner-approved short form `808adad` (full 40-hex revision unresolvable
  without a fetch, which this preparation never does; the route records the
  full resolved revision in the activation receipt, and a reviewed re-pin can
  tighten the prefix once known). Revision and digest kinds are never
  equated or interchanged. The packet permits exactly the approved
  old->new transition and an already-settled target; after the window's
  `bundle sync` it requires that exact expected source/content state, with
  no residual staleness, BEFORE attach/up. Any other pre-state, an unexpected
  post-sync source-origin, revision or content drift, or a missing identity
  refuses; it needs the explicit reviewed re-pin any changed Acow/bundle
  input needs. A packet for 5f466914 is not final qualification of later
  main.

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

1. The accepted effigy#544 r18 proof receipt: a faithfully serialized
   extraction of the exact Queue records, copied by the planner to
   `/Volumes/Builds/scratch/effigy/540/544-proof-receipt.json` (mode 0600)
   after qualification settles, whose SHA-256 the GO receipt quotes on a
   `544 receipt <sha256>` line. Machine contract (schema
   `effigy.qualification.acow-ordinary-compose.prerequisite-544.v1`): the
   exact task (`effigy#544` / `a648d634-82cf-4b3c-8903-a009eb427c12`); the
   pinned proof binding (`proofOnly.commit` `0ed7e839…`, the exact r18
   selector `python3 -I -B
   docs/qualification/ordinary-compose-colima-5f466914/launch.py`, and all
   three input hashes — r18 packet `545566dd…`, r18 artifacts manifest
   `944419d4…`, the frozen candidate binary `cb564fa6…` itself); a settled
   `passed` execution record with exit 0 that is neither historical nor
   refused-before-selector; the `proofOnlyResult` binding that same
   execution run and a distinct retained reviewer run; the retained review
   `ready_to_merge` at the same commit; and the durable `done` outcome with
   its `ready_to_merge` review and pr null. Proof-only never publishes, so
   both the retained and the durable publication ids are null
   (`reviewId`/`reviewCommentId` null, `reviewId`/`commentId` null); that
   all-null pair is the legitimate NO-PR transport shape and is never
   itself a link — the linkage is the retained run-id pair
   (`execution.runId` == `proofOnlyResult.executionRunId`,
   `review.runId` == `proofOnlyResult.reviewerRunId`). Any non-null
   publication id is invented surrogate evidence and refuses. Supported
   Queue sources: `server/report.ts` `reportSchema` omits
   `reviewId`/`reviewCommentId`; `server/resolution.ts` proof-only
   `advanceReviewerResult` records `proofOnlyResult` with
   `executionRunId`/`reviewerRunId` and never publishes;
   `server/review-verdicts.ts` `reviewRecordFor` defaults both publication
   ids to null; `server/store.ts` outcome projects missing publication ids
   as null. Records copied: `queue.detail(taskId)` →
   `task.proofOnly`, `task.proofExecution`, the `task.proofExecutions` entry
   named by `proofOnlyResult.executionRunId`, `proofOnlyResult` itself, and
   the `task.resolution.reviews` entry named by
   `proofOnlyResult.reviewerRunId`; `queue.outcome(taskId)` →
   `{taskId, reference, outcome, reason, pr, review}`. The extraction is
   transport only — a file alone grants no authority; the internal linkage
   is validated, no cryptographic identity is invented. Absent, malformed,
   ambiguous, mismatching or hash-drifted receipts refuse the warm-up route
   before any effect. The superseded effigy#539 green receipt (zero
   execution; its three authenticated `refusedBeforeSelector` receipts are
   retained as history) is not a prerequisite, and a fresh GO carrying a
   `539 receipt` line refuses.
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

Preconditions verified machine-readably before any effect: accepted #544
r18 proof receipt; exact installed identity (PATH `effigy --version` equal to the
GO-quoted line, carrying the `5f46691` source marker, and the GO-named
binary hashing exactly to `cb564fa6…`); clean selected Acow main with the
approved anchor intact and every relevant input unchanged at the selected
head; bundle lock at an approved pre-state (locked old identity with the
approved transition pending, or already settled at the approved target;
read-only `bundle inspect` plus the primary's source lock, and again after
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
candidate (the worktree materializes detached at the recorded SELECTED
current head, re-verified immediately before this first effect):

1. `effigy bundle sync` — then the worktree's lock must name exactly the
   approved target state (typed revision `808adad…` plus the exact content
   digest `28c92040…`, stale flag settled); any unexpected post-sync
   source-origin, revision or content drift refuses BEFORE attach/up
   (re-pin required). An already-settled pre-state syncs as a no-op to the
   same approved target and passes. Before and after identities are
   preserved in the activation receipt.
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

Expected owner baseline (qualified names resolved from the tracked
schema sources, whose blobs are verified at the selected head before the
route connects; never invented):

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
ambiguous, mismatching or hash-drifted #544 proof receipt — wrong task or
task id, commit/selector/input/candidate drift, missing, failed, pending,
historical or refused-before-selector execution, nonzero exit, missing
reviewer, execution/reviewer-run/head/selector/input mismatch, invented
publication evidence on either the retained or the durable review (any
non-null reviewId/reviewCommentId/commentId; proof-only never publishes),
contradictory outcome, tampered,
truncated or oversize data — or a stale #539 token in the GO; installed identity drift (version
line, source marker, binary hash, symlinked binary); anchor pin or anchor
tree drift; relevant input changed, missing or unreadable at the selected
head (exact path/identity in the refusal); declared include list other than
the reviewed closure (new relevant shape; reviewed re-pin); dirty, unreadable
or replaced primary head; selected head moved between preflight and the
first effect; bundle lock at an unapproved identity, an untyped revision or
digest, a changed declaration/source, or a hint that disagrees with the
locked pin; unexpected post-sync source-origin, revision or content drift;
post-sync lock missing the revision or digest, or still stale; existing
fixture path; wrong fixture parent length or foreign parent; ambiguous
engine inventory or malformed identities;
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
manifest SHAs; GO receipt SHA and reservation id; #544 proof receipt SHA
(task, commit, execution and reviewer run ids); the
candidate and input pins; exact owned identities (fixture path, session,
scoped project, container IDs); statuses; count results only; and a cleanup
inventory with preserved volumes. Row values, vault/ciphertext/env content,
credentials and raw secret-bearing child output are never recorded. The
boundary battery writes only disposable fixture metadata under the same
scratch root and removes it before exiting.

## Future activation requirements

This task delivers packet source and fixture verification only. Installation
is not part of it. A changed product candidate or Acow/bundle input needs an
explicit reviewed re-pin — including a relevant input drift, a declared
include-list change, or a bundle lock outside the approved transition; a
reviewed re-pin can also tighten the approved bundle revision prefix to a
full 40-hex commit once that identity is known. Exactly one retained Queue
reviewer owns the PR; no extra review or native-reader agents or workspaces.
Hosted CI is not a task gate. Future proof-only activation binds this
reviewed packet commit and hashes, the actual selected-input receipt at the
selected current head, and the actual before/after bundle identities, with
separate exact install/consumer GOs.
