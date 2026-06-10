# Post Distribution Policy Widening Slice Decision

Date: 2026-04-14
Roadmap: `g02.005`
Spec: `docs/specs/005-optional-distribution-surface-strict-lane.md`
Batch Card: `docs/roadmaps/g02/batch-cards/102-decide-post-distribution-policy-widening-slice.md`

## Decision

Run one bounded consumer-proof adoption batch next.

## Why

The current optional `[distribution]` contract is now broad enough to justify
one real cross-repo proof:

- package identity is manifest-driven
- publish identity is manifest-driven
- preflight task names are manifest-driven
- metadata requirements are manifest-driven
- closeout defaults are manifest-driven

The remaining Effigy-shaped assumptions are now narrow enough that they should
be surfaced by one real consumer proof instead of guessed at through more
internal widening.

## Guardrails

- keep the proof bounded to one repo
- do not claim universal distribution-channel flexibility yet
- do not edit consumer workflows without explicit human approval
- treat any remaining mismatch as product evidence, not as pressure to
  over-generalize the contract in advance

## Outcome

The next valid move in `g02.005` is no longer internal design widening. It is
one real consumer proof of the optional distribution surface.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `RELEASE`
- Moved: decision posture shifted from internal widening to real consumer
  validation
- Remaining open: one bounded consumer proof and the explicit product gaps it
  exposes

## Next Task

Execute `docs/roadmaps/g02/batch-cards/103-implement-consumer-proof-of-optional-distribution-surface.md`
to prove the optional distribution surface in one real consumer repo.
