# Prepared Source Drift Policy

Status: complete
Created: 2026-08-06
Roadmap: g08.026
Batch: prepared-source-drift-policy

## Summary

- Kept prepared-source drift non-overridable.
- Defined `--allow-stale` as an age-only acknowledgement in help, reviews, and
  release guidance.
- Added `suggested_actions` to JSON execute plans so automated callers receive
  the mandatory reprepare recovery already exposed by resume.
- Added CLI proof that `--allow-stale` cannot bypass HEAD and prepared-file
  drift.

## Decision

Age changes no source identity, so explicit acknowledgement is sufficient.
Branch, HEAD, or prepared-file drift changes the candidate whose gates produced
the prepared state. Execute must regenerate state and rerun preparation rather
than apply an override to mismatched gate evidence.

## Vision Target Delta

- Primary tags: `RELEASE`, `CONTRACT`, `OPERATE`
- Movement: baseline `one stale flag appeared to cover both age and source
  identity but could not recover source drift` -> current `age acknowledgement
  and mandatory source reprepare are distinct in CLI, JSON, and guidance`
- Remaining gap: full `0.9.1` candidate and consumer lockfile proof

## Validation Performed

- focused regression against pre-guidance behavior
  - result: failed because JSON execute plan had no `suggested_actions`
- `cargo test -q -p effigy-release`
  - result: 16 passed
- `cargo test -q --test cli_output_tests release`
  - result: 65 passed
- `cargo clippy -p effigy-release -p effigy --all-targets -- -D warnings`
  - result: pass
- `cargo fmt --all -- --check`
  - result: pass
- `git diff --check`
  - result: pass

## Boundaries

No release prepare, execute, tag, push, workflow edit, gate bypass, or
downstream mutation ran.

## Next Task

Execute card 1069: prove the patch-release candidate read-only.
