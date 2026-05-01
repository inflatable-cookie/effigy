# 030 - Contributor Onboarding in 15 Minutes

Use this guide in a fresh clone to reach a reliable working state quickly.

Use the installed `effigy` path when it is already available. Use `effigy-dev`
or `cargo run --bin effigy -- ...` only when validating the current checkout
before the local install is refreshed.

## Start Here

The shortest useful onboarding pass is:

1. confirm the toolchain and local command path
2. ask the repo what tasks exist
3. run health and test planning in non-destructive mode
4. run the docs QA bundle before the first PR

If `effigy` is not yet available on `PATH`, bootstrap first:

```sh
cargo run --bin effigy -- bootstrap:local
effigy version
type -a effigy effigy-dev
```

`bootstrap:local` installs a local binary and stamps an active build identity.
Expect `effigy version` to show the stable semver plus a local suffix such as
`v0.3.1+local.abc123` when you are using a self-built checkout.

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
effigy --help
# dev-checkout fallback:
# effigy-dev --help
```

Expected outcome:
- the active command path is clear
- the active build identity is clear when using a local stamped install
- help output renders with the expected built-ins (`tasks`, `doctor`, `test`,
  `watch`, and others)

## Minute 2-5: Task Discovery + Routing Probe

```sh
effigy tasks
effigy tasks --task qa
```

Dev-checkout fallback:

```sh
effigy-dev tasks
```

Expected outcome:
- discovered catalogs/tasks are listed
- self-hosted contributor tasks (`qa`, `qa:docs`, `bootstrap:local`, etc.) are visible

## Minute 5-8: Health + Explain

```sh
effigy doctor --verbose
effigy doctor test -- --help
```

Expected outcome:
- doctor report produced without runtime crashes
- explain mode returns selector reasoning

## Minute 8-11: Test Planning (Non-Destructive)

```sh
effigy test --plan
```

Expected outcome:
- suite detection and fallback chain rendered
- `cargo-nextest` is selected when available
- no test execution side-effects (plan-only)

## Minute 11-13: JSON Mode Sanity

```sh
effigy --json tasks
effigy --json doctor
effigy --json test --plan
```

Optional parse checks:

```sh
effigy --json tasks | jq .schema
effigy --json doctor | jq .schema
```

Expected outcome:
- each command emits valid JSON envelope (`effigy.command.v1`)

## Minute 13-15: Docs QA Gate

```sh
effigy qa:docs
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
effigy tasks
effigy doctor --verbose
effigy test --plan
effigy qa:docs
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
