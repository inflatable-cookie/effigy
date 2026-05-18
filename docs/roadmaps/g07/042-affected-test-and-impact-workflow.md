# g07.042 - Affected Test And Impact Workflow

Status: Complete
Depends on: `g07.041`

## Goal

Add a changed-file impact workflow comparable to CodeGraph's affected-test
surface, adapted to Effigy's task runner responsibilities.

## Scope

- add `effigy graph affected` or equivalent command after contract review
- accept changed files from args and stdin
- traverse imports/call/reference edges outward with a bounded depth
- identify likely tests from:
  - test file naming
  - test symbols
  - manifest task/test selectors
  - language-specific dependency edges
- return:
  - affected files
  - affected test files
  - candidate Effigy test tasks when resolvable
  - confidence and traversal reason
- include `--json`, quiet file-list mode, and depth/filter options if justified

## Guardrails

- no claim that affected tests are exhaustive unless the language graph proves
  it
- no automatic test execution in the query command
- no integration with release gates until the command is proven
- no fallback to scanning the whole tree on every query

## Acceptance Criteria

- changed-file impact queries work from `git diff --name-only`
- agents can choose a smaller validation target with visible confidence
- false-negative risks are documented
- benchmark harness includes at least two affected-test cases

## Evidence

- [`2026-05/18-171905-affected-test-impact-workflow.md`](../../logs/2026-05/18-171905-affected-test-impact-workflow.md)

## Next Task

Execute `992` after affected workflow behavior is measurable.
