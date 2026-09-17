# Structural doctor finding: host database tooling

Raised: 2026-09-17. From the Acowtancy workspace (Chatterbox), operator-directed.

## Request

Add a **structural** doctor finding when PostgreSQL tooling is present on the host —
installed (a Homebrew formula or equivalent) or listening on a local port — with the
remediation naming the exact removal commands.

## Why it belongs in the structural tier

It is environment tooling, which is doctor's own stated remit alongside manifest validity
and task references, and doctor already runs an environment-level structural check in
`container.workspace-ownership`. A host database is the same class of fact: a property of
the machine the workspace runs on, not of the repository.

## Why the repository cannot solve it

Doctor's default is now structural-only, and `--deep` adds content scans plus the selected
scope's `health` task. A repository-defined guard therefore has no place to live that a
default `doctor` run reaches:

- in `health`, it is only reached by `--deep`, which is opt-in;
- anywhere else, nothing invokes it.

Acowtancy added exactly such a guard today — a host-database preflight plus a Farmyard
refusal of loopback database URLs — and it is correct and unreached on the default path. It
still fires at PR time and under `--deep`, so the coverage is real but late.

## What it cost before the guard existed

One day in the Acowtancy workspace: three host Postgres servers found running, two started
by lanes the same day; `postgresql@18` installed with a server started from `/tmp/pg-terms-test`
on `127.0.0.1:5433`, roughly an hour after the prohibition was written; a lane reaching the
declared workspace Postgres through a host gateway bridge; and the shared test database
dropped and recreated empty as a result. The written rule was found and understood — it was
still broken within the hour, because from a Queue worktree there was no working alternative.
The conclusion we drew: a prohibition that depends on being read is not enforcement.

## Shape that would work

- Cheap and bounded: a formula/package listing and a loopback listener check. No builds, no
  scans, no container interaction.
- Remediation lines naming the exact commands to stop the server and remove the package.
- Reported structurally, so it appears on a default `doctor` run alongside the ownership check.
- Optional: distinguish "installed but not running" from "installed and listening", since the
  second is the one that damages shared state.

## Note on a companion convention

`--deep` runs the selected scope's `health` task, which implies one complete `health` per
repository. Acowtancy had split its `health` into a cheap composite plus a `health:heavy`
task to keep doctor fast; with default doctor now structural-only that reason is gone, and it
is being merged back so `health` means health. Worth stating in Effigy's guidance if that is
the intended convention, because a project that splits `health` silently gets a `--deep` run
that skips real checks.

## Not asked for here

Acowtancy's own `effigy doctor` reports `manifest.schema.unsupported_key` for a `catalog.graph`
key in eight of its catalogs, and 1,310 findings in one content scan. Both are ours to triage;
recorded only so they are not mistaken for the request above.
