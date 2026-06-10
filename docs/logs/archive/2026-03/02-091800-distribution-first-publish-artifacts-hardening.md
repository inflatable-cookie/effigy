# Distribution First-Publish Artifacts Hardening

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/distribution-channels.md`

## Scope

- Harden first-publish helper script for deterministic per-step evidence capture.
- Remove weak output-redirection behavior that hid step output context.
- Update docs to require/encourage artifact-directory usage for closeout reporting.

## Changes

- Updated script:
  - `scripts/check-distribution-first-publish.sh`
    - added `--artifacts-dir <dir>` to preserve logs
    - added per-step log files with deterministic ordering
    - added failure tail output for fast triage
    - added tag-format validation
- Updated docs:
  - `docs/guides/044-distribution-first-publish-execution-runbook.md`
  - `docs/guides/024-ci-and-automation-recipes.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/check-distribution-first-publish.sh && ./scripts/check-distribution-first-publish.sh --help`
  - result: pass

## Outcomes

- First-publish execution can now generate attachable evidence logs in a single directory for release closeout reports.
- Failure handling is clearer due to automatic tail output from the failing step log.

## Risks / Follow-ups

- Real publish-cycle verification still requires an actual tag and published crate version.
- Homebrew checks remain environment-dependent (`brew` availability and tap state).

## Next Batch Recommendation

- Run `./scripts/check-distribution-first-publish.sh --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z` on the first real release and publish the acceptance-closeout report with attached log references.
