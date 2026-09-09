# Flattened Northstar Task Switchover

Status: complete
Created: 2026-09-09
Roadmap: none — all existing generations are closed
Task: none — lifecycle migration

## Safety checkpoint

- Repository: `/Users/tom/Dev/projects/effigy`
- Integration branch and head: `main` at
  `7412d9ab182cf1070ea00e280df9aa2a21dafb6c`, synchronized with `origin/main`
  and clean before mutation.
- Northstar Queue: the sole Effigy record,
  `c541743f-067d-4ec1-a9ee-32ac12ab0140`, is `done`; PR #95 is merged and its
  worker/reviewer workspace is archived.
- Paseo: no active Effigy worker, reviewer, or coordinator exists. This
  Chatterbox is the only running Effigy thread.
- Worktrees: the integration checkout is clean. The retained PR #60 worktree is
  clean, its branch matches origin, and PR #60 is merged.
- Open PRs #79–#81 and #96–#102 are Dependabot lanes with no Northstar planning
  path ownership.
- New old-format dispatch is suspended. No unfinished submitted record owns a
  path in the removal set.

## Historic-generation classification

All expanded generations are safely closed. Front doors say each is closed,
no task remains executable, no active spec or handoff exists, and retained
open commitments already have current homes.

| Generation | Source tree | Files | External Markdown linkers | Disposition |
| --- | --- | ---: | ---: | --- |
| `g01` | `16f8e9b1087cdb450bf4b6158306320331b6ef52` | 148 | 26 | compact to `archive/g01.md` |
| `g02` | `a182d6baadc872be0f4e91074c4342920da29775` | 333 | 46 | compact to `archive/g02.md` |
| `g03` | `3f75b602e02008952496dc5a687b0f2cf19b39e2` | 114 | 25 | compact to `archive/g03.md` |
| `g04` | `ba44cb8e901f7caf5ec1408cd3d21fb091ca49f0` | 229 | 97 | compact to `archive/g04.md` |
| `g05` | `bbd7aa9ece3d5088b78c5a72acf0d2890a2daec6` | 79 | 13 | compact to `archive/g05.md` |
| `g06` | `efacdd2cd12e1535804aeed2b3e44f6c662ee931` | 19 | 2 | compact to `archive/g06.md` |
| `g07` | `eff0d886d45506eac981e25708ba952fe42b50cd` | 163 | 67 | compact to `archive/g07.md` |
| `g08` | `d047d4911694d1b2a305af2a9309049961205c54` | 129 | 51 | compact to `archive/g08.md` |
| `g09` | `d2929e2b3ede1027540f9b9118e740517ee59fda` | 19 | 22 | compact to `archive/g09.md` |

## Preservation manifest

- Exact classified removal roots: `docs/roadmaps/g01/` through
  `docs/roadmaps/g09/`, including every nested `batch-cards/` and audit path.
- Unique generation intent, durable outcomes, limits, material release/PR/SHA
  evidence, current destinations, and successor are preserved in
  `docs/roadmaps/archive/g01.md` through `g09.md`.
- Current authority remains in `docs/architecture/`, `docs/contracts/`,
  `docs/guides/`, `docs/vision/`, and the active front doors. No durable rule is
  promoted from a generation because those current surfaces already own it.
- Open commitments remain reachable in `docs/roadmaps/backlog/`,
  `docs/research/carry-forward-intake.md`, vision artifacts `007` and `020`,
  contract `032`, and the three open `docs/triage/` notes.
- Every Markdown link that resolves below a classified removal root will point
  to that generation's roll-up. Historical card terminology remains unchanged.
- Git retains the full milestone, batch-card, audit, validation, and merge
  history at the source commit and tree objects above.

## Active-generation mapping

None. `g09` closed before the switchover, no `g10` generation exists, and no
active or ready executable unit survives. There is therefore no old active
milestone/card ID to map into a flattened task. The next generation will use
top-level `gNN.NNN` Northstar tasks from its first commit.

## Planned live-surface repair

- Replace milestone/batch-card doctrine with the generation-plus-task model.
- Install `docs/roadmaps/templates/task-template.md`.
- Update the repository-owned documentation graph from `ready-card` to
  top-level Northstar task classification and task relations.
- Point every current front door at the explicit no-active-generation state and
  operator-led Northstar Atlas decision.

## Completed repair

- Replaced the live milestone-plus-card doctrine with one generation roadmap
  and top-level `gNN.NNN` task contract.
- Added the repository and bundled-starter task templates.
- Updated the documentation graph profile, starter assertions, benchmark
  fixtures, docs QA headings, agent instructions, and current front doors.
- Rewrote current and historical Markdown links to the generation roll-ups.
- Removed only `docs/roadmaps/g01/` through `g09/`, the roots frozen above.
- Repeated the lifecycle inventory. Only the roadmap front door, generation
  index, backlog, task template, and nine non-procedural roll-ups remain. No
  active task, legacy nested card tree, or duplicate executable ID reappeared.

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`, `CONTRACT`
- Movement: nested milestone and batch-card planning authority -> one
  generation roadmap plus one top-level task per executable unit
- Remaining gap: operator choice of the next strategic runway through
  Northstar Atlas; no product frontier is approved by this migration

## Validation Performed

- `effigy qa:docs`
  - passed: links, examples, indexes, forbidden defaults, roadmap/task
    headings, workflow paths, and vision next-action checks
- `cargo test -p effigy-catalog`
  - passed: 232 unit and integration tests
- `cargo test --test cli_output_tests docs_context`
  - passed: 25 tests
- `cargo test -p effigy-rhai docs_context_benchmark`
  - passed: 5 tests after correcting the archived-log benchmark fixture path
- `cargo fmt --all -- --check`
  - passed
- `effigy test --plan`
  - passed: selected the workspace `cargo-nextest` suite
- `effigy perf:docs-context-benchmark`
  - passed: all predeclared documentation-context expectations held
- `git diff --check`
  - passed

## Next Task

Run Northstar Atlas with the operator. Normal dispatch stays paused because no
approved frontier exists; this migration does not open `g10` or authorize
product execution.
