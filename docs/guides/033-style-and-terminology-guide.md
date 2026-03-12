# 033 - Style and Terminology Guide

Use this guide as the canonical writing standard for Effigy documentation.

## 1) Writing Tone

Required tone:
- direct, operational, and specific
- neutral and technical (no marketing language)
- actionable first; background second

Avoid:
- vague wording (`maybe`, `might` without context)
- placeholder advice with no commands/examples
- repeated explanations across multiple entry pages

## 2) Structure Conventions

Preferred section order:
1. purpose/when-to-use
2. commands or config examples
3. expected outcomes or constraints
4. related guides

For procedural docs:
- use numbered steps
- include at least one copy/paste command block
- include "Expected outcome" for critical steps

## 3) Command Formatting

- use fenced blocks with `sh` or `bash`
- commands must be runnable from repo root unless explicitly stated
- show canonical JSON mode as `effigy --json <command>`
- for non-destructive guidance, prefer plan/dry-run flags first (`--plan`, `--dry-run`)

Example:

```sh
effigy tasks --resolve test
effigy doctor --verbose
effigy --json test --plan
```

## 4) Config and Schema Naming Rules

Manifest naming:
- use `effigy.toml` consistently

Schema naming:
- use exact schema ids (for example `effigy.command.v1`, `effigy.test.plan.v1`)
- keep version suffixes in prose and examples
- avoid shorthand names when precision is required

## 5) Linking Rules

Entry-point strategy:
- `README.md`: newcomer-critical links only
- `docs/README.md`: complete docs index
- `docs/guides/README.md`: persona/task navigation

Cross-linking:
- each new guide should be linked from at least one index page
- avoid duplicating the same long link list in every page
- place legacy/secondary navigation in "Supplemental" sections

Validation:

```sh
effigy docs check-links --repo .
effigy qa:docs --repo .
```

## 6) Terminology Canon

Use these canonical terms:
- "catalog" (not module/workspace alias interchangeably)
- "selector" for `<catalog>/<task>` or `<task>` request strings
- "routing" for catalog/task selection behavior
- "deferral" for unresolved-request forwarding
- "command envelope" for `effigy.command.v1`
- "payload schema" for command-specific `result` contracts

Preferred phrasing:
- "Run `effigy tasks --resolve <selector>` to inspect routing evidence."
- "Use `effigy --json <command>` for machine consumers."

## 7) Example Quality Bar

Every new guide should include at least one of:
- realistic command examples
- minimal config snippet
- failure symptom -> fix mapping

For schema docs:
- include concrete JSON objects with realistic fields
- avoid placeholder-only payloads unless explicitly labeled schematic

## 8) Update Checklist (Per Guide)

Before finalizing a guide update:
1. confirm links resolve
2. confirm commands match current CLI behavior
3. confirm schema names/versions are current
4. confirm guide is discoverable from at least one index
5. avoid introducing redundant navigation blocks

## Expected Outcome

- docs stay consistent in tone, command examples, and schema naming
- terminology is applied uniformly across onboarding, operations, and CI guides
- new guide updates require less editorial cleanup during review

## Related Guides

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`031-docs-navigation-cleanup.md`](./031-docs-navigation-cleanup.md)
- [`032-docs-consistency-sweep-and-changelog.md`](./032-docs-consistency-sweep-and-changelog.md)
- [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)
- [`035-guide-ownership-and-update-triggers.md`](./035-guide-ownership-and-update-triggers.md)

## Next Step

After writing or updating a guide, run through the checklist in Section 8 and then validate links and docs quality gates before opening a PR.
