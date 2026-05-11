# Doctor Explain Mode

Use explain mode when you need Effigy to show its routing decision before a task
runs.

This guide is for the moment when a selector feels surprising: wrong catalog,
ambiguity, or deferral that does not match what you expected.


## Vision Alignment

- Primary tags: `ROUTE`, `OPERATE`
- Target movement: selector reasoning is explicit for operators and automation before task execution.

## When To Use This

Reach for `effigy doctor <selector> <args...>` when:

- `effigy <task>` resolves somewhere you did not expect
- a selector is ambiguous and you need the candidate list
- deferral or fallback behavior needs explaining
- you want routing evidence without starting the task itself

## Command Shape

```bash
effigy doctor [--repo <PATH>] <task> <args...>
effigy --json doctor --repo <PATH> <task> <args...>
```

Examples:

```bash
effigy doctor --repo /path/to/workspace catalog-a/build --watch
effigy --json doctor --repo /path/to/workspace catalog-a/build --watch
```

Use text output for human diagnosis. Use JSON when CI, tooling, or tests need
stable reasoning fields.

## Scenario 1: Successful Selection

Command:

```bash
effigy doctor --repo /path/to/workspace catalog-a/build --watch
```

Text output excerpt:

```text
Doctor Explain
──────────────
request: catalog-a/build
selection-status: ok
selected-catalog: catalog-a
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
    "task": "catalog-a/build",
    "args": ["--watch"]
  },
  "selection": {
    "status": "ok",
    "catalog": "catalog-a",
    "mode": "explicit_prefix",
    "evidence": ["selected catalog via explicit prefix `catalog-a`"],
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
effigy doctor --repo /path/to/workspace missing-task
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

## Expected Outcome

After this guide, you should be able to:

- tell whether Effigy selected a catalog directly, failed with ambiguity, or
  fell back to deferral
- inspect the evidence behind a routing outcome before running the task
- choose whether text or JSON explain output is the right fit for the next step

## Related Guides

- Resolution precedence details: [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- Tasks resolution probe mode: [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- Failure recipes for routing/deferral issues: [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)

## Next Step

After adding or changing explain fields, validate JSON shape against [`017-json-output-contracts.md`](./017-json-output-contracts.md) and add a troubleshooting note in [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md) if behavior changed.
