# 037 - Documentation Contribution Playbook

Use this playbook for any docs-affecting change.

## 1) Choose Change Type

Pick one primary change type:
1. Command/behavior change
2. JSON contract/schema change
3. Manifest/config surface change
4. CI/workflow/script change
5. Navigation/indexing change
6. Release-note/update-policy change

If multiple apply, process each section below.

## 2) Required Guide Updates by Change Type

### 1) Command/behavior change

Update:
- `021-quick-start-and-command-cookbook.md`
- `025-command-reference-matrix.md`
- relevant deep-dive (`013`, `018`, `019`, `020`, etc.)
- troubleshooting if user-visible failures change (`023`)

### 2) JSON contract/schema change

Update:
- `017-json-output-contracts.md`
- `026-json-payload-examples.md`
- CI parsing/contract docs (`024-ci-and-automation-recipes.md`)

### 3) Manifest/config surface change

Update:
- `022-manifest-cookbook.md`
- `027-copy-paste-snippets.md`
- terminology/style if needed (`033`, `034`)

### 4) CI/workflow/script change

Update:
- `024-ci-and-automation-recipes.md`
- `029-docs-qa-checklist-and-validation.md`
- ownership/trigger map (`035-guide-ownership-and-update-triggers.md`)

CI layout convention:
- active workflow docs should reference `.github-bak/workflows/*.yml` in this repository layout
- if workflows move back to `.github/workflows`, update all docs references in the same PR

### 5) Navigation/indexing change

Update:
- `README.md`
- `docs/README.md`
- `docs/guides/README.md`
- cleanup notes when structural (`031`, `032`)

### 6) Release-note/update-policy change

Update:
- `014-release-checklist-template.md`
- `036-release-notes-authoring-template-and-examples.md`
- `docs/logs/README.md`

## 3) Authoring Rules (Quick)

Follow standards from:
- `033-style-and-terminology-guide.md`
- `034-task-and-command-glossary.md`

Minimum bar:
- include runnable command examples for behavior docs
- use exact schema names for JSON docs
- avoid duplicating long navigation lists across entry pages

## 4) Validation Commands

Run in this order:

```sh
./scripts/check-doc-links.sh README.md $(find docs -name '*.md' | sort)
./scripts/check-quality-gates.sh --docs-only
./docs/scripts/check-doc-workflow-paths.sh
```

If behavior/JSON changed, also run relevant targeted checks:

```sh
./scripts/check-quality-gates.sh --json-only --ci
```

## 5) PR Checklist Snippet

Copy into PR description:

```md
## Documentation Contribution Playbook
- [ ] Change type identified (command/json/manifest/ci/navigation/release-note)
- [ ] Required guides updated for this change type
- [ ] Style/terminology checked against 033/034
- [ ] `./scripts/check-doc-links.sh README.md $(find docs -name '*.md' | sort)` passed
- [ ] `./scripts/check-quality-gates.sh --docs-only` passed
- [ ] `./docs/scripts/check-doc-workflow-paths.sh` passed
- [ ] JSON-related changes: `./scripts/check-quality-gates.sh --json-only --ci` run
```

## 6) Escalation Rules

Escalate docs scope when:
- a command help text change affects automation examples
- a schema field change affects CI parsing snippets
- a workflow/script rename invalidates validation instructions
- a new guide introduces navigation drift across entrypoints

## Expected Outcome

- contributors can map change type to required docs updates quickly
- PRs include reproducible docs validation evidence
- docs regressions are caught before merge

## Related Guides

- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)
- [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)
- [`035-guide-ownership-and-update-triggers.md`](./035-guide-ownership-and-update-triggers.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`038-docs-ia-snapshot.md`](./038-docs-ia-snapshot.md)
- [`039-docs-drift-monitoring.md`](./039-docs-drift-monitoring.md)

## Next Step

Before submitting a docs-impacting PR, execute the validation commands in Section 4 and include results with the checklist snippet from Section 5.
