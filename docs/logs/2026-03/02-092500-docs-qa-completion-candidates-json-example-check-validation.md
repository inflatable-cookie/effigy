# Docs QA Completion Candidates JSON Example Check Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Automate docs QA coverage for completion-candidates JSON example telemetry keys.

## Changes
- Added `scripts/check-doc-json-examples.sh`:
  - validates section `13) Completion Candidates` in `026-json-payload-examples.md`
  - requires warm-hit and miss JSON example blocks
  - asserts required cache telemetry keys in both blocks
  - enforces warm-hit in block #1 and miss in block #2
- Wired script into:
  - `scripts/check-quality-gates.sh --docs-only`
  - `scripts/check-prepush-ci.sh`
- Updated `029-docs-qa-checklist-and-validation.md` with checker behavior and CI wording.

## Validation
- command: `./scripts/check-doc-json-examples.sh`
  - result: pass
- command: `./scripts/check-quality-gates.sh --docs-only`
  - result: pass

## Risks / Follow-ups
- Checker is section-label and block-order aware; intentional docs structure changes will require updating script expectations.

## Next
- Extend docs QA automation with a lightweight check that every `docs/logs/*.md` file is indexed in `docs/logs/README.md`.
