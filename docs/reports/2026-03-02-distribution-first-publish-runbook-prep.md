# Distribution First-Publish Runbook Prep

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmap/backlog/distribution-channels.md`

## Scope

- Prepare the next executable batch for closing remaining Distribution acceptance criteria once a release tag exists.
- Add a concrete runbook and wire it into docs navigation and roadmap gate notes.

## Changes

- Added runbook:
  - `docs/guides/044-distribution-first-publish-execution-runbook.md`
- Updated guides navigation:
  - `docs/guides/README.md`
- Added execution-gate note in distribution backlog:
  - `docs/roadmap/backlog/distribution-channels.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only`
  - result: pass

## Outcomes

- Remaining acceptance work now has explicit executable steps and evidence requirements.
- Backlog completion now includes a concrete first-publish gate instead of implicit follow-up.

## Risks / Follow-ups

- Runbook execution is blocked until a real release tag exists.
- crates.io and Homebrew execution evidence still depend on publish-cycle timing.

## Next Batch Recommendation

- Execute **first-publish execution batch** when tag `vX.Y.Z` is available:
  - run channel matrix commands from guide 044
  - publish one acceptance-closeout report
  - mark remaining acceptance criteria complete (or document failures)
