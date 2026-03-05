# 012 Vision Tag and Terminology Canon v1

Status: Draft
Owner: Docs Owners + Platform Maintainers
Purpose: establish canonical tag and language definitions used across vision, roadmap, guides, and logs.

## 1. Canonical Vision Tags

1. `ROUTE`: deterministic selector resolution, catalog targeting, and explainability of routing outcomes.
2. `CONTRACT`: machine-readable envelope/payload stability, schema governance, and compatibility discipline.
3. `OPERATE`: operator ergonomics, failure actionability, and workflow responsiveness.
4. `MAINT`: modular boundaries, refactor safety, and sustainable complexity posture.
5. `RELEASE`: gate repeatability, distribution confidence, and rollback readiness.

## 2. Canonical Terms

1. `selector`: the task identifier provided by the caller (prefixed, relative, or unprefixed).
2. `routing`: deterministic resolution process mapping selector + context to concrete execution target.
3. `deferral`: explicit handoff path when Effigy intentionally does not execute directly.
4. `target envelope`: measurable quality range expected for a vision capability.
5. `vision target delta`: movement statement showing progress/regression against a target envelope.
6. `exception`: time-bounded, approved deviation from a vision constraint (`005`).
7. `governance review`: cadence artifact that records metrics, risks, exceptions, and actions (`009`).

## 3. Language Rules

1. Prefer canonical terms over ad-hoc synonyms in strategy and operational docs.
2. Keep tag use specific: do not attach multiple tags unless each is materially impacted.
3. Use "target envelope" for desired ranges; reserve "SLO" for measurable service-level thresholds (`003`).
4. Use "exception" only for approved deviations with owner and expiry.

## 4. Drift Patterns to Avoid

1. Using `ROUTE` to describe generic runtime behavior unrelated to selector resolution.
2. Using `CONTRACT` to mean any docs update without schema or machine-interface impact.
3. Mixing `actionable diagnostics` with generic verbose output claims.
4. Calling unapproved shortcuts "temporary" without exception records.

## 5. Maintenance Rules

1. Canon updates should be deliberate and recorded in a single PR.
2. New terms should include definition, scope, and related tags.
3. Deprecated terms should map to replacement terms to avoid ambiguity.

## Next Task

Create a compact alias/deprecation table for common non-canonical terms currently found in docs and reviews.
