# Workflow Shortcuts

Common command chains, ordered by frequency.

## Run tests

```bash
effigy test                  # built-in test detection or tasks.test override
effigy test --plan           # show plan without running
effigy test --json           # JSON envelope
effigy test <selector>       # run tests in a specific workspace
```

The built-in `test` prefers `cargo-nextest` when available, falling back to
`cargo test`. If the repo defines `tasks.test`, that overrides.

## Bring local dev up

```bash
effigy container up          # start containers declared in catalog
effigy gateway status        # confirm gateway routing reachable
effigy dev                   # start dev process orchestration
```

To tear down:

```bash
effigy container down
```

For deeper container ops: `docs/guides/063-container-commands.md` and
`docs/guides/064-system-and-workspace.md`.

## Pre-push validation

```bash
effigy qa:ci:fast            # fast subset (test, doc, json contracts)
effigy qa:ci:local           # full local CI mirror (fmt, clippy, test, doc, docs-links, json)
effigy qa                    # full QA: test + docs + json contracts
```

Use `qa:ci:fast` when iterating; `qa:ci:local` before pushing to a branch CI
will run.

## Manifest scaffolding

```bash
effigy init                  # interactive scaffold of effigy.toml
effigy migrate               # update existing manifest to current schema
```

`effigy migrate --plan` previews changes without writing.

## Changelog

```bash
effigy changelog extract --version X.Y.Z         # extract a release section
effigy changelog extract CHANGELOG.md --version X.Y.Z  # explicit file
effigy changelog --json extract --version X.Y.Z  # JSON envelope
```

## Release inspection (read-only)

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

## Doctor + explain

```bash
effigy doctor                       # health + routing diagnostic
effigy doctor <selector> --           # why does this selector resolve here
effigy doctor --json                # machine-readable envelope
```

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
