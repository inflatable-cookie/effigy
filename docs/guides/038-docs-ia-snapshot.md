# 038 - Docs IA Snapshot

This snapshot captures the current documentation information architecture for guides `010`-`037`.

## Guide Map

| Guide | Purpose | Primary Audience | Trigger-to-Update |
| --- | --- | --- | --- |
| `010` PATH installation and release | Local/PATH invocation and release workflow | Operator, Maintainer | install/release channel changes |
| `011` Output widgets and colour modes | CLI rendering and color/progress behavior | Operator, Maintainer | renderer/env-var behavior changes |
| `012` Dev process manager TUI | Managed `mode = "tui"` contracts and behavior | Operator | managed task/TUI behavior changes |
| `013` Testing orchestration | Built-in test detection/plan/execution semantics | Operator, Maintainer | built-in test behavior changes |
| `014` Release checklist template | Release execution checklist | Maintainer | release process/policy changes |
| `015` Deferral fallback migration | Legacy deferral strategy and migration | Maintainer | deferral behavior/policy changes |
| `016` Task routing precedence | Selector resolution rules | Operator, Maintainer | routing logic changes |
| `017` JSON output contracts | Envelope/payload contract definitions | CI Owner, Maintainer | schema/contract changes |
| `018` Doctor explain mode | Doctor explain command behavior | Operator, Maintainer | doctor explain fields/behavior changes |
| `019` Watch/init/migrate phase-1 | Built-in watch/init/migrate behavior | Operator | built-in watch/init/migrate changes |
| `020` DAG lock/policy baseline | DAG step policy and locking behavior | Maintainer, Operator | DAG/lock policy changes |
| `021` Quick start + command cookbook | practical command walkthrough | New User, Operator | command usage shape changes |
| `022` Manifest cookbook | copy/paste `effigy.toml` patterns | Operator, Maintainer | manifest contract changes |
| `023` Troubleshooting recipes | symptom -> diagnosis -> fix playbook | Operator, CI Owner | user-facing failures/errors change |
| `024` CI and automation recipes | CI workflow and contract automation patterns | CI Owner | workflow/script contract changes |
| `025` Command reference matrix | command-to-flags/schema matrix | Operator, Maintainer | command/flag/schema changes |
| `026` JSON payload examples | realistic schema payload samples | CI Owner, Maintainer | payload field changes |
| `027` Copy/paste snippets | scenario templates for manifests/CI | Operator, CI Owner | recommended patterns change |
| `028` Migration quick paths | migration decision paths | Maintainer, Operator | migration strategy changes |
| `028` Docs flow map (legacy) | legacy linear docs navigation map | New User, Maintainer | navigation model restructuring |
| `029` Docs QA checklist and validation | docs quality-gate and validation steps | Maintainer, Contributor | docs QA commands/workflow changes |
| `030` Contributor onboarding (15 min) | first-run contributor command path | Contributor | onboarding command flow changes |
| `031` Docs navigation cleanup | navigation normalization policy | Maintainer | docs IA/navigation refactors |
| `032` Docs consistency sweep/changelog | consistency sweep outcomes | Maintainer | entrypoint structure updates |
| `033` Style and terminology guide | docs writing style rules | Contributor, Maintainer | style/wording standards change |
| `034` Task and command glossary | canonical term definitions | Contributor, Operator | terminology set changes |
| `035` Guide ownership and update triggers | docs update trigger matrix | Maintainer | ownership/trigger policy changes |
| `036` Release notes authoring template | release-note structure and examples | Maintainer | release-note requirements change |
| `037` Documentation contribution playbook | end-to-end docs contribution workflow | Contributor, Maintainer | docs contribution process changes |

## How to Use This Snapshot

- planning docs work: use this table to identify which guides are in-scope
- reviewing behavior changes: cross-check trigger-to-update column
- onboarding contributors: pair `030` + `037` + this snapshot

## Notes

- `028` currently has two guides by design:
  - `028-migration-quick-paths.md` (primary)
  - `028-docs-flow-map.md` (supplemental/legacy)
- numbering continuity is tracked in `031` and `032`.

## Expected Outcome

- maintainers can quickly identify docs impacted by behavior, schema, workflow, or navigation changes
- contributors can map update triggers without scanning every guide
- docs planning and review use one shared IA snapshot

## Related Guides

- [`031-docs-navigation-cleanup.md`](./031-docs-navigation-cleanup.md)
- [`032-docs-consistency-sweep-and-changelog.md`](./032-docs-consistency-sweep-and-changelog.md)
- [`035-guide-ownership-and-update-triggers.md`](./035-guide-ownership-and-update-triggers.md)
- [`037-documentation-contribution-playbook.md`](./037-documentation-contribution-playbook.md)

## Next Step

After major docs or behavior changes, refresh this snapshot and confirm trigger mappings still align with [`035-guide-ownership-and-update-triggers.md`](./035-guide-ownership-and-update-triggers.md).
