# JSON Contracts Validation

Date: 2026-02-28
Owner: Platform
Related roadmap: Backlog - JSON output contracts

## Scope

Validate versioned JSON schema contracts for top-level commands:

- `tasks`
- `tasks --task <name>`
- `repo-pulse`
- `catalogs`
- `test --plan`
- `test`

## Commands

All commands executed from:
`/Users/betterthanclay/Dev/projects/effigy`

```bash
cargo run --quiet --bin effigy -- --json tasks --repo /Users/betterthanclay/Dev/projects/acowtancy
cargo run --quiet --bin effigy -- --json tasks --repo /Users/betterthanclay/Dev/projects/acowtancy --task test
cargo run --quiet --bin effigy -- --json repo-pulse --repo /Users/betterthanclay/Dev/projects/effigy
cargo run --quiet --bin effigy -- --json catalogs --repo /Users/betterthanclay/Dev/projects/acowtancy --resolve farmyard/api
cargo run --quiet --bin effigy -- --json test --plan --repo /Users/betterthanclay/Dev/projects/effigy
cargo run --quiet --bin effigy -- --json test --repo /Users/betterthanclay/Dev/projects/effigy
```

## Contract Identity Checks

- `tasks`: `effigy.tasks.v1|1`
- `tasks --task test`: `effigy.tasks.filtered.v1|1|filter=test`
- `repo-pulse`: `effigy.repo-pulse.v1|1|owner=platform`
- `catalogs`: `effigy.catalogs.v1|1|resolve=ok|catalog=farmyard`
- `test --plan`: `effigy.test.plan.v1|1`
- `test`: `effigy.test.results.v1|1`

## Observed Payload Summaries

`tasks` summary:

```json
{
  "schema": "effigy.tasks.v1",
  "schema_version": 1,
  "catalog_count": 7,
  "builtin_tasks_count": 7
}
```

`tasks --task test` summary:

```json
{
  "schema": "effigy.tasks.filtered.v1",
  "schema_version": 1,
  "filter": "test",
  "builtin_matches_count": 1,
  "notes_count": 1
}
```

`repo-pulse` summary:

```json
{
  "schema": "effigy.repo-pulse.v1",
  "schema_version": 1,
  "repo": "/Users/betterthanclay/Dev/projects/effigy",
  "evidence_count": 7,
  "risk_count": 0
}
```

`catalogs` summary:

```json
{
  "schema": "effigy.catalogs.v1",
  "schema_version": 1,
  "catalogs_count": 7,
  "resolve": {
    "selector": "farmyard/api",
    "status": "ok",
    "catalog": "farmyard"
  }
}
```

`test --plan` summary:

```json
{
  "schema": "effigy.test.plan.v1",
  "schema_version": 1
}
```

`test` summary:

```json
{
  "schema": "effigy.test.results.v1",
  "schema_version": 1
}
```

## Validation Result

All six JSON command surfaces returned versioned schemas with expected shape and stable top-level keys. Root `--json` mode produced pure JSON output suitable for CI and tooling consumers.
