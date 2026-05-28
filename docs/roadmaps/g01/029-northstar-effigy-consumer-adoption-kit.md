# 029 - Northstar + Effigy Consumer Adoption Kit

Generation: `g01`

Status: Complete
Owner: Platform
Created: 2026-03-12
Depends on: 013, 015, 026, 027, 028

## Vision Alignment

This roadmap turns the Effigy repo's current doctrine into a reusable consumer
adoption kit so an agent can be told "use Northstar and Effigy" and convert
that phrase into a concrete repo operating model.

That operating model should mean:

- create the expected documentation skeleton
- define the vision, long-term goals, and roadmaps in the right shapes
- wire Effigy tasks and validation around those docs
- establish changelog and release strategy
- give agents one short contract they can follow consistently

The goal is not to force every consumer repo to look exactly like Effigy. The
goal is to make the shared semantics portable:

- Northstar defines the documentation and planning contract
- Effigy owns the executable validation and operator surface
- agent skills teach how to apply both together

## Primary Tags

- `OPERATE`
- `CONTRACT`
- `ROUTE`
- `MAINT`

## Target Envelope

- Effigy ships a documented, reusable consumer-repo contract for Northstar +
  Effigy adoption.
- Agents can apply that contract through a dedicated skill package rather than
  relying on repo-specific folklore.
- Consumer repos get a starter documentation skeleton, agent contract,
  changelog/release baseline, and Effigy validation bundle.
- Effigy can validate the contract through built-ins and task composition
  rather than only prose guidance.
- At least one consumer repo proves the contract works outside the Effigy repo.

## Vision Target Delta

- Moved from `Effigy and Northstar doctrine mostly proven only inside the
  Effigy repo` toward `portable ecosystem contract with reusable scaffolding,
  validation, and agent behavior across consumer repos`.

## Source of Truth

This roadmap is based on the consumer-repo scan:

- [`../../logs/2026-03/12-135650-consumer-adoption-landscape-scan.md`](../../logs/2026-03/12-135650-consumer-adoption-landscape-scan.md)
- [`../../guides/056-northstar-effigy-consumer-repo-contract.md`](../../guides/056-northstar-effigy-consumer-repo-contract.md)
- [`../../logs/2026-03/12-141200-pilot-repo-a-consumer-contract-gap-assessment.md`](../../logs/2026-03/12-141200-pilot-repo-a-consumer-contract-gap-assessment.md)
- [`../../logs/2026-03/12-155600-pilot-repo-b-workspace-docs-authority-pilot.md`](../../logs/2026-03/12-155600-pilot-repo-b-workspace-docs-authority-pilot.md)
- [`../../logs/2026-03/12-184800-pilot-repo-d-released-surface-pilot.md`](../../logs/2026-03/12-184800-pilot-repo-d-released-surface-pilot.md)
- [`../../logs/2026-03/12-190515-pilot-repo-e-released-surface-pilot.md`](../../logs/2026-03/12-190515-pilot-repo-e-released-surface-pilot.md)
- [`../../logs/2026-03/12-193800-pilot-repo-c-released-surface-pilot.md`](../../logs/2026-03/12-193800-pilot-repo-c-released-surface-pilot.md)
- [`../../logs/2026-03/12-210000-workspace-docs-authority-cohort-example-site-underlay-reference.md`](../../logs/2026-03/12-210000-workspace-docs-authority-cohort-example-site-underlay-reference.md)
- [`../../logs/2026-03/12-212500-songsprout-trellis-authority-only-pilot.md`](../../logs/2026-03/12-212500-songsprout-trellis-authority-only-pilot.md)
- [`../../logs/2026-03/12-214500-songsprout-root-delegation-follow-up.md`](../../logs/2026-03/12-214500-songsprout-root-delegation-follow-up.md)
- [`../../logs/2026-03/12-220500-consumer-adoption-closeout-matrix.md`](../../logs/2026-03/12-220500-consumer-adoption-closeout-matrix.md)
- [`../../logs/2026-03/12-223500-product-boundary-and-verify-install-ssh-closeout.md`](../../logs/2026-03/12-223500-product-boundary-and-verify-install-ssh-closeout.md)
- [`../../logs/2026-03/12-225500-starter-docs-policy-bundle-proof.md`](../../logs/2026-03/12-225500-starter-docs-policy-bundle-proof.md)
- [`../../logs/2026-03/12-233000-contract-drift-path-check-layer.md`](../../logs/2026-03/12-233000-contract-drift-path-check-layer.md)
- [`../../logs/2026-03/12-235500-workspace-bundle-proof-and-bootstrap-boundary.md`](../../logs/2026-03/12-235500-workspace-bundle-proof-and-bootstrap-boundary.md)
- [`../../logs/2026-03/12-235900-source-of-truth-consolidation.md`](../../logs/2026-03/12-235900-source-of-truth-consolidation.md)

