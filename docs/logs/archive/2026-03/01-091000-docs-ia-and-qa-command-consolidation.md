# Docs IA and QA Command Consolidation Checkpoint

Date: 2026-03-01
Owner: Platform
Related roadmap: docs IA / QA hardening tranche

## Scope
- Consolidate contributor QA commands behind one repo-native entrypoint.
- Reduce docs navigation noise and clarify source-of-truth guide ownership.
- Keep CI and local workflows aligned on the same quality gate commands.

## Changes
- Added canonical quality gate wrapper:
  - `scripts/check-quality-gates.sh`
- Added cargo aliases for one-command usage:
  - `cargo qa`, `cargo qa-docs`, `cargo qa-json`, `cargo qa-json-ci`
- Added helper binary to support cargo aliases reliably:
  - `src/bin/effigy-qa.rs`
- Updated CI workflow to use the canonical wrapper for docs and JSON jobs.
- Consolidated docs QA guidance to `029-docs-qa-checklist-and-validation.md`.
- Trimmed duplicate QA procedure content in `024` and `030` and replaced with pointers to `029`.
- Reorganized docs indexes to separate core guides from operations guides (`029`-`032`, legacy flow map).

## Validation
- command: `cargo qa`
  - result: pass
- command: `cargo qa-docs`
  - result: pass
- command: `cargo qa-json`
  - result: pass

## Risks / Follow-ups
- Cargo aliases rely on `.cargo/config.toml`; contributors running commands outside repo root should use script fallback.
- Keep `029` as the canonical QA guide and avoid reintroducing duplicated checklists elsewhere.

## Next
- Add a short “Contributor Commands” section to root README near installation that lists `cargo qa*` aliases and when to use each.
