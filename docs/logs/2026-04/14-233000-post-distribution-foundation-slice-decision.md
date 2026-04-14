# Post-Distribution-Foundation Slice Decision

Date: 2026-04-14
Roadmap: `g02.005`
Spec: `docs/specs/005-optional-distribution-surface-strict-lane.md`
Card: `100-decide-post-distribution-foundation-slice.md`

## Summary

Chose one more internal widening batch before any consumer proof.

The manifest-driven foundation is real, but it currently covers only:

- package identity
- preflight task names
- metadata file requirements

That is enough to start shaping the product surface, but not enough to make a
consumer proof honest yet because the publish/summary/closeout path still
contains too much Effigy-shaped policy.

## Decision

Do one more internal `g02.005` batch next:

- widen manifest-driven policy across `distribution first-publish`
- widen manifest-driven policy across `distribution write-summary`
- widen manifest-driven policy across `distribution generate-closeout`

Do not jump to a consumer proof until those commands are less Effigy-specific.

## Why Not Consumer Proof Yet

- `first-publish` still carries channel/default assumptions that read as
  Effigy self-hosting, not generic cross-repo contract
- `write-summary` still reflects the same baked-in identity defaults
- `generate-closeout` still embeds Effigy-centric closeout language and
  roadmap framing

A consumer proof now would validate a surface that is only partially optional.

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved from: first manifest-driven distribution foundation shipped
- moved to: one explicit internal widening batch before any consumer proof
- remains open:
  - publish/summary/closeout manifest policy widening
  - bounded consumer proof after that widening
  - eventual workflow-bound glibc guard cutover when workflow edits are in scope

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `101-implement-distribution-policy-widening-for-publish-and-closeout.md`
to widen manifest-driven distribution policy across the remaining
publish/summary/closeout path before any consumer-proof batch.
