# 037 - Documentation Contribution Playbook

Use this playbook for any docs-affecting change.

Use it when the question is not "how do I write prose?" but "which docs move
with this product change, and how do I prove I updated the right ones?"

This is the primary guide for docs maintenance work.

Use:
- this guide for change scoping and update rules
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
  for the actual QA checklist and commands
- [`040-docs-archive-and-deprecation-policy.md`](./040-docs-archive-and-deprecation-policy.md)
  for retirement, merge, archive, and deprecation rules

## Start Here

Work in this order:

1. Identify the primary change type.
2. Update the matching docs surfaces in the same batch.
3. Run the docs QA bundle.
4. Run JSON contract checks too if the change affects machine-facing output.

If you only need the shortest maintenance path:

1. update the affected docs in the same batch as the behavior change
2. run `effigy qa:docs`
3. run `effigy docs check-workflow-paths`
4. widen to JSON or broader QA only when the change actually touches that surface

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
- active workflow docs should reference `.github/workflows/*.yml`
- if a workflow is renamed or relocated, update all docs references in the same PR

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
- keep agent-facing examples on repo-root defaults unless cross-repo execution is intentional

## 4) Validation Commands

Run in this order:

```sh
effigy qa:docs
effigy docs check-workflow-paths
```

If the change touches `AGENTS.md`, adoption snippets, setup/install docs, or
workflow examples,
also make sure the agent-default drift guard is still green:

```sh
effigy qa:docs:agent-defaults
```

Fallbacks when validating from a dev checkout instead of the installed binary:

```sh
effigy-dev qa:docs
cargo qa-docs
```

If behavior/JSON changed, also run relevant targeted checks:

```sh
effigy qa:json:ci
```

Dev-checkout fallback:

```sh
effigy-dev qa:json:ci
```

## 5) PR Checklist Snippet

Copy into PR description:

```md
## Documentation Contribution Playbook
- [ ] Change type identified (command/json/manifest/ci/navigation/release-note)
- [ ] Required guides updated for this change type
- [ ] Style/terminology checked against 033/034
- [ ] `effigy qa:docs` passed
- [ ] Agent/default guidance changes: `effigy qa:docs:agent-defaults` run
- [ ] `effigy docs check-workflow-paths` passed
- [ ] JSON-related changes: `effigy qa:json:ci` run
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
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`038-docs-ia-snapshot.md`](./038-docs-ia-snapshot.md)
- [`040-docs-archive-and-deprecation-policy.md`](./040-docs-archive-and-deprecation-policy.md)

## Next Step

Before submitting a docs-impacting PR, execute the validation commands in Section 4 and include results with the checklist snippet from Section 5.
