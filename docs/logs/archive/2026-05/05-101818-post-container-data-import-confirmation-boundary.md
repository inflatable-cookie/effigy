# Post Container Data Import Confirmation Boundary

Date: 2026-05-05
Roadmap: `g03.027`
Batch Card: `367-decide-post-container-data-import-confirmation-boundary.md`

## Summary

The lane should not move straight into broad `unlock` confirmation. The next
step is a prerequisite card that promotes the shared prompt policy out of
runner-only code so `effigy-builtin` can use it without duplicating rules.

## Decision

Next ready card:

- `368-promote-prompt-policy-for-builtin-unlock.md`

Broad `unlock` confirmation should later guard:

- `unlock --all`
- `unlock workspace`
- multi-scope unlocks
- explicit `shared:<name>` unlocks

Single `task:<name>` and single `profile:<task>/<profile>` unlocks stay
unprompted for now because they are precise recovery actions.

## Vision Target Delta

Primary tags: `CONTRACT`, `OPERATE`, `MAINT`

Baseline: the lane named broad `unlock` confirmation as the next exit-condition
surface, but `unlock` lives in `effigy-builtin` while prompt policy lived in
runner-only code.

Current state: the next card is a bounded prompt-policy promotion prerequisite,
with the guarded unlock shapes and `--yes` bypass decision recorded.

Remaining open: implement the policy promotion, then implement broad `unlock`
confirmation.