Key scan findings:

- `15/16` scanned repos already have `effigy.toml`
- `16/16` have `AGENTS.md`
- only `1/16` currently has `qa:docs`, `qa:json`, `[release]`, and
  `[docs_policy]`
- `13/16` AGENTS files still teach `--repo .`
- `9/16` already have `docs/vision` and `docs/roadmaps`, but those structures
  are not yet standardized or self-validating across the ecosystem

## Design Rules

### 1. Contract first, skill second, product third

Do not jump straight to more built-ins. First define the minimum reusable repo
contract. The skill and product surface should then implement and enforce that
contract rather than inventing it ad hoc.

### 2. Standardize semantics, not exact prose

Consumer repos can vary in naming and domain language, but the following should
be standardized:

- required document classes
- required section headings/metadata
- agent execution flow
- changelog/release expectations
- Effigy validation and QA entrypoints

### 3. Keep repo-specific doctrine declarative

Effigy should provide reusable engines and starter task composition. It should
not hardcode every consumer repo's vision vocabulary or file inventory.

### 4. Use real pilots before broad claims

Do not declare the adoption kit "done" based only on the Effigy repo. At least
one consumer app repo and one shared-foundation repo should prove the model.

## Wave 1 - Minimum Repo Contract

Define the minimum reusable Northstar + Effigy consumer contract.

Contract scope:

- `AGENTS.md` contract
- `effigy.toml` baseline task surface
- docs skeleton
- changelog baseline
- release baseline
- validation task bundle

Minimum expected surfaces:

```text
AGENTS.md
effigy.toml
CHANGELOG.md
docs/README.md
docs/vision/README.md
docs/roadmaps/README.md
docs/logs/README.md
```

Minimum semantics:

- `effigy tasks`
- `effigy doctor` or `effigy health`
- `effigy test --plan`
- `qa`
- `qa:docs`
- `qa:northstar` or equivalent repo-owned doctrine bundle

Current pilot boundary:

- `pilot-repo-a` proved the single-repo contract in native current-Effigy mode
- `pilot-repo-b` proved that a workspace container can stay thin while a nested
  docs-authority repo carries the real Northstar contract
- `pilot-repo-d` proved that released `effigy v0.2.6` is enough for native
  consumer-side `docs_policy`, `qa:docs`, and `qa:northstar` on a real
  non-Effigy repo
- `pilot-repo-e` proved that the same released surface coexists cleanly with a
  deeper existing repo validation lane, including a successful `effigy validate`
  pass after contract adoption
- `pilot-repo-c` proved that the same released surface also works on a
  research-heavy repo with a large existing docs tree and a retained
  repo-specific docs contract script, once the native docs lane surfaced and
  cleared real backlog link debt
- `example-site` and `underlay-reference` proved that thin workspace roots
  with dedicated docs-authority catalogs can standardize on root-level
  `qa:docs` / `qa:northstar` orchestration plus native authority-level docs
  policy on released `0.2.6`
- `songsprout` now matches that same workspace-container contract end to end:
  root-level docs orchestration plus native authority-level docs policy in
  `trellis`
- the contract can no longer be documented only as a single-repo shape
- the remaining rollout state is now explicit:
  - fully adopted consumer repos: `pilot-repo-a`, `pilot-repo-b`, `underlay`,
    `example-app`, `pilot-repo-d`, `pilot-repo-e`, `pilot-repo-c`, `example-site`,
    `underlay-reference`, `songsprout`
  - ready but deferred until calmer worktrees: `finch`, `loophole`
  - needs Effigy adoption first: `pug`
  - intentionally different from the consumer contract: `nucleus`
- remaining open: decide exactly where release posture belongs for split
  workspace/doc-authority projects, finish the validation/productization
  boundary, and close the post-release `verify-install` SSH-remote gap

Tasks:

- [x] Define the minimum consumer repo contract as an Effigy-owned reference
- [x] Decide which files are mandatory vs recommended
- [x] Define the minimum required section/heading set for vision, roadmaps,
      logs, and docs indexes
