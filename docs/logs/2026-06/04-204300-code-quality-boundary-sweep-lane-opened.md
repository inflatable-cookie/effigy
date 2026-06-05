# Code Quality Boundary Sweep Lane Opened

Date: 2026-06-04
Roadmap: `g08.009`
Batch card: `1037`

## Summary

Opened the code-quality boundary sweep follow-through lane from the
2026-06-04 audit. The lane is scoped to declaration drift, mixed system-level
decisions, duplicate helper definitions, and scan self-adoption noise.

No behavior changes were made in this batch.

## Sweep Baseline

- Rust code under `src` and `crates`: about 251,974 lines.
- Rust test code under test paths: about 59,951 lines.
- `effigy graph index --json`: completed before the sweep.
- `effigy scan duplicate-blocks --json`: 114 findings, with 2 high-severity
  duplicates and 112 warning findings.
- `effigy scan boundary-violations --json`: completed with no configured
  layer findings.
- `effigy scan dead-code --json`: advisory output is too noisy for deletion
  work without rule tuning.
- `effigy scan god-files --json`, `comment-ratio`, and `attention-markers`:
  no active findings in this sweep.

## Follow-Through Targets

- Command surface descriptors: command enum, parser metadata, help registry,
  and runner dispatch carry overlapping released-surface assumptions.
- Rhai feature descriptors: feature names, permissions, host APIs, docs, and
  dispatch paths need one reusable feature inventory.
- Container bring-up: `up` still mixes backend resolution, secrets, shell
  preparation, orchestration, and reporting decisions.
- Repo-marker and root rules: config root selection, repo discovery, and marker
  fixture expectations should share definitions.
- Scan and fixture duplicates: high-confidence duplication exists in scan role
  helpers and help topic row rendering.
- Boundary and dead-code scans: Effigy should adopt explicit self-rules so
  graph-aware scans produce maintainable, low-noise findings on this repo.

## Command Surface Inventory

Initial declaration points for `g08.009`:

- `crates/effigy-cli/src/lib.rs`
- `crates/effigy-cli/src/command_parsing.rs`
- `crates/effigy-cli/src/help/registry.rs`
- `src/runner/entrypoints/dispatch.rs`
- released CLI/help contract docs and tests

Decision: no new strict-lane spec is needed before `1038`. The first slice is
small enough to stay within existing command, JSON, and working-rule contracts:
add descriptor coverage, migrate one low-risk metadata consumer, and leave
parser and runner dispatch explicit.

## Rhai Feature Inventory

Initial declaration points for `g08.009`:

- `crates/effigy-rhai/src/surface.rs`
- `crates/effigy-rhai/src/host_api/`
- `src/runner/script_command/feature_dispatch.rs`
- `src/runner/rhai_command.rs`
- Rhai scripting docs and host-surface tests

`1039` remains pending until command metadata drift is bounded.

## Doctor Caveat

`effigy doctor` still fails on an existing fixture-schema issue, unrelated to
this lane:

- `tests/fixtures/graph-agent-benchmark/split-owner/effigy.toml` contains
  unsupported key `scan.validation_gaps`

This was recorded as a caveat, not folded into the code-quality sweep.

## Validation

- `effigy tasks`: pass
- `effigy doctor`: fail, existing unsupported `scan.validation_gaps` fixture key
- `effigy scan duplicate-blocks --json`: pass, 2 high and 112 warning findings
- `effigy scan boundary-violations --json`: pass
- `effigy test --plan`: pass, selected `cargo nextest run`

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`, `ROUTE`
- Baseline -> current: manual sweep findings are now a sequenced g08 runway
  with the first implementation card ready.
- Open: implement `1038`, then continue through Rhai descriptors, container
  phase cleanup, repo-marker convergence, duplicate-block reduction, and scan
  self-adoption tuning.

## Next Task

Run `1038`.
