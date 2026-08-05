# Dependency Health Status Parity

Status: complete
Created: 2026-08-05
Roadmap: g08.022
Batch: 1060

## Summary

- made Cargo and Bun link health one typed, read-only dependency-domain report
- added severity, exact evidence, remediation, and peer diagnostics to status
- kept text and `effigy.deps.status.v1` JSON on the same observation model

## Changes

- classify tracked or drifted Cargo config and active-link or unrelated
  lockfile drift, including explicit do-not-commit remediation
- distinguish Bun complete loss, partial closure, registration conflict,
  committed `link:` manifest/lock churn, and duplicate peer resolution
- report consumer roots, package closure, verification, finding severity,
  evidence paths, remediation, and peer paths in status text and JSON
- catch Cargo metadata and Bun peer/manifest inspection failures as typed
  health findings instead of mutating or hiding local state

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: baseline closure-only status -> current manager-neutral hygiene and
  remediation report suitable for direct doctor consumption
- Remaining gap: doctor adapter, severity mapping, and parity proof in `1061`

## Validation Performed

- `cargo test -p effigy-deps`: 67 unit, 2 real Bun integration, 3 Cargo
  integration, and doc tests passed
- focused runner status/CLI tests: 9 passed
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`: passed
- `cargo clippy --bin effigy --all-targets -- -D warnings`: passed
- `effigy qa:ci:fast`: 1,624 tests passed; 25 JSON contracts passed
- `effigy qa:docs`: links, examples, indexes, policy, and next-action checks
  passed

## Risks

- legacy binary `bun.lockb` cannot provide safe line-level saved-link evidence;
  status reports exact text-lock and manifest churn and leaves binary-only
  attribution to operation-time immutable snapshots
- doctor has not yet consumed the shared findings; that boundary is ready in
  card `1061`

## Next Task

- Execute ready card `1061` under `g08.022`.
