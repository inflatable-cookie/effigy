# g08.018 - Local Dependency Management Suite

Status: Complete
Depends on: `g08.017`
Opened: 2026-08-05

## Goal

Add a package-manager-aware `effigy deps` domain that preserves pinned,
committed dependency sources while restoring local edit-in-place development
through reversible machine-local links.

The suite covers Cargo config-level patches and save-less Bun links. It treats
full dependency closure, local-state ownership, verification, drift, lockfile
hygiene, and JSON output as one cross-manager contract rather than separate
shell recipes.

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Target envelope: local dependency switching is one deterministic,
  machine-readable workflow across the portfolio's supported managers.
- Vision target delta: package-manager-specific local override ritual becomes
  an Effigy-owned plan/apply/verify/status domain.

## Sequence

1. [`g08.019`](./019-dependency-inventory-and-command-foundation.md) — shared
   inventory, desired state, command grammar, status, and JSON foundation.
2. [`g08.020`](./020-cargo-local-dependency-linking.md) — Cargo closure patches,
   nested workspaces, lock safety, and reversible verification.
3. [`g08.021`](./021-bun-local-dependency-linking.md) — save-less Bun
   registration/symlinks, drift repair, and peer dedupe evidence.
4. [`g08.022`](./022-dependency-link-doctor-and-hygiene.md) — doctor findings and
   do-not-commit hygiene across both mechanisms.
5. [`g08.023`](./023-dependency-link-portfolio-proof-and-closeout.md) — Signal
   consumer proof, Bun fixture proof, operator docs, and suite closeout.

## Governing Authority

- [`023-local-dependency-linking-architecture.md`](../../architecture/023-local-dependency-linking-architecture.md)
- [`034-local-dependency-linking-contract.md`](../../contracts/034-local-dependency-linking-contract.md)
- [`001-working-rules.md`](../../contracts/001-working-rules.md)

## Guardrails

- no `Cargo.toml` or `package.json` edits from link/unlink
- no Bun `--save`
- no partial closure links
- no destructive lockfile restore
- no overwrite of hand-managed Cargo patches or Bun registrations
- no package managers beyond Cargo and Bun in this tranche
- no speculative `deps` subcommands beyond status, link, and unlink
- no release execution or workflow edits

## Acceptance Criteria

- [x] every mutating manager milestone consumes the shared inventory and
      desired-state model
- [x] `effigy deps`, status, link, and unlink have text and standard-envelope
      JSON contracts
- [x] Cargo and Bun operations are idempotent, dry-runnable, and verified
- [x] doctor makes local link drift and do-not-commit state actionable
- [x] Cargo proof covers flat and nested-workspace consumers
- [x] Bun proof demonstrates zero manifest/lockfile churn and drift repair
- [x] operator docs separate committed dependency truth from machine-local link
      state

## Execution Posture

Completed under strict spec `099` through cards `1051` to `1064`.

## Next Task

Select the next substantial g08 scope separately. No release or generation
rollover is implied.
