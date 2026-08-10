# Explicit Catalog Membership Closeout

Status: complete
Created: 2026-08-10
Roadmap: g08.028
Batch: card-1075-explicit-catalog-membership-closeout

## Summary

- Published explicit root-owned membership across README, guides, cookbook,
  troubleshooting, command matrix, both bundled skill copies, starter
  manifests, and changelog migration guidance.
- Proved the current repo runs as one root-only catalog without discovery
  ignore configuration.
- Proved nested, sibling, symlinked, named-mount, inline-mount, and ordinary
  mount boundaries through routing, runner, and test-plan fixtures.
- Closed roadmap `g08.028`, cards `1072` through `1075`, and strict spec `101`.
- Archived spec `101` and removed every stale ready-card pointer.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `MAINT`, `OPERATE`
- Movement: baseline `catalog membership is inferred from filesystem shape and
  mount side effects` -> current `every non-root catalog is intentionally
  declared by the composed root manifest`
- Remaining gap: None in this lane. Contract `037` owns the durable behavior.

## Consumer-Shape Evidence

- Root-only self-host:
  - `cargo run --quiet --bin effigy -- doctor`: pass, 16 ok, 2 warnings, 0
    errors
  - `cargo run --quiet --bin effigy -- tasks`: one effective catalog
  - `cargo run --quiet --bin effigy -- test --plan`: one target
- `cargo test -p effigy-routing`: pass, 6 tests
  - one fixture includes declared nested and sibling members, inline system
    and workspace members, ordinary structured and legacy mounts, and an
    undeclared nested manifest
  - symlink/physical declarations deduplicate canonically
- `cargo test -p effigy --lib catalog_membership_tests`: pass, 13 tests
  - covers symlink routing, named mounts, inline workspace mounts, undeclared
    artifacts, alias conflicts, prefix routing, and root anchoring
- explicit built-in test-plan fixture: pass
  - named and inline catalog mounts included
  - ordinary mount and undeclared manifest excluded

## Migration Surface

- `[catalog.members]` is the primary root-owned declaration.
- `{ member = "..." }` reuses a named member for membership and runtime mount
  ownership without repeating the source path.
- `{ source = "...", catalog = true }` declares an inline mounted member.
- Ordinary structured mounts and legacy string mounts never imply membership.
- `[catalog.discovery]` and `effigy catalog cache clear` fail with direct
  migration guidance.

## Validation Performed

- `cargo run --quiet --bin effigy -- qa:ci:fast`
  - result: pass; 1,637 tests, released-surface checks, full JSON selection,
    and JSON artifact validation
- `cargo run --quiet --bin effigy -- qa`
  - result: pass; full test, docs, and JSON task sequence
- `cargo test --test cli_output_tests`
  - result: pass, 237 tests; 1 ignored
- focused Clippy across all touched product crates
  - result: pass
- `cargo fmt --all -- --check`
  - result: pass
- `git diff --check`
  - result: pass
- live-guidance exact-token audit
  - result: no discovery-era membership or cache guidance remains outside
    historical changelog/planning evidence and intentional removal diagnostics

## Boundaries

No release command ran and no workflow changed. Selector precedence and generic
catalog JSON structures remain stable.

## Next Task

Await the next operator-approved g08 scope. Do not infer release work or a
generation rollover.
