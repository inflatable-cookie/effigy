# Consumer Adoption Closeout Matrix

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: consumer-adoption-closeout-matrix

## Summary

Completed the final landscape classification after the released `effigy v0.2.6`
consumer pilots.

The broad migration sweep is no longer an open-ended queue. The repos under
`~/Dev/projects` now fall into a small set of explicit
states:

- fully adopted on the Northstar + Effigy consumer contract
- structurally ready, but better deferred until calmer worktrees
- missing Effigy adoption, so not yet worth a full contract sweep
- intentionally different from the consumer-repo contract

This closes the ecosystem scan loop and turns roadmap `g01.029` from
`keep migrating repos` into `finish the reusable contract and productization
boundary`.

## Closeout Matrix

### Fully adopted consumer repos

These repos now prove the released `0.2.6` contract across the main adoption
shapes:

- `monkey`
- `compli-me`
- `underlay`
- `acowtancy`
- `signal`
- `convergence`
- `jetstream`
- `contact-patch`
- `underlay-reference`
- `songsprout`

Adoption shapes covered:

- single repo with native docs-policy
- workspace container with nested docs authority
- research-heavy repo with retained repo-specific docs checks
- thin workspace root plus nested docs-authority catalog

### Source-of-truth repos, not consumer rollout targets

- `effigy`
  - origin for the executable contract, validation surfaces, and roadmap
- `northstar`
  - origin for the documentation model, bundle, and portable skill

These remain part of the doctrine, but they are not consumer rollout targets in
the same sense as application or foundation repos.

### Ready for full contract, but defer for now

- `finch`
  - has a real docs authority at `docs/` plus root `effigy.toml`
  - still teaches stale current-repo `--repo .` defaults
  - currently has a large active worktree, so a migration batch would mix
    contract work with unrelated runtime/docs churn
- `loophole`
  - has the expected workspace-container shape with `chorus/` as the likely
    docs authority
  - still teaches stale root defaults and lacks native authority-level docs QA
  - root and nested repos are already dirty, so this is better handled as a
    deliberate follow-up rather than folded into the clean cohort

These are no longer unknowns. They are straightforward future migrations once
their working trees are calm enough for a clean contract batch.

### Needs Effigy adoption first

- `pug`
  - already has Northstar-style docs structure
  - does not yet have a root `effigy.toml`
  - should not be treated as a normal consumer-contract migration until Effigy
    itself is introduced as a repo-level execution surface

### Intentionally different or low-value for this contract

- `nucleus`
  - uses Effigy, but its content model is a coordination hub rather than a
    product repo with the standard `docs/vision`, `docs/roadmaps`, `docs/logs`
    spine
  - migrating it into the consumer app/foundation contract would add
    ceremony without improving clarity

## Validation Basis

The matrix is based on:

- the original landscape scan
- completed pilot batches across the adopted repos
- targeted shape checks for the untouched repos:
  - `finch`
  - `loophole`
  - `pug`
  - `nucleus`

Targeted checks confirmed:

- `finch` has `docs/vision`, `docs/roadmaps`, `docs/logs`, and a nested
  `docs/effigy.toml`, but also a large dirty worktree
- `loophole` has `chorus/` as a likely docs authority with `vision`,
  `roadmaps`, and `logs`, but both root and nested repos already have
  in-progress changes
- `pug` has the Northstar docs spine without a root Effigy manifest
- `nucleus` is clean, but its repo model is intentionally different from the
  standard consumer-docs contract

## Decision

The migration sweep is effectively complete for the repos that are both:

- good fits for the contract
- stable enough to update without colliding with unrelated active work

What remains is not another large repo sweep. What remains is:

- finish the reusable starter bundle and validation doctrine
- decide which pieces stay in the `northstar-effigy` skill
- promote only the stable reusable pieces into Effigy product surface
- optionally migrate `finch` and `loophole` later as calm-worktree follow-ups
- introduce Effigy to `pug` before attempting contract normalization

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Movement: baseline `consumer adoption roadmap still behaved like an open repo
  migration queue` -> current `consumer adoption is classified into completed
  rollout targets, deliberate deferrals, non-adopters, and intentional
  out-of-scope repos`
- Remaining gap: finish Wave 3 and Wave 5 product-boundary work, plus the
  `release verify-install` SSH-remote normalization gap

## Next Task

Use this matrix to close the sweep phase in roadmap `g01.029`: mark remaining
repo discovery/classification work complete, add the explicit adoption matrix
to the roadmap, and shift the next Effigy work to productization boundaries
instead of more ad hoc repo migrations.
