# 030 - Contributor Onboarding in 15 Minutes

Use this guide in a fresh clone to reach a reliable working state quickly.

## Prerequisites

- Rust toolchain (`cargo`, `rustc`)
- `jq` (for JSON contract tooling)
- shell access from repository root

Quick checks:

```sh
cargo --version
jq --version
```

## Minute 0-2: Build + General Help

```sh
cargo run --bin effigy -- bootstrap:local --repo .
type -a effigy effigy-dev
effigy-dev --help
```

Expected outcome:
- stable `effigy` and dev `effigy-dev` commands are available from `~/.local/bin`
- dev help output renders with command list (`tasks`, `doctor`, `test`, `watch`, etc.)

## Minute 2-5: Task Discovery + Routing Probe

```sh
effigy-dev tasks --repo .
effigy-dev tasks --repo . --task qa
```

Expected outcome:
- discovered catalogs/tasks are listed
- self-hosted contributor tasks (`qa`, `qa:docs`, `bootstrap:local`, etc.) are visible

## Minute 5-8: Health + Explain

```sh
effigy-dev doctor --repo . --verbose
effigy-dev doctor --repo . test -- --help
```

Expected outcome:
- doctor report produced without runtime crashes
- explain mode returns selector reasoning

## Minute 8-11: Test Planning (Non-Destructive)

```sh
effigy-dev test --plan --repo .
```

Expected outcome:
- suite detection and fallback chain rendered
- `cargo-nextest` is selected when available
- no test execution side-effects (plan-only)

## Minute 11-13: JSON Mode Sanity

```sh
effigy-dev --json tasks --repo .
effigy-dev --json doctor --repo .
effigy-dev --json test --plan --repo .
```

Optional parse checks:

```sh
effigy-dev --json tasks --repo . | jq .schema
effigy-dev --json doctor --repo . | jq .schema
```

Expected outcome:
- each command emits valid JSON envelope (`effigy.command.v1`)

## Minute 13-15: Docs QA Gate

```sh
effigy-dev qa:docs --repo .
```

Expected outcome:
- link checker passes
- docs-only quality gate passes
- for full docs QA checklist/troubleshooting, see [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)

## First Contribution Checklist

- run one docs or code change in a focused scope
- rerun the smallest relevant validation slice
- keep `effigy --json ...` examples aligned with documented schemas
- link any new guide from at least one docs entrypoint

## Fast Re-run Bundle

When returning later, this minimal bundle is usually enough:

```sh
effigy-dev tasks --repo .
effigy-dev doctor --repo . --verbose
effigy-dev test --plan --repo .
effigy-dev qa:docs --repo .
```

## Expected Outcome

- a new contributor can validate core command behavior in one short pass
- JSON mode checks confirm the `effigy.command.v1` envelope
- docs quality checks pass before first PR

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)

## Next Step

After onboarding is complete, use the update rules in [`037-documentation-contribution-playbook.md`](./037-documentation-contribution-playbook.md) to scope your first docs or behavior change.
