# Doctor Explain Mode

Use explain mode to diagnose task resolution decisions without running the task itself.

## Command Shape

```bash
effigy doctor <task> <args>
effigy --json doctor <task> <args>
```

Examples:

```bash
effigy doctor farmyard/build -- --watch
effigy --json doctor farmyard/build -- --watch
```

## Scenario 1: Successful Selection

Command:

```bash
effigy doctor farmyard/build -- --watch
```

Text output excerpt:

```text
Doctor Explain
──────────────
request: farmyard/build
selection-status: ok
selected-catalog: farmyard
selected-mode: explicit_prefix
selection-reasoning: selected catalog by explicit task prefix
deferral-considered: false
deferral-selected: false
deferral-reasoning: deferral was not considered because the selection outcome does not trigger deferral
```

JSON output excerpt:

```json
{
  "schema": "effigy.doctor.explain.v1",
  "schema_version": 1,
  "request": {
    "task": "farmyard/build",
    "args": ["--", "--watch"]
  },
  "selection": {
    "status": "ok",
    "catalog": "farmyard",
    "mode": "explicit_prefix",
    "evidence": ["selected catalog via explicit prefix `farmyard`"],
    "error": null
  },
  "reasoning": {
    "selection": "selected catalog by explicit task prefix",
    "deferral": "deferral was not considered because the selection outcome does not trigger deferral"
  }
}
```

## Scenario 2: Missing Target With Deferral Considered

Command:

```bash
effigy doctor missing-task
```

Text output excerpt:

```text
Doctor Explain
──────────────
selection-status: error
selection-reasoning: selection failed because no unambiguous task target was resolved
deferral-considered: true
deferral-selected: true
deferral-reasoning: deferral was selected from configured or implicit fallback routing
deferral-source: catalog root (.../effigy.toml)
```

JSON output excerpt:

```json
{
  "selection": {
    "status": "error",
    "catalog": null,
    "mode": null
  },
  "deferral": {
    "considered": true,
    "selected": true,
    "source": "catalog root (.../effigy.toml)"
  },
  "reasoning": {
    "selection": "selection failed because no unambiguous task target was resolved",
    "deferral": "deferral was selected from configured or implicit fallback routing"
  }
}
```

## Field Summary

- `candidates`: all matching candidate catalogs for the requested task.
- `selection`: final resolution status, catalog, mode, and selection evidence.
- `ambiguity_candidates`: populated when resolution fails due to ambiguity.
- `deferral`: whether fallback deferral was considered and selected.
- `reasoning`: explicit narrative for selection and deferral outcomes.
