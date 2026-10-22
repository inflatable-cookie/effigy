# Agent Operating Loop

Effigy is not a fixed entry ritual. Pick the first command that matches the
job. The common mistake is front-loading `doctor`, `tasks`, and `test --plan`
even when the work is clearly code understanding or direct execution.

## Route by job

| Job | First command |
|-----|---------------|
| Code understanding | `rg` for the symbol, then `docs/README.md` for owning knowledge |
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

## Code-understanding lane

Effigy has no code graph in v0.15. Start with `rg` on the symbol or string, then
read `docs/README.md` and the owning knowledge file it links. Do not front-load
health or inventory commands for a code-location question.

## Execution lane

```bash
effigy <selector>                  # manifest task or built-in
effigy test <focused-selector>     # named suite or runner filter
```

Prefer Effigy over raw `cargo` / `npm` / `docker compose` when a task or
built-in covers the path.

## Validation lane

```bash
effigy test --plan
effigy test <focused-selector>
```

Use selectors named by the current work brief. `effigy tasks --json` includes
internal tasks as well as public entry points; `effigy --plan <selector>`
inspects either without execution.
Run tests for changed code, compile touched targets, and run the docs selector
once when documentation changed. Capture each exit status. For background
processes, record the PID you started and stop only that PID or process group.
Use Effigy selectors so admission-aware execution stays in effect.

Full `effigy qa` belongs to the planner on `main` at Queue milestones, not the
default per-task check. See the current host-wide admission guide at
`docs/guides/080-host-wide-validation-admission.md`.

`effigy test --plan` narrows a target; it is not exhaustive proof.

## Health lane (when drift matters)

```bash
effigy doctor --verbose            # includes enabled scan checks
effigy scan god-files --json       # individual scanners also available
```

## Not part of the default route

Use only when the repo or user needs them:

| Need | Surface |
|------|---------|
| Local stack | `effigy container up`, `effigy dev` (repo task) |
| Cross-repo clone | `effigy bootstrap <git-url>` |
| Secrets | `effigy secrets doctor`, `effigy secrets list` |
| State / deploy | `effigy state plan`, `effigy deploy plan` |
| Release cut | human-gated; see `release-protocol.md` |

## Source checkout fallback

When `effigy` is not on PATH inside the Effigy repo itself:

```bash
cargo run --bin effigy -- <command>
```

Outside that repo, install the binary per the project README or use
`effigy bootstrap`.

Agent skill files and the executable are independent. Check the executable
resolved from the consumer checkout:

```bash
command -v effigy
effigy --version
```

Heavy selectors wait only for an `EFFIGY_ADMISSION_HOOK` the launcher sets;
`effigy run status|logs|stop <key>` controls a run started with
`--run-id <key>`. Existing agent contexts may still
hold older skill instructions after the files are refreshed.