- [x] Define the minimum AGENTS contract for "use Northstar and Effigy"
- [x] Define changelog and release minimum bar for consumer repos
- [ ] Decide which parts belong in Effigy docs vs Northstar docs

Acceptance:

- a fresh agent can read one contract and know what to create
- a maintainer can review a repo against the contract without guessing
- the contract covers both single-repo and workspace-container adoption modes

## Wave 2 - Skill and Template Bundle

Create the reusable agent package that applies the contract.

Target artifact:

- `northstar-effigy` skill

Skill responsibilities:

- inspect repo state
- scaffold missing contract surfaces
- create starter docs skeleton
- add or normalize `effigy.toml`
- create `CHANGELOG.md`
- add release baseline
- add validation tasks
- leave a next roadmap/log action in the correct format

Expected bundled resources:

- `SKILL.md`
- `references/repo-contract.md`
- `references/docs-skeleton.md`
- `references/changelog-release.md`
- `references/agent-contract.md`
- `assets/templates/...`
- optional scaffolding scripts

Tasks:

- [x] Write the skill trigger language so phrases like "use Northstar and
      Effigy" activate the right workflow
- [x] Create the skill skeleton and bundled references
- [x] Add starter templates for `AGENTS.md`, `effigy.toml`, docs indexes,
      changelog, and release config
- [x] Make the templates branch correctly between `released Effigy surface`
      and `future native docs/release surface`
- [x] Add example consumer-repo transformation flow to the references
- [x] Validate the skill against Effigy as the first dogfood target
- [ ] Extend the skill guidance to choose correctly between single-repo and
      workspace-container docs-authority adoption

Acceptance:

- the skill can produce a coherent starter contract without handholding
- the generated repo state matches Wave 1 contract expectations
- the skill does not force a root-level docs/release pattern onto a
  workspace-container repo that already has a nested docs authority

## Wave 3 - Effigy Validation Bundle

Make the contract enforceable through Effigy-native checks plus task
composition.

Target command/task direction:

```text
effigy docs check-headings
effigy docs check-index
effigy docs check-next-action
effigy docs check-forbidden
effigy qa:docs
effigy qa:northstar
effigy release status --check-gates
```

Possible additions:

- consumer-oriented docs-policy starter config
- built-in or task-composed repo contract checks
- starter `qa:northstar` bundle for headings/indexes/next-actions/forbidden
  defaults

Tasks:

- [x] Define the starter `qa:northstar` task bundle shape
- [x] Decide whether new built-ins are required or task composition is enough
- [x] Close the remaining released-surface gap for consumer repos: docs
      validation now works on released `0.2.6`, and release-install
      verification now handles SSH-style remotes outside Effigy's own repo
- [x] Package a starter `[docs_policy]` consumer config where appropriate
- [x] Add explicit validation rules for agent-contract drift and docs skeleton
      drift
- [x] Ensure the validation bundle works without Effigy-repo-specific file
      assumptions

Acceptance:

- a consumer repo can fail fast when the Northstar + Effigy contract drifts
- adoption is not dependent on manual doc review alone

Current decision:

- the starter `qa:northstar` bundle should be task-composed from existing
  Effigy-native validators:
  `docs check-index`, `docs check-next-action`, `docs check-headings`, and
  `docs check-forbidden`
- the starter native consumer docs-policy bundle is now explicit:
  `[docs_policy.indexes.vision]` + `[docs_policy.next_actions.vision]` plus a
  repo-owned `docs/policy/vision-next-task-verbs.txt`
- that starter bundle is proven by a neutral fixture test instead of only by
  Effigy's own docs tree or migrated consumer repos
- one additional generic built-in was justified for contract hardening:
  `effigy docs check-paths`, which covers repo/docs spine presence checks more
  cleanly than substring-based heuristics
- the finished starter bundle is now also proven in a thin workspace root with
  a nested docs-authority fixture, so Wave 3 no longer depends on Effigy-shaped
  repo assumptions
- repo shape choice, starter file creation, and concrete heading inventories
  remain skill/template work
- Effigy product scope should stay focused on reusable validation engines and
  release/runtime surfaces

## Wave 4 - Consumer Pilot Rollout

Prove the contract in real consuming repos.

Recommended pilot order from the scan:

1. `pilot-repo-a`
2. `pilot-repo-b`
3. `underlay`
4. `example-app`
5. `pilot-repo-d`
6. `pilot-repo-e`

