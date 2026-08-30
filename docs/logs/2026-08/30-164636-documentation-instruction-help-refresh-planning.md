# Documentation, Instruction, And Help Parity Refresh Planning

Status: complete
Created: 2026-08-30
Roadmap: g08.036
Card: 1091
Spec: 109

## Summary

- The operator selected a whole-project documentation and in-app help refresh,
  including scan review and a Northstar AGENTS instruction-surface review.
- Strict spec `109`, roadmap `g08.036`, and card `1091` define one bounded
  audit-and-repair worker lane.
- The active documentation-graph lane pauses before card `1089` because both
  lanes can touch command help, generated reference, guides, and coverage tests.
  Card `1091` returns `1089` to ready after evidence-backed closeout.

## Decisions

- “In-app help” means shipped general and scoped Effigy CLI help plus generated
  config/reference output.
- “All documentation” means active current surfaces. Historical logs, archived
  specs, closed roadmap prose, vendored files, and generated build output are
  not rewrite targets.
- The full scan family is evidence. Documentation/help/instruction findings are
  repaired here; unrelated code-only findings receive explicit dispositions and
  remain outside the lane.
- The worker is explicitly authorized to make bounded, evidence-backed repairs
  to `AGENTS.md` and `CLAUDE.md` after the Northstar AGENTS review.
- The g08.034 feature matrix is a baseline checklist, not proof of current
  coverage after later feature work.

## Validation Performed

- `effigy tasks`: passed; docs, scan, focused, and full QA surfaces available
- `effigy doctor`: no errors; graph freshness and five warning-level god-file
  findings reported before planning
- `effigy --json scan god-files`: five warning findings, no high or critical
  findings
- current Northstar authority, g08.034 evidence, active g08.035 runway, docs
  front doors, and repository papercuts reviewed

## Risks

- Whole-project coverage can become unbounded prose churn. The lane requires an
  explicit behavior-family matrix and verified gaps before edits.
- Scan output can invite unrelated code refactors. The card limits repairs to
  documentation, instructions, help, and their verification infrastructure.
- Card `1089` overlaps built-in docs/help and command-matrix surfaces. The
  documentation-graph lane remains paused until this maintenance PR closes.

## Next Task

Dispatch one worker from the committed handoff for ready card `1091`.
