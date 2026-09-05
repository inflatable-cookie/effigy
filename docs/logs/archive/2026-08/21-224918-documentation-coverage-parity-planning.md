# Documentation Coverage Parity Planning

Status: complete
Created: 2026-08-21
Roadmap: g08.034
Batch: documentation-coverage-planning

## Summary

- Operator intent opened a bounded whole-repository documentation coverage
  lane after the managed-runtime follow-up exposed uneven skill and built-in
  discovery.
- Strict spec `107`, roadmap `g08.034`, and cards `1086`–`1087` now define the
  audit, repair, recurrence, validation, and closeout runway.
- Historical logs and archived planning remain evidence rather than rewrite
  targets. Runtime behavior, release work, and workflows remain out of scope.

## Changes

- promoted a source-to-docs evidence matrix as the audit method
- required all verified in-scope gaps to be fixed or explicitly blocked
- seeded the audit with headless managed mode, managed ordering/readiness,
  optional forced secrets, workspace ownership, and non-console exec
- split implementation into one inventory/repair card and one guard/closeout
  card for a serial worker lane

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Movement: baseline feature-local docs checks -> current repository-wide,
  evidence-backed docs parity runway
- Remaining gap: execute cards `1086` and `1087` in the worker PR

## Validation Performed

- `effigy tasks`: passed; docs and full QA selectors are available
- `effigy doctor`: passed with no errors; existing graph/god-file warnings are
  unrelated to this lane
- `effigy qa:docs`: passed
- `effigy docs check workflow-paths`: passed
- `git diff --check`: passed

## Risks

- "whole repo" can invite unbounded prose churn; spec `107` limits the sweep to
  current public behavior and active documentation surfaces
- prose completeness is not fully mechanical; recurrence tests must protect
  stable relationships without creating another source of truth

## Next Task

Dispatch one serial implementation worker with cards `1086` and `1087` after
the planning base and worker handoff are committed and pushed.
