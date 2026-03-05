# 011 Batch 8 Closeout - 2026-03-05

Status: Complete
Date: 2026-03-05
Purpose: close Batch 8 by enforcing non-empty `## Next Task` sections across vision artifacts.

## 1. Scope Completed

1. Added dedicated vision next-task validator:
- `docs/scripts/check-vision-next-task.sh`

2. Integrated next-task validation into vision metadata checks:
- `docs/scripts/check-vision-metadata.sh`

3. Updated docs QA guidance to include next-task validation command:
- `docs/guides/029-docs-qa-checklist-and-validation.md`

## 2. Compliance Results

| Artifact Group | Requirement | Result |
| --- | --- | --- |
| Vision artifacts | `## Next Task` section present | Pass |
| Vision artifacts | `## Next Task` section non-empty | Pass |
| Metadata gate | next-task validator executed in vision metadata check | Pass |

## 3. Validation

- command: `./docs/scripts/check-vision-next-task.sh`
  - result: pass
- command: `./docs/scripts/check-vision-metadata.sh`
  - result: pass (includes next-task check)
- command: `cargo qa-docs`
  - result: pass

## 4. Residual Gaps

1. `cargo qa-docs` does not directly run `check-vision-next-task.sh`; enforcement currently flows through `check-vision-metadata.sh`.
2. Next-task quality is syntactic (non-empty) and does not yet score actionability.

## 5. Decision

Batch 8 is accepted as complete for vision next-task enforcement.

## Next Task

Execute Batch 9: add a direct docs-only gate step for `docs/scripts/check-vision-metadata.sh` so all vision integrity checks (index + next-task + policy) run in one standard QA path.
