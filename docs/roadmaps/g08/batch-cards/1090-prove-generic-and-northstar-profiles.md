# 1090 - Prove Generic And Northstar Profiles

Roadmap: [`../035-repository-defined-documentation-graph.md`](../035-repository-defined-documentation-graph.md)
Architecture: [`../../../architecture/024-repository-defined-documentation-graph.md`](../../../architecture/024-repository-defined-documentation-graph.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/041-documentation-graph-profile-contract.md`](../../../contracts/041-documentation-graph-profile-contract.md)
Spec: [`../../../specs/108-documentation-graph-profiles-strict-lane.md`](../../../specs/108-documentation-graph-profiles-strict-lane.md)
Predecessor: [`1089`](./1089-add-bounded-documentation-context-query.md)

Status: Ready
Owner: cross-repo proof, starter adoption, and lane closeout
Created: 2026-08-29
Ready after: card `1089` closeout proves the public query contract
Ready since: 2026-08-31, on card `1089` evidence
[`../../../logs/2026-08/31-181957-documentation-context-1089.md`](../../../logs/2026-08/31-181957-documentation-context-1089.md)

## Purpose

Prove that the feature is repository-neutral, publish Northstar as one committed
profile, measure retrieval quality, and close the lane.

## Owner And Seam

Generic fixtures and runtime tests stay below adoption assets. The Northstar
starter and guides may provide profile content, but tests must prove the
consumer manifest remains the only runtime authority when no skill directory is
available.

## Work

- keep one non-Northstar fixture with arbitrary roots, kinds, fields, statuses,
  and relation names
- add the Northstar profile to the shipped consumer starter
- document that skills/init materialize a profile and later updates are explicit
- add example queries for architecture, contract, current roadmap, next task,
  and historical decision discovery
- run a predeclared Effigy benchmark corpus and record rank, context bytes, and
  current-versus-historical behavior
- prove the expected live authority appears within the top three and no related
  historical-only counterpart ranks above it
- run focused validation, docs QA, formatting, Clippy, and full Effigy QA
- update changelog, architecture/package ownership docs, cards, spec, roadmap,
  and front doors; archive spec `108` only after acceptance is complete

## Acceptance

- [ ] the generic fixture passes with no Northstar tokens in runtime logic
- [ ] the Northstar starter profile is valid and queryable after copying alone
- [ ] removing access to installed skills does not change runtime results
- [ ] benchmark evidence meets the top-three and currentness target
- [ ] adoption docs distinguish repository authority from template origin
- [ ] focused and full validation pass
- [ ] closeout leaves no stale ready card

## Validation

- focused manifest/codegraph/CLI/built-in/starter tests
- documentation links, generated reference, and command matrix checks
- benchmark replay on Effigy plus the generic fixture
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Close with one dated log containing the benchmark corpus and ranks, generic and
Northstar profile proof, installed-skill independence proof, validation output,
and residuals.

## Stop Conditions

Stop if the benchmark can pass only through Effigy/Northstar path hard-coding,
if starter updates become implicit runtime inheritance, or if full validation
exposes an unresolved behavior defect.

## Next Task

Close the lane and return evidence to the operator. Do not infer release work.
