# Distribution Preflight Summary Output

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/distribution-channels.md`

## Scope

- Add machine-readable summary output support to distribution preflight command.
- Update distribution docs to use the summary-output flag.

## Changes

- Updated script:
  - `scripts/check-distribution-preflight.sh`
    - added `--output <path>`
    - writes `TAG`, `DOCS_STATUS`, `METADATA_STATUS`, `SMOKE_STATUS`
- Updated docs:
  - `docs/guides/024-ci-and-automation-recipes.md`
  - `docs/guides/044-distribution-first-publish-execution-runbook.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/check-distribution-preflight.sh && tmp="$(mktemp -d)" && ./scripts/check-distribution-preflight.sh --tag v0.1.0 --output "$tmp"/preflight.env && grep -q '^TAG=v0.1.0$' "$tmp"/preflight.env && grep -q '^DOCS_STATUS=ok$' "$tmp"/preflight.env && grep -q '^METADATA_STATUS=ok$' "$tmp"/preflight.env && grep -q '^SMOKE_STATUS=ok$' "$tmp"/preflight.env`
  - result: pass

## Outcomes

- Preflight status can now be consumed by CI/report automation without parsing console logs.
- Distribution runbook examples now include persisted preflight evidence output.

## Risks / Follow-ups

- Summary format is key-value env style; downstream consumers should tolerate additive keys.
- Real publish-cycle evidence still required for acceptance closure.

## Next Batch Recommendation

- Execute first real release-tag artifact flow and include preflight summary + closeout report artifacts in acceptance-closeout evidence.
