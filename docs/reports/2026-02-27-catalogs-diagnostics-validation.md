# Catalogs Diagnostics Validation

Date: 2026-02-27
Owner: Platform
Related roadmap: Backlog - routing diagnostics hardening

## Scope

Validate the built-in `catalogs` diagnostics command across:
- text mode,
- pretty JSON mode,
- compact JSON mode,
- and selector probe behavior.

## Commands

All commands executed from:
`/Users/betterthanclay/Dev/projects/effigy`

```bash
cargo run --quiet --bin effigy -- catalogs --repo /Users/betterthanclay/Dev/projects/acowtancy --resolve farmyard/api
cargo run --quiet --bin effigy -- catalogs --repo /Users/betterthanclay/Dev/projects/acowtancy --json --resolve farmyard/api
cargo run --quiet --bin effigy -- catalogs --repo /Users/betterthanclay/Dev/projects/acowtancy --json --pretty false --resolve farmyard/api
```

Note: Effigy currently renders the standard CLI header preamble before command output in all modes, including JSON.
Note: superseded by root `--json` mode in later updates; JSON-mode commands should now emit pure JSON with no preamble.

## Observed Results

### Text mode

- Catalog count rendered as `6`.
- Catalog rows include alias/root/depth/manifest/defer-state columns.
- Routing precedence block renders all 4 ordered rules.
- Probe section resolved `farmyard/api` to `catalog: farmyard` with explicit-prefix evidence.

### Pretty JSON mode

- JSON payload includes top-level keys: `catalogs`, `precedence`, `resolve`.
- `catalogs` includes all 6 acowtancy catalogs.
- `resolve.status` is `ok`.
- `resolve.catalog` is `farmyard` and `resolve.task` is `api`.

### Compact JSON mode

- Payload rendered on one line.
- Same semantic fields/values as pretty JSON mode.
- Confirmed compatibility for diff/report scripting.

## Sample Snippets

Text probe excerpt:

```text
Resolution Probe: farmyard/api
catalog: farmyard
catalog-root: /Users/betterthanclay/Dev/projects/acowtancy/farmyard
task: api
evidence:
- selected catalog via explicit prefix `farmyard`
```

Pretty JSON probe excerpt:

```json
"resolve": {
  "selector": "farmyard/api",
  "status": "ok",
  "catalog": "farmyard",
  "catalog_root": "/Users/betterthanclay/Dev/projects/acowtancy/farmyard",
  "task": "api",
  "evidence": [
    "selected catalog via explicit prefix `farmyard`"
  ],
  "error": null
}
```

Compact JSON probe excerpt:

```json
{"resolve":{"selector":"farmyard/api","status":"ok","catalog":"farmyard","catalog_root":"/Users/betterthanclay/Dev/projects/acowtancy/farmyard","task":"api","evidence":["selected catalog via explicit prefix `farmyard`"],"error":null}}
```

## Conclusion

`catalogs` diagnostics output is consistent across text and JSON modes and is suitable for both human troubleshooting and machine-readable reporting workflows.