Rationale:

- `pilot-repo-a` already has `docs/vision`, `docs/roadmaps`, `docs/guides`, and a
  simple local Effigy surface, but does not yet carry the full
  changelog/release/validation doctrine
- `pilot-repo-b` already has `docs/vision`, `docs/roadmaps`, and a docs authority
  surface, but is more complex because it is a workspace-scale environment
- `underlay` tests the model on a shared foundation repo instead of only an
  app repo
- `example-app` tests workspace-scale orchestration and multi-repo agent
  semantics after the contract is proven in simpler targets, especially around
  a separate planning authority repo (`ledger`) and staged release posture
- `pilot-repo-d` proves that the released `0.2.6` binary, not just the dev build,
  can carry native docs-policy validation in a real repo with an existing
  Northstar docs spine and changelog
- `pilot-repo-e` proves that the same released surface works in a repo with an
  existing deeper validation lane and does not require special shell fallback
  once the docs contract is normalized

Tasks:

- [x] Apply the contract manually to `pilot-repo-a` and record the released-surface
      gap
- [ ] Re-run `pilot-repo-a` with the reusable skill once Wave 2 exists
- [x] Apply the contract manually to `pilot-repo-b` and record the
      workspace-container docs-authority mode
- [x] Apply the revised contract to `underlay`
- [x] Record friction and missing-product gaps
- [x] Apply the revised contract to `example-app`
- [x] Apply the released `0.2.6` contract to `pilot-repo-d`
- [x] Apply the released `0.2.6` contract to `pilot-repo-e`
- [x] Classify the remaining untouched repos by migration worthiness
- [ ] Capture which parts should remain skill-level versus become Effigy-native

Acceptance:

- at least one consumer repo reaches contract compliance with low manual patching
- pilot notes identify product gaps clearly instead of leaving them implicit
- pilot evidence clearly separates `works today on released Effigy` from
  `needs new product surface`
- pilot evidence covers both app repos and shared foundation repos

Current closeout matrix:

- fully adopted consumer repos:
  `pilot-repo-a`, `pilot-repo-b`, `underlay`, `example-app`, `pilot-repo-d`, `pilot-repo-e`,
  `pilot-repo-c`, `example-site`, `underlay-reference`, `songsprout`
- source-of-truth repos, not consumer rollout targets:
  `effigy`, `northstar`
- ready for full contract, but defer for now:
  `finch`, `loophole`
- needs Effigy adoption first:
  `pug`
- intentionally different or low-value for this contract:
  `nucleus`

Rollout consequence:

- the broad consumer sweep is effectively complete
- remaining repo work is targeted follow-up, not another open-ended migration
  batch
- the next milestone should focus on reusable validation, starter bundles, and
  product-boundary decisions

## Wave 5 - Productization Boundary

Decide which adoption-kit parts should become first-class Effigy features.

Candidate product surfaces:

- `effigy init --northstar` or equivalent scaffold path — **shipped** as
  `effigy init <name>` by `g02.021` (complete). The unified init surface
  now emits named starters from `crates/effigy-catalog/starters/<name>/`
  with `--dry-run` / `--force` / `--json` and `--list [--json]`; multi-file
  starters (e.g. `underlay`) emit nested file trees plus optional
  post-emission guidance via the `effigy.init.v1` payload's `files[]` and
  `guidance` fields. The Northstar starter itself is now also shipped as
  a pure content slot at `crates/effigy-catalog/starters/northstar/`,
  emitting the single-repo consumer contract (README, AGENTS, CHANGELOG,
  docs spine, starter `[docs_policy]`, `qa:northstar` bundle, and vision
  next-task verb list) through the same loader — no command work was
  required.
- reusable consumer `[docs_policy]` starter blocks
- explicit repo-contract validation command family
- release/changelog starter generation

Tasks:

- [ ] Review pilot friction logs
- [ ] Promote only the stable, reusable parts into Effigy
- [ ] Keep repo-specific doctrine in templates/skills rather than product code
- [ ] Publish the final adoption guidance for consumer repos

Acceptance:

- Effigy product scope stays coherent
- the skill remains useful even after productizing the highest-value pieces

## Open Questions

- Should Northstar own the canonical consumer repo contract, with Effigy
  implementing it, or should Effigy own the contract and link back to Northstar
  doctrine pages?
- How much of the docs skeleton should be required in small repos versus
  generated only on demand?
