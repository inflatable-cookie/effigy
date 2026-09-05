# 1063 - Prove Bun Closure Drift And Repair

Roadmap: [`../023-dependency-link-portfolio-proof-and-closeout.md`](../023-dependency-link-portfolio-proof-and-closeout.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Ready after: completed card `1062`

## Purpose

Prove save-less Bun linking, full package closure, install drift, peer evidence,
and idempotent repair with a portfolio-shaped published-package fixture.

## Owner And Seam

`effigy-deps` remains the Bun registration, symlink, observation, and ownership
authority. The proof uses real Bun commands against isolated HOME and repo
fixtures.

## Work

- build a multi-package local library and registry-style consumer fixture with
  direct and transitive matching packages
- prove `deps link bun` registers and links the full closure without
  `package.json` or lockfile changes
- run a real `bun install` to remove consumer symlinks and prove status/doctor
  report complete or partial drift with contract severity
- re-link idempotently and prove the full closure returns
- create a duplicate peer-resolution shape and preserve both physical paths in
  status/doctor evidence
- unlink and prove owned registration release plus immutable files

## Guardrails

- isolated temporary HOME only
- explicit save-less Bun operations; never `--save`
- no real portfolio TS publication claim
- no operator-guide or suite-closeout work

## Acceptance

- [x] full direct/transitive package closure links locally
- [x] manifest and lockfile bytes remain unchanged
- [x] `bun install` drift is detected without mutation by status/doctor
- [x] re-link repairs drift idempotently
- [x] duplicate peer paths are exact in text and JSON
- [x] unlink releases only Effigy-owned unshared registrations

## Evidence

- [`Bun closure, drift, and repair proof`](../../../logs/archive/2026-08/05-230446-bun-closure-drift-repair-proof.md)
- Bun `1.3.14` linked the direct `@effigy-proof/protocol` package and
  transitive `@effigy-proof/core` package through explicit `--no-save`
- a real `bun install` produced a one-of-two partial closure; status and doctor
  reported an error and managed re-link restored the full closure
- text and JSON peer diagnostics preserved the exact consumer and local Svelte
  paths
- unlink removed both consumer links, released both unshared owned
  registrations, and retained exact manifest and lock hashes

## Validation

- real Bun integration fixture with isolated HOME
- focused Bun status/doctor parity tests
- `cargo test -p effigy-deps --test bun_link`
- `effigy qa:ci:fast`
- `git diff --check`

## Stop Conditions

Stop and replan if the supported Bun release changes save-less behavior,
install drift cannot be reproduced deterministically, or safe proof requires a
published portfolio package that does not yet exist.

## Next Task

Execute ready operator guidance and suite closeout card `1064`.
