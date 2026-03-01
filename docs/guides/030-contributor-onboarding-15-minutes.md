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
cargo run --bin effigy -- --help
```

Expected outcome:
- help output renders with command list (`tasks`, `doctor`, `test`, `watch`, etc.)

## Minute 2-5: Task Discovery + Routing Probe

```sh
cargo run --bin effigy -- tasks
cargo run --bin effigy -- tasks --resolve test
```

Expected outcome:
- discovered catalogs/tasks are listed
- routing probe returns selection evidence (or explicit not-found/ambiguity diagnostics)

## Minute 5-8: Health + Explain

```sh
cargo run --bin effigy -- doctor --verbose
cargo run --bin effigy -- doctor test -- --help
```

Expected outcome:
- doctor report produced without runtime crashes
- explain mode returns selector reasoning

## Minute 8-11: Test Planning (Non-Destructive)

```sh
cargo run --bin effigy -- test --plan
```

Expected outcome:
- suite detection and fallback chain rendered
- no test execution side-effects (plan-only)

## Minute 11-13: JSON Mode Sanity

```sh
cargo run --bin effigy -- --json tasks
cargo run --bin effigy -- --json doctor
cargo run --bin effigy -- --json test --plan
```

Optional parse checks:

```sh
cargo run --bin effigy -- --json tasks | jq .schema
cargo run --bin effigy -- --json doctor | jq .schema
```

Expected outcome:
- each command emits valid JSON envelope (`effigy.command.v1`)

## Minute 13-15: Docs QA Gate

```sh
cargo qa-docs
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
cargo run --bin effigy -- tasks
cargo run --bin effigy -- doctor --verbose
cargo run --bin effigy -- test --plan
cargo qa-docs
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
