# Release Checklist Artifact Flow Alignment

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/release-contract-v0.md`

## Scope

- Align release checklist and release contract docs with new artifact validation/report generation scripts.
- Ensure first-publish execution gate references concrete script commands.

## Changes

- Updated release checklist template:
  - `docs/guides/014-release-checklist-template.md`
- Updated release contract backlog note:
  - `docs/roadmaps/backlog/release-contract-v0.md`
- Updated distribution first-publish gate:
  - `docs/roadmaps/backlog/distribution-channels.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only`
  - result: pass

## Outcomes

- Release operations now include explicit artifact capture, validation, and closeout report generation steps.
- First-publish completion criteria are tied to executable commands, not only narrative guidance.

## Risks / Follow-ups

- Checklist still depends on real publish-cycle execution to satisfy acceptance criteria.
- If script names/flags change, checklist and contract docs must be updated in the same batch.

## Next Batch Recommendation

- Run first real release tag through the full artifact workflow and use generated closeout report to reconcile remaining distribution acceptance criteria.
