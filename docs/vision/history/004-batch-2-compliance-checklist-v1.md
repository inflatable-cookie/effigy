# 004 Batch 2 Compliance Checklist v1

Status: Complete
Date: 2026-03-05
Purpose: define the next compliance pass that moves Effigy's existing roadmap, guides, and reports into explicit vision alignment.

## 1. Scope

Batch 2 scope:

1. Annotate roadmap files `001` to `012` with:
- `Vision Alignment`
- `Primary Tags`
- `Target Envelope`
- `Vision Target Delta`

2. Annotate core guide files `016` to `026` with:
- concise `Vision Alignment` sections tied to `ROUTE/CONTRACT/OPERATE/MAINT/RELEASE`.

3. Standardize report conventions by requiring:
- `Vision Target Delta` in validation and release report templates/guidance.

4. Update backlog docs with:
- explicit promotion criteria tied to target movement.

## 2. Planned Compliance Table

| Artifact Group | Required additions | Pass condition |
| --- | --- | --- |
| `docs/roadmaps/*.md` | Vision metadata sections | All numbered roadmap files include all four sections |
| `docs/guides/016-026*.md` | Vision alignment notes | All targeted guides include tags + target movement intent |
| `docs/logs/README.md` and report authoring guidance | Vision target delta requirement | New report guidance explicitly enforces target-delta section |
| `docs/roadmaps/backlog/*.md` | Target-linked promotion criteria | Each backlog item includes measurable promotion signals |

## 3. Validation Plan

1. Run docs QA checks and link validation after edits.
2. Perform manual spot-check on at least:
- three roadmap files,
- three guides,
- three reports
to confirm metadata consistency.
3. Publish a dated closeout note summarizing pass/fail by artifact group.

## 4. Risks and Mitigations

1. Risk: metadata sections drift in wording across files.
- Mitigation: define one canonical section template in docs operations guidance.

2. Risk: vision sections become decorative and unmeasurable.
- Mitigation: require at least one concrete target envelope line per artifact.

3. Risk: report writers skip the new section under time pressure.
- Mitigation: include section in report templates/checklists and QA scripts.

## 5. Batch 2 Closeout

Batch 2 implementation completed on 2026-03-05.

Closeout report:
- `docs/vision/005-batch-2-closeout-2026-03-05.md`

## Next Task

Run Batch 3 by expanding vision-tag metadata to architecture and contract-index docs, then add automated docs checks that enforce roadmap metadata sections.
