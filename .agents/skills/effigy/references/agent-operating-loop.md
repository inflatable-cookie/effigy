# Agent Operating Loop

Effigy is not a fixed entry ritual. Pick the first command that matches the
job. The common mistake is front-loading `doctor`, `tasks`, and `test --plan`
even when the work is clearly code understanding or direct execution.

## Route by job

| Job | First command |
|-----|---------------|
| Code understanding | `effigy repo graph explore "<question>" --json` |
| Graph-backed risk review | `effigy repo scan <graph-aware-subcommand> --json` |
| Selector inventory | `effigy tasks` |
| Test-routing inspection | `effigy test --plan` |
| Routing ambiguity or repo health | `effigy doctor` |
| Direct execution | `effigy <selector>` |

Use machine-readable output only when another tool or agent step needs it:

```bash
effigy --json tasks
effigy --json doctor
effigy --json test --plan
```

If routing is unclear, narrow the question instead of running broad health
checks by reflex:

```bash
effigy doctor <selector> <args...>
```

Details: `first-five-commands.md` and `selector-routing.md`.

## Code-understanding lane (before broad scanning)

Do this when you need to find owners, trace behavior, or orient in an
unfamiliar tree — **before** spraying `rg` or opening many files.

```bash
effigy repo graph explore "<question>" --max-files 6 --max-bytes 12288 --json
```

Graph queries build or refresh the index on demand. Use `graph status` only
when you need the report-only pre-refresh state, and explicit `graph index`
only to pre-warm a large repo or recover a broken cache.

Details: `graph-assist.md`.

Do not force this phase onto unrelated tasks. If the job is clearly execution,
deployment, state orchestration, docs validation, or release inspection, use
the matching built-in instead of inserting `graph` ritualistically.

## Execution lane

```bash
effigy <selector>                  # manifest task or built-in
effigy test                        # or effigy test --plan first
```

Prefer Effigy over raw `cargo` / `npm` / `docker compose` when a task or
built-in covers the path.

## Validation lane

```bash
git diff --name-only | effigy repo graph affected --stdin --json
effigy test                        # or a repo-specific qa:* task
```

Use `graph affected` to pick a smaller target; it is not exhaustive proof.

## Risk-review lane

```bash
effigy repo scan boundary-violations --json
effigy repo scan dead-code --json
git diff --name-only | effigy repo scan validation-gaps --stdin --json
```

Use this lane when the question is boundary drift, likely isolation, or
validation risk. Do not substitute it for code navigation or exact proof.

## Health lane (when drift matters)

```bash
effigy doctor --verbose            # includes enabled scan checks
effigy repo scan god-files --json       # individual scanners also available
```

## Not part of the default route

Use only when the repo or user needs them:

| Need | Surface |
|------|---------|
| Local stack | `effigy local container up`, `effigy dev` (repo task) |
| Cross-repo clone | `effigy deliver bootstrap <git-url>` |
| Secrets | `effigy admin secrets doctor`, `effigy admin secrets list` |
| State / deploy | `effigy deliver state plan`, `effigy deliver deploy plan` |
| Release cut | human-gated; see `release-protocol.md` |

## Source checkout fallback

When `effigy` is not on PATH inside the Effigy repo itself:

```bash
cargo run --bin effigy -- <command>
```

Outside that repo, install the binary per the project README or use
`effigy deliver bootstrap`.
