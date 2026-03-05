# Distribution Artifact Validator

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmap/backlog/distribution-channels.md`

## Scope

- Add a dedicated artifact completeness validator for first-publish evidence logs.
- Integrate validator into closeout report generation for fail-fast evidence gating.
- Update automation/runbook docs with explicit validator usage.

## Changes

- Added script:
  - `scripts/validate-distribution-artifacts.sh`
- Updated script:
  - `scripts/generate-distribution-closeout-report.sh`
    - now validates artifacts before generating report
    - supports `--expect-homebrew` for strict channel evidence requirements
- Updated docs:
  - `docs/guides/024-ci-and-automation-recipes.md`
  - `docs/guides/044-distribution-first-publish-execution-runbook.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/validate-distribution-artifacts.sh ./scripts/generate-distribution-closeout-report.sh`
  - result: pass
- command: `tmp="$(mktemp -d)" && touch "$tmp"/01-tag-install-validation.log "$tmp"/02-crates-io-install-validation-0-1-0.log "$tmp"/03-crates-io-binary-help.log "$tmp"/04-crates-io-binary-json-tasks.log && ./scripts/validate-distribution-artifacts.sh --artifacts-dir "$tmp"`
  - result: pass
- command: `tmp="$(mktemp -d)" && touch "$tmp"/01-tag-install-validation.log "$tmp"/02-crates-io-install-validation-0-1-0.log "$tmp"/03-crates-io-binary-help.log "$tmp"/04-crates-io-binary-json-tasks.log "$tmp"/05-homebrew-install.log "$tmp"/06-homebrew-binary-help.log "$tmp"/07-homebrew-binary-json-tasks.log "$tmp"/08-homebrew-upgrade.log && ./scripts/generate-distribution-closeout-report.sh --tag v0.1.0 --artifacts-dir "$tmp" --expect-homebrew --output "$tmp"/report.md && test -s "$tmp"/report.md`
  - result: pass

## Outcomes

- Closeout report generation now enforces baseline evidence completeness before creating report files.
- Homebrew evidence requirements can be toggled explicitly per release window.

## Risks / Follow-ups

- Validator is pattern-based and depends on current step naming conventions in first-publish script logs.
- If step labels change, validator patterns must be updated in the same batch.

## Next Batch Recommendation

- On first real release tag, run the full matrix with artifacts and strict Homebrew expectation, generate closeout report, and reconcile remaining acceptance criteria in `distribution-channels.md`.
