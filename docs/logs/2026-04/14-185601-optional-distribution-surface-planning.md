# Optional Distribution Surface Planning

Date: 2026-04-14
Roadmap: `g02.005`
Spec: `docs/specs/005-optional-distribution-surface-strict-lane.md`

## Summary

Paused the Rhai lane on a clean Effigy-internal boundary and opened a new lane
for optional cross-repo distribution support.

The decision is that Effigy's native distribution commands are now strong
enough to treat distribution as its own product surface rather than as one more
subproblem inside the scripting lane.

## Decision

- keep the shipped built-ins
- keep distribution optional
- move repo-specific policy behind manifest config
- document the distribution surface as a front door for adoption

## Why This Batch Exists

Effigy now has native commands for:

- glibc floor inspection
- first-publish orchestration
- artifact validation
- closeout generation
- summary writing

That is useful, but still partly Effigy-self-hosting in policy. Other repos
need a way to opt into the useful parts without inheriting a fixed release
model.

## Planned Contract Direction

The next implementation slice should define a minimal optional
`[distribution]` contract that can own:

- package identity
- preflight task chain
- enabled channels
- artifact expectations
- closeout defaults

The built-ins should then read manifest policy instead of relying on Effigy's
hardcoded release assumptions.

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved from: Effigy-native distribution cutover framed as the tail of the
  Rhai lane
- moved to: optional cross-repo distribution support as its own planning lane
- remains open:
  - minimal manifest-driven `[distribution]` implementation
  - command-by-command removal of remaining Effigy-self-hosting assumptions
  - deeper generic adoption documentation after the config lands

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `099-implement-manifest-driven-distribution-contract-foundation.md` to
land the minimal optional `[distribution]` contract and make at least one
current self-hosting-biased distribution command read manifest policy instead.
