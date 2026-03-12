# Acowtancy Workspace + Ledger Authority Pilot

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: acowtancy-workspace-ledger-authority

## Summary

Applied the Northstar + Effigy consumer contract to `acowtancy` as the fourth
pilot and confirmed that it is best modeled as:

- a thin workspace orchestration root
- a nested planning/docs authority repo (`ledger`)
- separate releasable code repos that should own release posture in their own
  repo contracts

This closes the main open question from the earlier workspace pilots:
changelog and release posture should not be forced onto a workspace container
root or a docs-only authority repo at initial adoption time.

## Changes

- normalized the `acowtancy` workspace root so it teaches `effigy tasks`,
  `effigy doctor`, `effigy test --plan`, and direct `effigy <task>` usage
  without redundant `--repo .`
- updated the workspace root `effigy.toml` so `qa:docs` checks the root
  contract surfaces and `qa:northstar` routes through `ledger`
- normalized active agent instructions in `cream`, `dairy`, `farmyard`,
  `cattle-grid`, and `froyo` so they no longer teach current-directory
  `--repo .`
- upgraded `ledger` into the explicit Northstar authority for the workspace by
  adding native docs-policy config, `qa:docs`, `qa:northstar`, a next-task verb
  allowlist, and clearer README/AGENTS guidance
- normalized `ledger` readmes so the planning spine is expressed with explicit
  headings and next-task structure instead of looser label text

## Validation

Validated directly against the native built-in docs surface with
`/Users/betterthanclay/.local/bin/effigy`:

- `effigy docs check-links` in `acowtancy/ledger`
- `effigy docs check-index --policy-index vision` in `acowtancy/ledger`
- `effigy docs check-next-action --policy vision` in `acowtancy/ledger`
- `effigy docs check-headings` across `ledger/vision/README.md`,
  `ledger/vision/001-acowtancy-platform-vision.md`,
  `ledger/roadmaps/README.md`,
  `ledger/roadmaps/g01/README.md`,
  `ledger/roadmaps/g02/README.md`, and
  `ledger/roadmaps/generation-index.md`
- `effigy docs check-forbidden AGENTS.md README.md package.json --forbid '--repo .'`
  at the `acowtancy` workspace root

Two environment constraints were surfaced during verification:

- the shell in this session still resolves bare `effigy` to Homebrew `0.2.4`
  before `~/.local/bin`, even though the newer binary exists locally
- live workspace locks in `acowtancy` and `ledger` prevented running the new
  `qa:docs` / `qa:northstar` task aliases end-to-end without interrupting
  existing user sessions

Those are environment/runtime issues, not contract-shape failures.

## Decision

For workspace-container adoption:

- the workspace root must carry orchestration guidance and routing tasks
- the docs-authority repo must carry the real Northstar docs skeleton and docs
  QA
- release posture is maturity-gated and belongs only on repos that actually
  ship releases

That means `CHANGELOG.md` and `[release]` stay mandatory for single-repo apps
and releasable repos, but become conditional for workspace containers and
docs-only authority repos.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Movement: baseline `workspace-container mode proven only in simpler
  docs-authority form` -> current `workspace orchestration root + separate
  Northstar authority + maturity-gated release posture proven on a real
  multi-repo app workspace`
- Remaining gap: `PATH` and subprocess binary-resolution drift still need a
  cleaner operator story if native consumer docs-policy tasks are to be fully
  reliable without environment cleanup`

## Next Task

Tighten the reusable skill and contract references so workspace-container mode
explicitly distinguishes orchestration root, docs authority, and releasable
subrepos, then decide whether Effigy should productize any of that split beyond
task composition and docs-policy starters.
