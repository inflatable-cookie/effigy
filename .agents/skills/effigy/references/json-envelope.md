# JSON Envelope

Every Effigy command run with `--json` returns one JSON document in the
`effigy.command.v1` envelope shape.

## Envelope shape

```json
{
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "ok": true,
  "command": {
    "kind": "tasks",
    "name": "tasks"
  },
  "result": {
    "schema": "effigy.tasks.v1",
    "schema_version": 1,
    "catalog_tasks": [ ... ]
  },
  "error": null
}
```

On error:

```json
{
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "ok": false,
  "command": {
    "kind": "task",
    "name": "missing-task"
  },
  "result": null,
  "error": {
    "kind": "RunnerError",
    "message": "...",
    "details": { ... }
  }
}
```

Key invariants:

- `schema` is always `effigy.command.v1`.
- Either `result` or `error` is non-null, never both.
- `result` carries the command-specific data. Some graph reports additionally
  nest their report under `result.payload`.
- `error.details` is structured (object), not a string.
- A failed command may place useful command-specific report data under
  `error.details`.

## Payload schemas worth parsing

These commands produce stable JSON payloads suitable for agents to consume:

| Command | Payload contains |
|---------|------------------|
| `effigy --json tasks` | `result.catalog_tasks[]` and `result.builtin_tasks[]`; catalog rows use `task`, `manifest`, and `run` |
| `effigy --json doctor` | `result.findings[]` with `check_id`, `evidence`, `severity`, and `remediation` |
| `effigy --json doctor <selector> <args...>` | routing decision tree |
| `effigy --json test --plan` | `result.targets[]` with resolved test plans |
| `effigy --json <selector> --plan` | `result` is `effigy.task.plan.v1` (`executed` is false); `effigy --json <selector>` still runs the task |
| `effigy --json config completion candidates` | `result.candidates[]` and completion cache metadata |
| `effigy --json config` | merged config tree |
| `effigy --json release status` | `result.gates.results[]` on success; failed gate checks may carry the same report under `error.details.gates.results[]` |
| `effigy --json graph status` | `result.payload.freshness`, stale paths, and counts |
| `effigy --json graph explore "<q>"` | primary owners, excerpts, relations, guidance |
| `effigy --json graph affected` | affected files, likely tests/tasks after edits |
| `effigy --json graph explore --excerpts-only "<q>"` | ranked context items (lower-level than explore) |


## Worked `jq` examples

**Pull all task names:**

```bash
effigy --json tasks | jq -r '.result.catalog_tasks[].task'
```

**Find all failing doctor checks:**

```bash
effigy --json doctor \
  | jq -r '.result.findings[] | select(.severity == "error") | .check_id + ": " + .evidence'
```

**Extract release gate states:**

```bash
effigy --json release status --check-gates \
  | jq -r '(.result // .error.details).gates.results[] | "\(.name): \(.passed)"'
```

**Inspect report-only graph freshness:**

```bash
effigy --json graph status | jq '.result.payload.stale_paths'
```

Graph queries report freshness without refreshing. Run `graph index` explicitly
when the returned packet reports missing or stale data.

**Detect error envelope:**

```bash
output=$(effigy --json doctor)
if [ "$(echo "$output" | jq -r '.error')" != "null" ]; then
  echo "$output" | jq -r '.error.kind + ": " + .error.message' >&2
  exit 1
fi
```

## Streaming vs single-shot

Graph commands emit one JSON document; there is no graph event stream.
