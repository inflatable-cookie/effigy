# g08.027 - Papercuts Discovery And Capture

Status: Complete
Depends on: `g08.026`
Contracts: [`001`](../../contracts/001-working-rules.md),
[`036`](../../contracts/036-papercuts-discovery-contract.md)
Spec: [`100`](../../specs/100-papercuts-discovery-and-capture-strict-lane.md)

## Goal

Add a rootless-capable Effigy command that aggregates conventional project
papercut queues for humans and agents, then safely captures canonical entries
for one project.

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Target envelope: project friction is discoverable across a local portfolio
  without becoming implicit planning authority.
- Vision target delta: isolated Markdown queues become one deterministic human
  and machine-readable inventory.

## Execution Plan

- [x] card 1070: add read-only discovery, parsing, text, and JSON
- [x] card 1071: add safe capture, publish guidance, validate, and close

## Non-Goals

- no recursive arbitrary-directory scan
- no semantic duplicate grouping or prioritization
- no issue, backlog, spec, roadmap, or close operation
- no Northstar runtime dependency
- no workflow or release mutation

## Acceptance Criteria

- [x] one command works inside a project and from `~/Dev/projects`-style roots
- [x] nested templates do not appear as project queues
- [x] malformed input is diagnosed without hiding valid entries
- [x] JSON output is stable and agent-ready
- [x] add preserves existing Markdown and refuses duplicate open titles

## Next Task

Use `effigy --json papercuts --scope ~/Dev/projects` for the first periodic
portfolio triage run.
