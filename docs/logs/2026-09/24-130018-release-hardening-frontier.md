# Release Hardening Frontier

Status: complete
Created: 2026-09-24
Roadmap: g10.011–g10.013
Batch: pre-0.13.0-release-hardening

## Summary

- The operator approved all four findings from the release-readiness audit as pre-release work and approved Queue dispatch. The confirmed release target is 0.13.0.
- Promoted committed docs-source consent/provenance and source handle/status identity together into `g10.011`; doctor subprocess deadlines into `g10.012`; and nested skill passthrough into `g10.013`.
- The operator settled the only open handle choice: duplicate repository basenames across portfolio directories must fail with a clear error before querying either repository. Existing `invalid` status represents non-missing directory read failures.
- Removed the four fully promoted triage notes in this batch. The remaining triage candidates are unscheduled.

## Vision Target Delta

- Primary tags: `RELEASE`, `CONTRACT`, `OPERATE`, `ROUTE`.
- Movement: four non-executable audit findings -> three ready, independently owned repair tasks with adversarial oracles and Queue handoffs.
- Remaining gap: reviewed implementation and closeout, exact-head CI, all release gates, then separately authorized release mutation.

## Validation Performed

- `effigy qa:docs`: passed, including links, JSON examples, indexes, headings,
  forbidden text, workflow paths, and vision next-action checks.
- `git diff --check`: passed.
- Semantic review: task ownership, operator choices, concurrent siblings, release boundary, and front-door links checked before dispatch.

## Risks

- The tasks share `CHANGELOG.md`; Queue merges must serialize and preserve all entries.
- Prior `effigy release gates` stopped at the exact-head CI gate. This promotion does not satisfy that gate or authorize release preparation.

## Next Task

Submit `g10.011` through `g10.013` to Queue from the pushed integration commit. After all three reviewed closeouts, reassess 0.13.0 readiness.
