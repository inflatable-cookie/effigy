# 002 Refocus Matrix v1

Status: Draft
Purpose: map Effigy's current documentation tracks to blueprint ideals and define keep/change/defer actions for alignment.

## 1. Ideal Tags

- `ROUTE`: deterministic selector resolution, catalog targeting, deferral behavior.
- `CONTRACT`: JSON envelope/payload stability, schema governance, machine-consumer reliability.
- `OPERATE`: operator ergonomics, observability, troubleshooting flow, workflow speed.
- `MAINT`: modularity, boundary clarity, refactor safety, docs/process hygiene.
- `RELEASE`: distribution reliability, gate repeatability, upgrade/rollback confidence.

## 2. Current Track Mapping

| Documentation Track | Primary Tags | Keep | Change | Defer |
| --- | --- | --- | --- | --- |
| Architecture (`docs/architecture/*`) | ROUTE, MAINT | Clear layer framing and module inventory | Add explicit vision-tag metadata and target envelopes | Deep per-module internals not tied to roadmap movement |
| Core routing/doctor/test guides (`016`-`020`) | ROUTE, OPERATE, CONTRACT | Strong command-shape and troubleshooting orientation | Add measurable target envelopes and vision-delta notes | Broad UX rewrite outside command behavior |
| Command/reference guides (`021`-`027`) | OPERATE, CONTRACT | Practical examples and canonical term usage | Add explicit links to blueprint ideals and policy deltas | New scenario expansion before baseline alignment is complete |
| Roadmap set (`001`-`012`) | MAINT, ROUTE, CONTRACT, OPERATE | Structured goals/non-goals/acceptance format | Add `Vision Alignment`, `Primary Tags`, `Target Envelope`, `Vision Target Delta` | New roadmap creation not mapped to vision targets |
| Reports (`docs/reports/*`) | CONTRACT, RELEASE, OPERATE | Strong validation command evidence | Standardize "Vision Target Delta" section across report families | Legacy report backfill beyond active release windows |
| Backlog roadmaps (`roadmap/backlog/*`) | RELEASE, MAINT | Clear promotion rules and staged planning | Attach target-envelope expectations before promotion | Additional backlog threads not yet tied to blueprint ideals |
| Contract index/examples (`docs/contracts`, guides `017`/`026`) | CONTRACT, RELEASE | Canonical envelope and schema indexing | Add explicit contract-governance vision tags and drift checks | Schema expansion without concrete command surfaces |

## 3. Documentation Realignment Targets

1. Every roadmap file includes vision tags and one explicit target envelope.
2. Every high-traffic guide includes a short "Vision Alignment" section.
3. Every validation/release report includes a "Vision Target Delta" section.
4. Contract docs and payload examples include drift-check ownership and update triggers.
5. Backlog items define promotion criteria in terms of target movement.

## 4. Realignment Priorities (Recommended)

1. Patch roadmap files `001` to `012` with vision tags and target envelopes.
2. Patch guide files `016` to `026` with concise vision alignment sections.
3. Patch report template guidance (reports index + authoring docs) with target-delta requirement.
4. Keep implementation checklists and closeout evidence in reports history, not the vision strategy folder.

## 5. Acceptance Signals for Batch 1

1. Reviewers can identify a roadmap/guide's target movement in under 30 seconds.
2. New reports include explicit movement against one or more vision tags.
3. Contract docs point to one canonical drift policy and owner trigger.
4. No conflicting terminology appears between blueprint docs and core guides.

## Next Task

Update the current track mapping so it reflects active Effigy priorities and adjusted tag emphasis where needed.
