# 045 - Vision Next-Task Allowlist Maintenance

Use this guide when updating `docs/scripts/fixtures/vision-next-task/actionable-verbs.txt`.

## 1) Purpose

The allowlist controls which lead verbs are accepted by:
- `docs/scripts/check-vision-next-task.sh`
- `docs/scripts/check-vision-next-task-regression.sh`

Changes to this file directly affect docs QA behavior for `docs/vision/*` artifacts.

## 2) Update Criteria

Add a verb only when all are true:

1. It is clearly action-oriented for a follow-on engineering task.
2. It improves real authoring flexibility (not just stylistic preference).
3. It does not weaken lint quality by allowing vague intents.

Do not add verbs that imply passive intent.

## 3) Acceptable vs Unacceptable Examples

Acceptable lead phrases:
- `Execute Batch 13...`
- `Add fixture coverage for...`
- `Update docs QA guidance...`
- `Validate release-note policy...`
- `Integrate check into docs-only gate...`

Unacceptable lead phrases:
- `Consider...`
- `Think about...`
- `Maybe update...`
- `Look into...`

## 4) Change Procedure

1. Edit `docs/scripts/fixtures/vision-next-task/actionable-verbs.txt`.
2. Keep one verb per line in lowercase.
3. Keep ordering stable (append by default unless grouping/cleanup is deliberate).
4. Run validation:

```sh
./docs/scripts/check-vision-next-task-regression.sh
./docs/scripts/check-vision-next-task.sh
./docs/scripts/check-vision-metadata.sh
effigy qa:docs --repo .
```

5. In your PR notes, include:
- why the verb was added/removed,
- one example `## Next Task` line that now passes/fails because of the change.
- use the reusable checklist snippet in [`046-vision-next-task-allowlist-pr-checklist-snippet.md`](./046-vision-next-task-allowlist-pr-checklist-snippet.md).

## 5) Review Checklist

- [ ] Verb is action-oriented and specific.
- [ ] No vague/passive intent introduced.
- [ ] Regression fixtures still pass.
- [ ] Existing vision artifacts still pass.
- [ ] Docs QA (`effigy qa:docs --repo .`) remains green.

## Expected Outcome

- allowlist updates are intentional, reviewable, and low-risk
- next-task lint remains strict enough to preserve actionable follow-on work

## Related Guides

- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`037-documentation-contribution-playbook.md`](./037-documentation-contribution-playbook.md)
- [`046-vision-next-task-allowlist-pr-checklist-snippet.md`](./046-vision-next-task-allowlist-pr-checklist-snippet.md)

## Next Step

When you adjust the allowlist, attach regression command output in your PR and link the exact verb delta.
