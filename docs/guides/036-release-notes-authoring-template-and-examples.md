# 036 - Release Notes Authoring Template and Examples

Use this guide to write consistent release notes for Effigy milestones and incremental releases.

## 1) Required Structure

Every release note should include these sections in this order:

1. `Summary`
2. `User-Visible Changes`
3. `Migration Notes`
4. `Validation`
5. `Rollback Notes`
6. `Compatibility`

## 2) Template

Copy this into `docs/reports/YYYY-MM-DD-<topic>-release-note.md`:

```md
# <Release Note Title>

Date: YYYY-MM-DD
Owner: <team/person>
Related roadmap: <id/title>
Release: <version or milestone label>

## Summary
- What shipped and why it matters.

## User-Visible Changes
- Command/behavior changes visible to operators.
- New flags, output fields, or workflow changes.

## Migration Notes
- Required user actions (if any).
- Before/after command examples.
- Safe fallback path.

## Validation
- command: `...`
  - result: ...
- command: `...`
  - result: ...

## Rollback Notes
- Previous known-good version/tag.
- Rollback command path.
- Data/config side effects to watch.

## Compatibility
- Backward-compatible behavior retained.
- Known limitations or edge conditions.

## Next
- Follow-up tasks or monitoring checks.
```

## 3) Example A - Command Behavior Release

```md
# Doctor Explain Mode Release Note

Date: 2026-02-28
Owner: effigy maintainers
Related roadmap: 009 - doctor health consolidation
Release: milestone m1

## Summary
- Added doctor explain mode for selector reasoning and deferral diagnostics.

## User-Visible Changes
- New invocation: `effigy doctor <task> -- <args>`.
- JSON payload schema includes `effigy.doctor.explain.v1` in envelope result.

## Migration Notes
- Before: `effigy tasks --resolve <selector>` only.
- After: use `effigy doctor <selector> -- <args>` when you need richer reasoning.
- Fallback: `effigy tasks --resolve <selector>` remains available.

## Validation
- command: `effigy doctor api/build -- --watch`
  - result: explain output with selection and deferral reasoning
- command: `effigy --json doctor api/build -- --watch`
  - result: valid `effigy.command.v1` envelope with `effigy.doctor.explain.v1` payload

## Rollback Notes
- Roll back to previous tag if explain mode blocks automation unexpectedly.
- Continue using `effigy tasks --resolve` for routing diagnostics.

## Compatibility
- Existing `effigy doctor` health mode unchanged.
- Existing JSON envelope contract retained.

## Next
- Expand explain docs with additional ambiguous-selector scenarios.
```

## 4) Example B - CI/Contract Release

```md
# JSON Contracts CI Policy Release Note

Date: 2026-02-28
Owner: effigy maintainers
Related roadmap: 008 - universal JSON command coverage
Release: milestone m1

## Summary
- Standardized PR/full-run JSON contract checks and selection artifact validation.

## User-Visible Changes
- CI now runs docs-link and JSON contract quality gates in workflow.
- Selection artifact validation is enforced in CI.

## Migration Notes
- Replace ad-hoc JSON checks with:
  - `./scripts/check-json-contracts-ci.sh`
  - `./scripts/validate-json-contract-selection-artifact.sh ./json-contracts-selected.json`
- Keep `./scripts/check-json-contracts.sh --fast --print-selected=json` for local preflight.

## Validation
- command: `./scripts/check-json-contracts-ci.sh`
  - result: PR-aware contract selection and validation passes
- command: `./scripts/check-selection-artifact-validator-smoke.sh`
  - result: invalid payload fixtures rejected, valid fixture accepted

## Rollback Notes
- Revert workflow changes in `.github/workflows/json-contracts.yml` if pipeline is blocked.
- Keep local script-based checks as temporary fallback.

## Compatibility
- Command envelope contract remains `effigy.command.v1`.
- Existing command payload schemas remain valid unless explicitly changed.

## Next
- Add changed-only contract summary to release artifacts.
```

## 5) Authoring Rules

- Prefer concrete commands over prose-only validation claims.
- Keep migration notes explicit even when no migration is required (`None required`).
- Use exact schema IDs in JSON-related notes.
- Keep rollback instructions executable and short.

## 6) Where to Link Release Notes

For each new release note:
1. add file under `docs/reports/` with date-first naming
2. add link to `docs/reports/README.md` under recent release notes
3. reference from relevant guide(s) when behavior is user-impacting

## Related Guides

- `014-release-checklist-template.md`
- `024-ci-and-automation-recipes.md`
- `029-docs-qa-checklist-and-validation.md`
- `033-style-and-terminology-guide.md`
