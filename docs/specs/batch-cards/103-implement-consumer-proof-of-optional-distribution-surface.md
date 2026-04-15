# 103 Implement Consumer Proof Of Optional Distribution Surface

Status: complete
Updated: 2026-04-15
Roadmap: `g02.005`
Spec: `docs/specs/005-optional-distribution-surface-strict-lane.md`

## Objective

Prove the optional `[distribution]` surface in one real consumer repo so the
manifest-driven distribution contract is exercised outside Effigy's own
self-hosting defaults.

## In Scope

- choose one safe consumer repo with a real binary/distribution context
- add only the minimal `[distribution]` manifest config needed for that repo
- exercise at least one manifest-driven distribution command path honestly
- record what worked, what still felt Effigy-shaped, and whether the consumer
  proof changed the product boundary

## Out Of Scope

- forcing full release-orchestration adoption in the consumer repo
- editing `.github/workflows/` without explicit human approval
- broad channel abstraction beyond the current optional contract

## Acceptance Criteria

- one real repo uses the optional distribution surface
- the proof identifies any remaining product gaps concretely
- the lane has an honest next boundary after the proof

## Validation

- repo-specific distribution command proof in the chosen consumer repo
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute the follow-up ready card to widen the named consumer-proof gaps in
`distribution validate-metadata` and `distribution first-publish` without
reopening the whole distribution lane.
