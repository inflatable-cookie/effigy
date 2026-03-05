# Distribution Closeout Report Generator

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/distribution-channels.md`

## Scope

- Add a script that generates a closeout checkpoint report from first-publish artifact logs.
- Wire the script into first-publish runbook and CI/automation recipes.

## Changes

- Added script:
  - `scripts/generate-distribution-closeout-report.sh`
- Updated docs:
  - `docs/guides/044-distribution-first-publish-execution-runbook.md`
  - `docs/guides/024-ci-and-automation-recipes.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/generate-distribution-closeout-report.sh && ./scripts/generate-distribution-closeout-report.sh --help`
  - result: pass
- command: `tmp="$(mktemp -d)" && touch "$tmp"/01-tag-install-validation.log "$tmp"/02-crates-io-install-validation-0-1-0.log "$tmp"/03-crates-io-binary-help.log "$tmp"/04-crates-io-binary-json-tasks.log && ./scripts/generate-distribution-closeout-report.sh --tag v0.1.0 --artifacts-dir "$tmp" --output "$tmp"/report.md && test -s "$tmp"/report.md`
  - result: pass

## Outcomes

- First-publish artifact logs can now be converted into a dated closeout report with a single command.
- Closeout reporting is standardized and less error-prone for release windows.

## Risks / Follow-ups

- Generated report quality still depends on complete artifacts from `check-distribution-first-publish.sh`.
- Homebrew evidence remains conditional on environment/tap availability during first publish.

## Next Batch Recommendation

- Run first real publish-cycle (`vX.Y.Z`) with artifacts enabled and generate the closeout report directly from those artifacts, then reconcile remaining acceptance criteria in `distribution-channels.md`.
