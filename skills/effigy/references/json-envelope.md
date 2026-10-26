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
    "kind": "refused",
    "type": "RunnerError",
    "message": "...",
    "details": { ... }
  }
}
```

Key invariants:

- `schema` is always `effigy.command.v1`.
- Either `result` or `error` is non-null, never both.
- `result` carries the command-specific data.
- `error.kind` is one of the three kinds a person reads: `refused`,
  `unavailable` or `broken`; `error.type` names the error's own type (for
  example `RunnerError`, `CliParseError`).
- `error.details` is structured (object), not a string.
- A failed command may place useful command-specific report data under
  `error.details`.

## Payload schemas worth parsing

These commands produce stable JSON payloads suitable for agents to consume:

| Command | Payload contains |
|---------|------------------|
| `effigy --json tasks` | `result.catalog_tasks[]` and `result.builtin_tasks[]`; catalog rows use `task`, `manifest`, `run`, `group`, `visibility` and `description` |
| `effigy --json doctor` | `result.findings[]` with `check_id`, `evidence`, `severity`, and `remediation` |
| `effigy --json doctor <selector> <args...>` | routing decision tree |
| `effigy --json test --plan` | `result.targets[]` with resolved test plans |
| `effigy --json --plan <selector>` | `result` is `effigy.task.plan.v1` (`executed` is false); `effigy --json <selector>` still runs the task |
| `effigy --json config completion candidates` | `result.candidates[]` and completion cache metadata |
| `effigy --json config` | merged config tree |
| `effigy --json release status` | `result.gates.results[]` on success; failed gate checks may carry the same report under `error.details.gates.results[]` |


## Worked `jq` examples

**Pull all task names:**

```bash
effigy --json tasks | jq -r '.result.catalog_tasks[].task'
```

**Read what each task is for:**

```bash
effigy --json tasks \
  | jq -r '.result.catalog_tasks[] | select(.description) | "\(.task): \(.description)"'
effigy --json <task> --help   # effigy.help.v1 for one task; nothing runs
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

**Detect error envelope:**

```bash
output=$(effigy --json doctor)
if [ "$(echo "$output" | jq -r '.error')" != "null" ]; then
  echo "$output" | jq -r '.error.kind + ": " + .error.message' >&2
  exit 1
fi
```

## Streaming vs single-shot

Effigy emits one JSON document per command; there is no event stream.