- Release and changelog posture should be staged behind maturity for thin
  workspace containers and docs-authority repos; keep it mandatory only for
  repos that are actually preparing to ship releases.
- Is `effigy doctor` or `effigy health` the right default second step across
  consumer repos, or should the contract allow both?
- How should the skill decide between `released surface` and `future native
  surface` without teaching agents to emit unsupported manifest keys?

## Expected Outcome

- "Use Northstar and Effigy" becomes a concrete, portable repo contract instead
  of a repo-local phrase
- agents get one reusable skill that can scaffold and normalize consumer repos
- consumer repos gain a standard docs/planning/release baseline with Effigy
  validation behind it
- future product work is driven by proven adoption friction rather than
  speculation

## Validation Evidence

- consumer scan log:
  [`../../logs/2026-03/12-135650-consumer-adoption-landscape-scan.md`](../../logs/2026-03/12-135650-consumer-adoption-landscape-scan.md)
- `pilot-repo-a` gap assessment:
  [`../../logs/2026-03/12-141200-pilot-repo-a-consumer-contract-gap-assessment.md`](../../logs/2026-03/12-141200-pilot-repo-a-consumer-contract-gap-assessment.md)
- `pilot-repo-a` Wave 1 pilot and released-surface gap:
  [`../../logs/2026-03/12-142509-pilot-repo-a-wave1-pilot-and-released-surface-gap.md`](../../logs/2026-03/12-142509-pilot-repo-a-wave1-pilot-and-released-surface-gap.md)
- `pilot-repo-b` workspace + docs-authority pilot:
  [`../../logs/2026-03/12-155600-pilot-repo-b-workspace-docs-authority-pilot.md`](../../logs/2026-03/12-155600-pilot-repo-b-workspace-docs-authority-pilot.md)
- `underlay` single-repo foundation pilot:
  [`../../logs/2026-03/12-163200-underlay-single-repo-pilot.md`](../../logs/2026-03/12-163200-underlay-single-repo-pilot.md)
- `example-app` workspace + ledger-authority pilot:
  [`../../logs/2026-03/12-174500-example-app-workspace-ledger-authority-pilot.md`](../../logs/2026-03/12-174500-example-app-workspace-ledger-authority-pilot.md)
- `pilot-repo-d` released-surface pilot:
  [`../../logs/2026-03/12-184800-pilot-repo-d-released-surface-pilot.md`](../../logs/2026-03/12-184800-pilot-repo-d-released-surface-pilot.md)
- `pilot-repo-e` released-surface pilot:
  [`../../logs/2026-03/12-190515-pilot-repo-e-released-surface-pilot.md`](../../logs/2026-03/12-190515-pilot-repo-e-released-surface-pilot.md)
- rollout closeout matrix:
  [`../../logs/2026-03/12-220500-consumer-adoption-closeout-matrix.md`](../../logs/2026-03/12-220500-consumer-adoption-closeout-matrix.md)
- product boundary + verify-install SSH closeout:
  [`../../logs/2026-03/12-223500-product-boundary-and-verify-install-ssh-closeout.md`](../../logs/2026-03/12-223500-product-boundary-and-verify-install-ssh-closeout.md)
- starter docs-policy bundle proof:
  [`../../logs/2026-03/12-225500-starter-docs-policy-bundle-proof.md`](../../logs/2026-03/12-225500-starter-docs-policy-bundle-proof.md)
- contract-drift path-check layer:
  [`../../logs/2026-03/12-233000-contract-drift-path-check-layer.md`](../../logs/2026-03/12-233000-contract-drift-path-check-layer.md)
- workspace bundle proof + bootstrap boundary:
  [`../../logs/2026-03/12-235500-workspace-bundle-proof-and-bootstrap-boundary.md`](../../logs/2026-03/12-235500-workspace-bundle-proof-and-bootstrap-boundary.md)
- source-of-truth consolidation:
  [`../../logs/2026-03/12-235900-source-of-truth-consolidation.md`](../../logs/2026-03/12-235900-source-of-truth-consolidation.md)

## Next Task

Wave 3 is complete and the Effigy-side consumer-adoption boundary is now
closed. Use the external sweep handoff in
[`docs/handoffs/g01-029-consumer-adoption-sweep.md`](../../handoffs/g01-029-consumer-adoption-sweep.md)
for any follow-up repo cohort check, and only reopen Effigy-side product work
if that sweep shows repeated pain the current skill/template layer cannot
cover.
