# 038 - Docs IA Snapshot

Use this snapshot when you need a fast map of the active documentation system
without re-reading every guide.

This page is intentionally lighter than a full catalog. It highlights the
current entry points, the main guide clusters, and the support docs that keep
the system coherent.

## Start Here

Choose the entry point by reader intent:

- newcomer or evaluator: [`../../README.md`](../../README.md)
- docs system overview: [`../README.md`](../README.md), then [`./README.md`](./README.md)

Primary docs entry points:

- repository front door: [`../../README.md`](../../README.md)
- docs hub: [`../README.md`](../README.md)
- practical guide hub: [`README.md`](./README.md)

## Current Information Architecture

### Front Doors

| Surface | Purpose | Primary Audience | Update Trigger |
| --- | --- | --- | --- |
| `README.md` | product promise, quick start, main workflows | New User, Operator | top-level product workflow or install story changes |
| `docs/README.md` | docs system routing by goal | New User, Operator, Maintainer | docs structure or primary reading paths change |
| `docs/guides/README.md` | practical guide navigation | Operator, Contributor, Maintainer | guide lineup or recommended paths change |

### Core Operator Guides

| Guide | Purpose | Primary Audience | Trigger to Update |
| --- | --- | --- | --- |
| `021` Quick start and command cookbook | first-run command path | New User, Operator | obvious first-use workflows change |
| `055` Everyday workflows | day-to-day human workflows | Operator | common operator path changes |
| `022` Manifest cookbook | `effigy.toml` patterns | Operator, Maintainer | manifest contract changes |
| `025` Command reference matrix | command and flag lookup | Operator, Maintainer | command/flag/schema changes |
| `023` Troubleshooting recipes | symptom to fix path | Operator, CI Owner | user-facing failures or diagnostics change |

### Runtime and Contract Deep Dives

| Guide | Purpose | Primary Audience | Trigger to Update |
| --- | --- | --- | --- |
| `016` Task routing precedence | selector resolution rules | Operator, Maintainer | routing logic changes |
| `018` Doctor explain mode | routing diagnosis details | Operator, Maintainer | explain behavior or fields change |
| `019` Watch, init, and migrate | setup and rerun flows | Operator | built-in watch/init/migrate behavior changes |
| `017` JSON output contracts | canonical machine-facing contract | CI Owner, Maintainer | envelope or payload rules change |
| `026` JSON payload examples | realistic machine-facing samples | CI Owner, Maintainer | payload field sets change |
| `024` CI and automation recipes | CI and automation patterns | CI Owner, Maintainer | workflow wiring or automation guidance changes |

### Release and Change Communication

| Guide | Purpose | Primary Audience | Trigger to Update |
| --- | --- | --- | --- |
| `051` Release orchestration | built-in release flow and config | Maintainer | release command or config behavior changes |
| `052` Changelog workflows | changelog CLI and policy | Maintainer | changelog workflow or library surface changes |
| `036` Release notes authoring | human release-note structure | Maintainer | release-note requirements change |
| `014` Release checklist template | execution checklist | Maintainer | release process or gates change |

### Docs Operations and Contribution

| Guide | Purpose | Primary Audience | Trigger to Update |
| --- | --- | --- | --- |
| `029` Docs QA checklist and validation | docs validation flow | Contributor, Maintainer | docs QA commands or policy checks change |
| `030` Contributor onboarding | first-pass contributor setup | Contributor | onboarding command flow changes |
| `033` Style and terminology guide | writing standard | Contributor, Maintainer | editorial standards change |
| `037` Documentation contribution playbook | docs update workflow | Contributor, Maintainer | docs contribution process changes |
| `035` Guide ownership and update triggers | ownership and trigger matrix | Maintainer | ownership or trigger policy changes |

### Supplemental and Historical Support

| Guide | Purpose | Primary Audience | Trigger to Update |
| --- | --- | --- | --- |
| `027` Copy/paste snippets | quick bootstrap fragments | Operator, Contributor | recommended starter patterns change |
| `028` Migration quick paths | scenario-based migration paths | Operator, Maintainer | migration strategy changes |
| `archive/031` Docs navigation cleanup | historical navigation cleanup record | Maintainer | no longer actively maintained |
| `archive/032` Docs consistency sweep and changelog | historical docs sweep record | Maintainer | no longer actively maintained |
| `038` Docs IA snapshot | current IA summary | Maintainer, Contributor | primary docs structure changes |
| `archive/028-docs-flow-map` | archived linear-flow navigation map | Maintainer | hub README now owns goal-driven navigation directly |

## How To Use This Snapshot

- planning a docs change: identify the relevant cluster first, then update the
  entry point if discoverability changes
- reviewing a product change: use the trigger columns to decide which guides
  move with it
- onboarding a contributor: pair `030`, `037`, and the practical guide hub
  instead of sending people into the whole docs tree

## Notes

- The practical center of gravity has moved toward the top-level README, the
  docs hub, and the guides hub rather than one linear reading order.
- `055-everyday-workflows.md` is now part of the main operator path and should
  be kept in sync with any major workflow simplifications or new friction
  points.
- `028-migration-quick-paths.md` is the sole active guide under number 028.
  The earlier `028-docs-flow-map.md` duplicated the hub README's goal-driven
  navigation and now lives in `archive/` per policy 040.

## Expected Outcome

- maintainers can identify the active front doors and guide clusters quickly
- contributors can tell which support docs exist to reduce drift
- large docs changes can be scoped without rediscovering the whole IA

## Related Guides

- [`archive/031-docs-navigation-cleanup.md`](./archive/031-docs-navigation-cleanup.md)
- [`archive/032-docs-consistency-sweep-and-changelog.md`](./archive/032-docs-consistency-sweep-and-changelog.md)
- [`035-guide-ownership-and-update-triggers.md`](./035-guide-ownership-and-update-triggers.md)
- [`037-documentation-contribution-playbook.md`](./037-documentation-contribution-playbook.md)

## Next Step

After a significant docs restructure or feature-surface sweep, refresh this
snapshot and then verify that [`README.md`](../../README.md),
[`docs/README.md`](../README.md), and [`docs/guides/README.md`](./README.md)
still reflect the same current reading paths.
