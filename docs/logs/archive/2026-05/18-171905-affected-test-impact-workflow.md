# Affected Test Impact Workflow

Date: 2026-05-18  
Roadmap: [`g07.042`](../../../roadmaps/g07/042-affected-test-and-impact-workflow.md)  
Batch card: [`991`](../../../roadmaps/g07/batch-cards/991-add-affected-test-impact-workflow.md)  
Strict lane: [`091`](../../../specs/091-codegraph-parity-strict-lane.md)

## What Changed

- added `effigy graph affected` to the CLI and runner surfaces
- added a new additive JSON payload:
  - `effigy.graph.affected.v1`
- accepted changed files from:
  - positional args
  - newline-delimited stdin via `--stdin`
- traversed bounded graph topology outward from changed files and their symbols
- classified likely validation targets into:
  - `affected_files`
  - `likely_test_files`
  - `likely_test_tasks`
- attached visible confidence labels and traversal reasons to affected files

## Contract Shape

`graph affected --json` returns:

- `changed_paths`
- `freshness`
- `depth`
- `affected_files`
- `likely_test_files`
- `likely_test_tasks`
- `notes`

Confidence values currently shipped:

- `exact`
- `heuristic`

The command does not run tests. It only narrows likely validation scope.

## Validation

- `cargo test -p effigy-codegraph`
- `cargo test parse_graph_affected_accepts_depth_limit_and_stdin -- --nocapture`
- `cargo test graph_affected_json_and_text_report_likely_validation_targets -- --nocapture`
- `cargo clippy -p effigy-codegraph -p effigy-cli -p effigy -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo fmt --all -- --check`

New regressions:

- `graph_affected_returns_likely_test_files_and_tasks_for_changed_source`
- `parse_graph_affected_accepts_depth_limit_and_stdin`
- `graph_affected_json_and_text_report_likely_validation_targets`

## Interpretation

- agents now have a bounded first-pass answer to "what should I test after
  changing these files?" without inventing their own ad hoc file-walk logic
- manifest task facts and graph topology now meet in one workflow, so the
  graph can suggest both likely test files and candidate Effigy test tasks
- the result is intentionally conservative about certainty and keeps heuristic
  evidence visible

## Residual Limits

- affected output is still bounded, not compiler-grade exhaustive reachability
- test task classification is heuristic and currently tuned toward common test
  selectors rather than a stronger manifest role model
- large-repo storage and migration hardening still needs its own pass before
  parity closeout claims

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: Effigy now has a graph-backed changed-file impact workflow that
  accepts diff-shaped input and returns likely validation targets with visible
  confidence and reasons
- remains open: large-repo scale hardening, agent workflow polish, and final
  parity proof

## Next Task

Execute `992`.
