# Dependabot Cargo Frontier

Status: planning complete
Created: 2026-09-24
Roadmap: g10.014–g10.015
Batch: post-0.13.0-dependency-maintenance

## Summary

- The operator asked to resolve ten open Dependabot PRs and selected consolidated reviewed batch PRs. All ten target versions are still absent from the 0.13.0-era `Cargo.lock`; most bot heads predate the release.
- Six compatible lockfile-only updates (#79, #80, #96, #97, #99, #100) form `g10.014`. The direct `argon2`, `tree-sitter`, and `tabled` upgrades (#81, #98, #102) plus the overlapping transitive `tree-sitter-language` update (#101) form `g10.015`.
- Both batches own `Cargo.lock`, so `g10.015` has a serial Queue dependency on the terminal closeout of `g10.014`. Each replacement PR needs independent exact-head review and current-base CI. Named bot PRs close only after their replacement merge.
- Reconciled stale release and lifecycle next actions in the active roadmaps, generation index, contracts, specs, logs, and vision front doors. The 0.13.0 release is already published and install-verified; these tasks do not mutate it.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`.
- Movement: ten overlapping stale bot heads with no current executable lane -> two bounded dependency tasks with serial lockfile ownership and explicit compatibility oracles.
- Remaining gap: reviewed implementation, bot-PR disposition, and Queue closeout.

## Validation Performed

- `effigy qa:docs`: passed, including links, indexes, headings, workflow paths, and vision next actions.
- `git diff --check` and Northstar currentness audit: passed; zero currentness violations.

## Risks

- `argon2` changes the vault KDF dependency. A same-version encrypt/decrypt test does not prove existing vaults remain readable; `g10.015` requires a baseline vector or vault fixture.
- `tree-sitter` and `tabled` can change graph facts or plain table bytes without a compile failure; focused behavioral baselines are part of the review oracle.
- Source PRs may update while workers run. The aggregate PR and disposition evidence must name exact source heads and merge commits.

## Next Task

Submit `g10.014` through Queue from pushed planning. After its terminal closeout, dispatch the already approved `g10.015` dependency lane.
