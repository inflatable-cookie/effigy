# 101 Implement Distribution Policy Widening For Publish And Closeout

Status: complete
Updated: 2026-04-14
Roadmap: `g02.005`
Spec: `docs/specs/archive/005-optional-distribution-surface-strict-lane.md`

## Objective

Widen the optional distribution manifest contract across the remaining
publish/summary/closeout commands so a later consumer proof exercises a
meaningfully repo-configurable surface instead of a mostly Effigy-shaped one.

## In Scope

- make `distribution first-publish` read more manifest-driven policy where it
  still assumes Effigy defaults
- make `distribution write-summary` and `distribution generate-closeout`
  consume the same optional policy boundary
- add the next minimal `[distribution]` config needed for publish/closeout
  identity and artifact behavior
- update docs so the widened contract remains clear and optional

## Out Of Scope

- editing `.github/workflows/` without explicit human approval
- forcing another repo to adopt the distribution surface during this batch
- solving every distribution channel variation in one slice

## Acceptance Criteria

- publish/summary/closeout commands are less Effigy-specific in policy
- the widened manifest contract is documented
- the lane is ready for either a consumer proof or one final policy decision

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

After this batch, decide whether the widened optional distribution surface is
now honest enough for one consumer-proof adoption batch.
