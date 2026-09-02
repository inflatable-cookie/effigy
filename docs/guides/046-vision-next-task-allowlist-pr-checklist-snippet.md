> Status: Deprecated
> Superseded by: internal PR support only
> Kept for: narrow historical and repo-specific lint policy work

# 046 - Vision Next-Task Allowlist PR Checklist Snippet

Use this snippet when a PR changes:
- `docs/scripts/fixtures/vision-next-task/actionable-verbs.txt`
- `effigy repo docs check next-action --policy vision`
- the Rust CLI next-action negative-path coverage

## Checklist Snippet

Copy into PR description:

```md
## Vision Next-Task Allowlist Change
- [ ] Change rationale documented (why this verb/policy change is necessary)
- [ ] Added/removed verbs listed explicitly
- [ ] One passing `## Next Task` example included
- [ ] One failing `## Next Task` example included (or reason not applicable)
- [ ] `cargo test --test cli_output_tests cli_docs_check_next_action_json_ -- --nocapture` passed
- [ ] `effigy repo docs check next-action --policy vision` passed
- [ ] `effigy qa:docs:vision` passed
- [ ] `effigy qa:docs` passed
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
