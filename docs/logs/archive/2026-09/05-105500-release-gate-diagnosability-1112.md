# Release Gate Diagnosability Closeout

Status: complete
Created: 2026-09-05
Roadmap: g09.004
Batch: release-gate-diagnosability-1112

## Summary

Card `1112` shipped in PR `90` at reviewed head
`4a149cc37101dfcfaa9010075f1124758912f279` and merged to `main` at
`f1732c87ddffda5013194408e7eabdd46916dff8`. The operator ratified the
existing `.gitignore` hygiene write as in scope.

## Changes

- Persisted every executed gate's full stdout/stderr at
  `.effigy/reports/release/gates/<gate>.log` and one redacted
  `environment.json`; latest run wins.
- Added optional `log_path` and `environment_path` JSON fields without schema
  ID changes.
- Added failed-gate 20-line tails and log paths to text output.
- Made inventory and progress stderr-only and announced configured gates before
  execution.
- Ensured `.effigy` is gitignored before report writes, matching the established
  runtime-directory convention; the first-run `.gitignore` mutation is
  operator-ratified in scope.
- Updated guides `051` and `017`; no workflow or release execution changed.

## Review Oracle Evidence

1. Passing and failing fixture gates left per-gate logs and `environment.json`.
2. Logs retained full labelled stdout/stderr; failed text showed exactly the
   last 20 combined lines plus the log path; passed gates stayed one line.
3. `CARGO_REGISTRY_TOKEN` and matching names were recorded as `<redacted>`;
   live grep found no secret value under `.effigy`.
4. Captured non-TTY stderr began with `configured gates (2): alpha, floor`
   before the first gate.
5. JSON stdout contained no progress/inventory lines; existing schema IDs were
   unchanged and new fields were optional.
6. `$SHELL -lc`, gate order, fail-fast, rollback, and `Prepared: no` remained
   unchanged; persistence failures were non-fatal.
7. Gate data stayed under `.effigy/reports/release/gates/`; no new flag, env
   var, gate kind, workflow edit, or release mutation was introduced.

Independent review re-ran all seven rows live at the exact head and approved:
[PR 90 review verdict](https://github.com/inflatable-cookie/effigy/pull/90#issuecomment-5550907541).

## Validation Performed

- `cargo test -p effigy-release --lib` — 25 passed at the revised head.
- Release-focused CLI and library tests — 71 CLI release tests and 54 library
  release tests passed.
- `cargo nextest run --workspace --no-fail-fast` — 3750 passed, 1 skipped.
- `effigy qa:docs` and `effigy qa:json` — passed.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed.
- `git diff --check` — passed.
- Aggregate `effigy qa` encountered the known startup timing flake and a
  transient stream-session timing flake; the latter passed on retry. Equivalent
  coverage completed via nextest plus docs and JSON QA.

## Risks

- A first gated run in a repository without `.effigy` ignored may add the
  `.effigy` entry to `.gitignore`; operators should commit that normal repo
  hygiene change once. The scope was ratified before merge.
- Keep-on-failure remains out of scope and in triage.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `RELEASE`
- Movement: opaque release-gate failure -> persisted artifacts, redacted
  environment, bounded failure tail, and stderr inventory/progress.
- Remaining gap: none for `g09.004`; `g09.005` / card `1113` is the next ready
  serial lane.

## Next Task

Dispatch card `1113` under strict spec `120`; do not dispatch planned `g09.006`.
