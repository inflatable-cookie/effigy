# Workflow Shortcuts

Common command chains, ordered by frequency.

Pick the chain that matches the current job. `graph` is the right default for
code-understanding questions, but not for every Effigy interaction.

## Map code before scanning

```bash
effigy graph explore "<question>" --max-files 6 --max-bytes 12288 --json
git diff --name-only | effigy graph affected --stdin --json
```

Both queries refresh a stale or missing index before reading it.

See `graph-assist.md`.

## Run tests

```bash
effigy test --plan                    # inspect execution shape
effigy test <suite-or-runner-filter>  # run a focused test selection
effigy --json test --plan            # JSON envelope for another tool
```

Use the selector named in the current work brief or a current draft. Filtered
validation should cover changed code and compile touched targets;
`effigy test --plan` never executes a suite.

## Bring local dev up

```bash
effigy container up          # start containers declared in catalog
effigy gateway status        # confirm gateway routing reachable
effigy dev                   # repo task (commonly named dev); not a built-in verb
```

To tear down:

```bash
effigy container down
```

For deeper container ops: `docs/guides/063-container-system-guide.md` and
`docs/guides/064-system-workspace-and-dev-contract.md`.

## Test against local library edits

```bash
effigy deps link cargo ../signal --dry-run
effigy deps link cargo ../signal
effigy --json deps status cargo
effigy deps unlink cargo ../signal
```

Use `bun` for a Bun package library. Re-run the same Bun link after
`bun install` if status reports symlink drift. Never edit the committed
manifest or use Bun `--save`. Guide:
`docs/guides/077-local-dependency-linking.md`.

## Validation for changed work

```bash
effigy tasks
effigy <focused-selector> --plan
effigy test <suite-or-runner-filter>
```

Use the current work brief's named Effigy selectors or a current task draft's
plan to choose tests for changed code and compile touched targets. Run the
repository's docs selector once when docs changed. Record each exit status;
for a background command, record its PID and stop only that process or group.
Effigy selectors preserve host-wide admission for admission-aware work.

The planner owns full `effigy qa` on `main` at Queue milestones. It is not the
default per-task or pre-push recommendation. See
`docs/guides/080-host-wide-validation-admission.md` for binary and admission
proof.

## Manifest scaffolding

```bash
effigy init                        # idempotent baseline repo setup
effigy init --check --json         # no-write setup report for agents/tools
effigy tasks migrate --apply       # import package.json scripts into [tasks]
```

Preview without writing: `effigy tasks migrate` (omit `--apply`).

## Repo scanners

```bash
effigy doctor --verbose              # health + enabled scan checks
effigy scan god-files --json
effigy scan attention-markers --json
effigy scan boundary-violations --json
effigy scan dead-code --json
git diff --name-only | effigy scan validation-gaps --stdin --json
```

Use this lane only when health, drift, or scanner output is the actual job.
Do not insert `doctor` into ordinary code-understanding or task-execution work.

## Changelog

```bash
effigy changelog extract --version X.Y.Z         # extract a release section
effigy changelog extract CHANGELOG.md --version X.Y.Z  # explicit file
effigy --json changelog extract --version X.Y.Z   # JSON envelope
```

## Secrets import

When `[secrets]` is declared and values live in a dotenv file:

```bash
effigy secrets import
effigy secrets import infra/local.env --json
```

## Release inspection (read-only)

First prove the clean, pushed candidate commit through a manually dispatched
`ci.yml` run. Release inspection checks that exact-SHA proof; it does not
accept a green run from another commit.

```bash
effigy release simulate                # dry-run the release flow
effigy release status --check-gates    # show gate states
effigy release prepare --plan          # preview prepare step
effigy release execute --plan          # preview execute step
effigy release gates                   # list gates and current pass/fail
```

These are safe to run unprompted. Anything with `--yes` or that pushes a tag
is **not** safe to run unprompted — see `release-protocol.md` and
`footguns.md`.

Distribution evidence (`preflight`, `proof`, `evidence validate`, …) also
lives under `effigy release`, not a separate `effigy distribution` command.
See `built-in-surfaces.md` and `docs/guides/062-distribution-system-guide.md`.

## Doctor + explain

```bash
effigy doctor                       # health + routing diagnostic
effigy doctor <selector> --           # why does this selector resolve here
effigy doctor --json                # machine-readable envelope
```

This is the right surface for ambiguity and repo health, not a default greeting.

## JSON for everything

Append `--json` (or prefix with `effigy --json <command>`) to get an
`effigy.command.v1` envelope from any command. See `json-envelope.md`.

## Bootstrap from outside the repo

When you don't have the binary on PATH yet:

```bash
effigy bootstrap git@github.com:inflatable-cookie/effigy.git
```

Or run from source:

```bash
cargo run --bin effigy -- <command>
```
