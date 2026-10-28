# Project JSON Contract Validation Contract

Owner: Platform

## Purpose

Define how `effigy contracts check-json` finds a project's JSON contract
material, what it validates, and the one JSON Schema engine behind it
([Q-009](../questions.md#q-009--v015-user-facing-surface-cuts)).

## Configuration

A project declares its own paths under `[contracts]` in `effigy.toml`:

```toml
[contracts]
schema_index = "docs/knowledge/contracts/json-schema-index.json"
selection_contract = "docs/knowledge/contracts/json-selection-contract.json"
```

- Paths are repository relative. `--index` and `--contract` override them.
- There is no built-in default. A project without `schema_index` is refused with
  `no JSON schema index configured: declare [contracts] schema_index = "<path>" in
  effigy.toml`, and is never validated against Effigy's own documentation tree.
- A directory with no `effigy.toml` has no configuration: the same refusal
  applies, and an explicit `--index` still works.
- `selection_contract` is required only when a selection payload is validated.

## Schema index

The index keeps its existing row format (`schemas[]` with `schema`,
`schema_version`, `command`, `status`, optional `expect_failure` and
`isolated_home`) and adds one
optional table:

A row with `isolated_home: true` gets its own empty, owned temporary HOME for
the command. This is required for observations such as runtime migration plans
that must not inspect the validator's active runtime state. The directory
survives through command completion and is removed with the other owned
fixtures; omitted or false preserves the caller's environment. Non-boolean
values refuse the index.

```json
"payload_schemas": {
  "effigy.command.v1": "payload-schemas/effigy.command.v1.json"
}
```

- Keys are schema ids as they appear in command payloads (including the nested
  `result.schema` of an `effigy.command.v1` envelope).
- Values are JSON Schema documents, resolved relative to the index file.
- A row whose resolved payload schema id has an entry is validated against that
  document. Rows without one keep the schema/schema_version identity check.
- A payload schema that does not compile, or that is not valid JSON, stops the
  run before any indexed command. The JSON failure is
  `effigy.contracts.check-json.v1` with a `failures[]` row whose schema is
  `effigy.json-contract-body.v1`. It never passes silently.

## One engine

`crates/effigy-json-schema` is the only JSON Schema engine in Effigy:

- It wraps the maintained `jsonschema` crate (0.50) with
  `default-features = false`, so no HTTP or file retriever, no TLS and no async
  runtime is pulled in. Marginal cost is 25 crates, dominated by exact numeric
  comparison (`fraction`/`num-bigint`), `fancy-regex` for pattern keywords, and
  `email_address` for the `email` format annotation.
- Every schema compiles as Draft 2020-12, whatever `$schema` says.
- Unsupported or annotative behaviour is explicit: `format` is never asserted,
  `contentEncoding`/`contentMediaType` are annotations only, and `$ref` resolves
  inside the supplied document — a reference that leaves it is rejected when the
  schema compiles rather than fetched.
- Errors are reported as `<json pointer>: <message>` (or `<root>: <message>`),
  sorted and de-duplicated.
- The crate has no Effigy dependencies, so the planned Rhai JSON Schema API
  (lead `e0330cf7`, Draft 2020-12) depends on this module instead of adding a
  second validator.

`contracts check-json` no longer contains hand-written payload assertions. What
remains is identity: the payload's `schema`/`schema_version` must match the index
row, and a nested `result`/`error.details` payload is unwrapped before its
schema id is checked.

## Selection validation

`validate-selection` is folded into `check-json`:

- `check-json --validate-selection` validates the selection payload of the
  current run against `selection_contract`.
- `--print-selected=json` implies `--validate-selection`, so the CI-facing form
  validates exactly the payload it publishes.
- `--validate-selection --artifact <PATH>` validates a stored artifact instead.
- A violation is a `failures[]` row with schema
  `effigy.json-contract-selection.v1` inside the unchanged
  `effigy.contracts.check-json.v1` envelope; the envelope also reports
  `selection_contract` and `selection_artifact`.
- A selection contract that does not compile, or that is not valid JSON, is the
  same admission refusal as a payload schema body:
  `effigy.json-contract-body.v1` inside `effigy.contracts.check-json.v1`, and no
  command is run.
- `effigy contracts validate-selection` fails with an error naming the new form.

JSON Schema cannot compare a property with an array length, so `count` is a
producer guarantee (`count = selected.length`), recorded in the selection
contract's `producer_guarantees`. Everything else in the payload is schema
checked.

## Retired

- `effigy contracts validate-selection` and its `--contract`/`--artifact`
  position in the command grammar. Replacement: `check-json
  --validate-selection` (implied by `--print-selected=json`) or `--artifact`.
- Rhai feature `contracts.validate_selection`; the selection validation options
  live on `contracts::check_json` as `validate_selection` (bool) plus `contract`
  and `artifact` paths.
- Effigy's implicit default index and selection-contract paths
  (`docs/knowledge/contracts/json-schema-index.json`,
  `docs/knowledge/contracts/json-selection-contract.json`). Replacement:
  `contracts.schema_index` and `contracts.selection_contract` in `effigy.toml`.
- `scripts/write-json-contract-artifacts.rhai` and the
  `json-contracts-selected.json` artifact it wrote for local QA;
  `qa:ci:json` now runs `check-json --print-selected=json`, which
  validates the payload in process.

See [retired.toml](../retired.toml) for the machine-readable record.
