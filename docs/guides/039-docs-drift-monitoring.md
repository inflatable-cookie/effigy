# 039 - Docs Drift Monitoring

Use this guide for recurring documentation maintenance.

## 1) Docs Maintenance Cadence

Run from repository root.

| Cadence | Owner | Required Checks | Deliverable |
| --- | --- | --- | --- |
| Weekly (every Friday) | Docs maintainer on rotation | Link integrity, entrypoint index coherence, terminology spot check (`JSON mode`, `selector`, `routing`, `deferral`) | short checkpoint log in `docs/logs/` if issues are found |
| Monthly (first business day) | Docs maintainer + CI owner | Full monthly drift checklist in this guide | dated drift log in `docs/logs/` |
| Quarterly (first week of quarter) | Docs maintainer + maintainer lead | Quarterly deep drift sweep in this guide | consolidated IA health log with follow-up actions |

If ownership rotates, update the current owner in your team runbook and reference that owner in the dated log.

## 2) Monthly Drift Checklist

Run from repository root.

### A) Link integrity

```sh
effigy docs check-links --repo . README.md $(find docs -name '*.md' | sort)
```

Pass criteria:
- no broken links

### B) Index coherence

Review:
- `README.md`
- `docs/README.md`
- `docs/guides/README.md`

Pass criteria:
- no stale links
- newcomer path still clear
- supplemental/legacy links not promoted into primary onboarding unintentionally

### C) Schema-reference verification

Review:
- `docs/guides/017-json-output-contracts.md`
- `docs/guides/026-json-payload-examples.md`
- `docs/guides/025-command-reference-matrix.md`

Pass criteria:
- schema names/versions consistent (`effigy.command.v1` and payload schemas)
- payload examples still aligned with current contracts

### D) Command example sanity

Spot-run:

```sh
effigy-dev --help
effigy-dev tasks --repo .
effigy-dev doctor --repo . --verbose
effigy-dev test --plan --repo .
effigy-dev --json tasks --repo . | jq .schema
```

Pass criteria:
- commands still execute as documented
- JSON mode still emits expected envelope

### E) Quality gates

```sh
effigy-dev qa:docs --repo .
```

Pass criteria:
- docs-only quality gate passes

### F) Workflow path reference coherence

```sh
effigy docs check-workflow-paths --repo .
```

Pass criteria:
- docs-referenced workflow paths exist in the current repository layout
- no stale `.github/workflows/*.yml` references remain when `.github-bak/workflows/*.yml` is authoritative

## 3) Quarterly Deep Drift Sweep

In addition to monthly checks:
- compare guide matrix in `038` against current guide set
- verify trigger matrix in `035` still matches active code/workflow patterns
- ensure `029` commands still reflect actual scripts/workflows
- review `036` release-note template against recent release-note quality

## 4) Recurring Log Snippet

Use this in a dated log under `docs/logs/YYYY-MM/`:

```md
# Docs Drift Monitoring Checkpoint

Date: YYYY-MM-DD
Owner: <team/person>

## Scope
- Monthly docs drift checklist (`039`)

## Validation
- command: `effigy docs check-links --repo . README.md $(find docs -name '*.md' | sort)`
  - result: pass/fail
- command: `effigy-dev qa:docs --repo .`
  - result: pass/fail
- command: `effigy-dev test --plan --repo .`
  - result: pass/fail

## Findings
- ...

## Risks / Follow-ups
- ...

## Next
- ...
```

## 5) Escalation Conditions

Escalate to docs cleanup work when any of these occur:
- repeated link-check failures across checkpoints
- command examples no longer match actual CLI flags
- schema docs diverge from current contracts
- guide indexes drift into contradictory onboarding paths

## Expected Outcome

- recurring checks catch link, navigation, and terminology drift before it accumulates
- docs updates stay aligned with live command and schema behavior
- maintenance ownership and cadence remain explicit across weeks, months, and quarters

## Related Guides

- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`032-docs-consistency-sweep-and-changelog.md`](./032-docs-consistency-sweep-and-changelog.md)
- [`035-guide-ownership-and-update-triggers.md`](./035-guide-ownership-and-update-triggers.md)
- [`038-docs-ia-snapshot.md`](./038-docs-ia-snapshot.md)

## Next Step

After each monthly run, update the latest findings in [`032-docs-consistency-sweep-and-changelog.md`](./032-docs-consistency-sweep-and-changelog.md) and link the dated log.
