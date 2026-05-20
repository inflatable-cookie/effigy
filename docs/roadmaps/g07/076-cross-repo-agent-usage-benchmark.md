# g07.076 - Cross-Repo Agent Usage Benchmark

Status: Complete
Depends on: `g07.075`

## Goal

Create a small, honest benchmark that measures whether `effigy graph` reduces
agent navigation work on real tasks across multiple repos.

This is not a marketing benchmark. It should show where graph helps, where it
does not, and where `rg` remains the right tool.

## Scope

- define a reusable benchmark manifest for task-shaped navigation questions
- include cases for:
  - ownership lookup
  - behavior lookup
  - split-feature edit target lookup
  - likely-test lookup
  - exact-token search where `rg` should still win
- run cases against at least:
  - Effigy
  - Underlay reference or another Underlay app
  - a decodelabs bundle app or library
  - one small fixture repo
- record:
  - graph command count
  - fallback search count
  - first-hit correctness
  - elapsed time where stable enough to compare
  - whether the packet was enough to avoid opening broad files

## Guardrails

- no exaggerated "percent fewer calls" claim unless the harness proves it
- no benchmark that depends on private local-only paths without skip behavior
- no hard failure when optional external repos are absent
- no broad CI requirement for local legacy repos
- no hiding failed cases

## Acceptance Criteria

- benchmark can run in the Effigy repo with fixture-backed cases
- optional live-repo cases are documented and skipped clearly when absent
- output is readable by humans and parseable by agents
- closeout can state where graph beats search and where it still does not

## Next Task

Execute `1026`.
