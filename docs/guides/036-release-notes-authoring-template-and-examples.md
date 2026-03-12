# 036 - Release Notes Authoring Template and Examples

Use this guide to write consistent release notes for Effigy milestones and incremental releases.

Use it when a behavior change is already shipped or about to ship and the
remaining job is to explain the change clearly to humans.

## Start Here

Work in this order:

1. extract the raw release body from the changelog
2. wrap it with summary, validation, rollback, and compatibility context
3. link the note from `docs/logs/README.md`

Start with:

```bash
effigy changelog extract CHANGELOG.md --version X.Y.Z
```

Treat that output as the baseline, not the finished note.

## 1) Required Structure

Every release note should include these sections in this order:

1. `Summary`
2. `User-Visible Changes`
3. `Vision Target Delta`
4. `Migration Notes`
5. `Validation`
6. `Rollback Notes`
7. `Compatibility`

## 2) Template

Copy this into `docs/logs/YYYY-MM/DD-HHMMSS-<topic>-release-note.md`:

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

## Vision Target Delta
- Primary tags: `ROUTE|CONTRACT|OPERATE|MAINT|RELEASE`
- Movement: baseline `...` -> current `...`
- Remaining gap: `...` (or `None`)

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

## Vision Target Delta
- Primary tags: `ROUTE`, `OPERATE`
- Movement: selector diagnosis moved from partial (`tasks --resolve` only) to explicit explain-mode diagnostics.
- Remaining gap: broader explain examples for nested/symlinked catalogs.

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

## Vision Target Delta
- Primary tags: `CONTRACT`, `RELEASE`
- Movement: contract validation moved from ad-hoc local checks to enforced CI policy gates.
- Remaining gap: include richer changed-contract summaries in release artifacts.

## Migration Notes
- Replace ad-hoc JSON checks with:
  - `effigy contracts check-json --repo . --full --print-selected=json`
  - `effigy contracts validate-selection --repo . --artifact ./json-contracts-selected.json`
- Keep `effigy contracts check-json --repo . --fast --print-selected=json` for local preflight.

## Validation
- command: `effigy contracts check-json --repo . --full --print-selected=json`
  - result: schema-index selection and validation passes
- command: `cargo test --test cli_output_tests cli_contracts_validate_selection_rejects_invalid_artifacts -- --nocapture`
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

- Generate the initial per-version note body from the changelog before editing:
  - `effigy changelog extract CHANGELOG.md --version X.Y.Z`
- Treat that extracted output as a baseline, not the final note. Add summary,
  validation, rollback notes, and compatibility context around it.
- Prefer concrete commands over prose-only validation claims.
- Keep migration notes explicit even when no migration is required (`None required`).
- Use exact schema IDs in JSON-related notes.
- Include measurable `Vision Target Delta` movement and remaining gaps.
- Keep rollback instructions executable and short.
- Keep historical references accurate when release notes describe past workflows; do not rewrite old evidence paths only for layout normalization.

## 6) Where to Link Release Notes

For each new release note:
1. add file under `docs/logs/YYYY-MM/` with `DD-HHMMSS-<topic>.md` naming
2. add link to `docs/logs/README.md` under recent release notes
3. reference from relevant guide(s) when behavior is user-impacting

## 7) Built-In Extraction Recipe

Use this when preparing release notes from the just-cut changelog section:

```bash
effigy changelog extract CHANGELOG.md --version X.Y.Z
```

Expected behavior:
- prints only the body for that version, not the outer `## [X.Y.Z] - DATE` heading
- preserves category headings and entries as release-note source material
- exits non-zero if the version is missing or empty

This is the same release-note extraction surface now used by Effigy's release
workflow and by human-authored release-note drafting. Treat the extracted body
as the machine-generated baseline, then add summary, validation, rollback
notes, and compatibility context before publishing the human-reviewed note.

## 8) Historical Workflow Reference Rule

- If a release note/log documents a workflow path that was correct at that time, keep that historical path as-is.
- For current operational guidance (outside historical logs), use the active
  workflow paths in `.github/workflows/*.yml`.
- Validation check `effigy docs check-workflow-paths --repo .` intentionally excludes `docs/logs/` to preserve historical evidence fidelity.

## Expected Outcome

- release notes are consistent, scannable, and directly actionable
- each note includes concrete validation and rollback steps
- release documentation stays connected to roadmap and guide updates

## Related Guides

- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`053-release-wrapper-retirement-record-template.md`](./053-release-wrapper-retirement-record-template.md)
- [`054-release-checkpoint-log-template.md`](./054-release-checkpoint-log-template.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`052-changelog-workflows-and-northstar-profile.md`](./052-changelog-workflows-and-northstar-profile.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)

## Next Step

After drafting a release note, run the release checklist in [`014-release-checklist-template.md`](./014-release-checklist-template.md) and attach validation outputs from the executed commands.
