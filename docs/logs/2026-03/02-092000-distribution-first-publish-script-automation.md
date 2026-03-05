# Distribution First-Publish Script Automation

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/distribution-channels.md`

## Scope

- Add an executable script to run the first-publish distribution matrix as one command.
- Wire the script into runbook and CI/automation docs.

## Changes

- Added script:
  - `scripts/check-distribution-first-publish.sh`
- Updated runbook:
  - `docs/guides/044-distribution-first-publish-execution-runbook.md`
- Updated CI automation recipes index:
  - `docs/guides/024-ci-and-automation-recipes.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/check-distribution-first-publish.sh && ./scripts/check-distribution-first-publish.sh --help`
  - result: pass

## Outcomes

- First-publish matrix now has a single executable entrypoint.
- Tag install, crates.io install, and optional Homebrew checks are orchestrated with consistent check logging.

## Risks / Follow-ups

- Script execution still requires a real release tag and published crates.io version.
- Homebrew validation depends on local/CI availability of `brew` and tap readiness.

## Next Batch Recommendation

- Execute script against first real release tag and publish acceptance-closeout evidence report with command outputs.
