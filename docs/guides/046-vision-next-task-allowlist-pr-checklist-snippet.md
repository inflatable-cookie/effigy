# 046 - Vision Next-Task Allowlist PR Checklist Snippet

Use this snippet when a PR changes:
- `docs/scripts/fixtures/vision-next-task/actionable-verbs.txt`
- `docs/scripts/check-vision-next-task.sh`
- `docs/scripts/check-vision-next-task-regression.sh`

## Checklist Snippet

Copy into PR description:

```md
## Vision Next-Task Allowlist Change
- [ ] Change rationale documented (why this verb/policy change is necessary)
- [ ] Added/removed verbs listed explicitly
- [ ] One passing `## Next Task` example included
- [ ] One failing `## Next Task` example included (or reason not applicable)
- [ ] `./docs/scripts/check-vision-next-task-regression.sh` passed
- [ ] `./docs/scripts/check-vision-next-task.sh` passed
- [ ] `./docs/scripts/check-vision-metadata.sh` passed
- [ ] `cargo qa-docs` passed
```

## Required Evidence Fields

When this snippet is used, include:

1. `Rationale`: one paragraph explaining why the allowlist/policy changed.
2. `Verb Delta`: exact added/removed verbs.
3. `Regression Evidence`: command outputs or summary for all four checks.

## Example Verb Delta

```md
Verb Delta:
- Added: `stabilize`
- Removed: `consider`
```

## Expected Outcome

- allowlist changes stay auditable and reviewable
- reviewers can quickly verify rationale and regression coverage

## Related Guides

- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`045-vision-next-task-allowlist-maintenance.md`](./045-vision-next-task-allowlist-maintenance.md)

## Next Step

When an allowlist-related PR is opened, paste this checklist and complete every evidence field before review.
