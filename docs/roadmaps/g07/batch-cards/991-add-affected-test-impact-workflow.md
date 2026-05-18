# 991 - Add Affected Test Impact Workflow

Roadmap: [`../042-affected-test-and-impact-workflow.md`](../042-affected-test-and-impact-workflow.md)
Strict lane: [`../../../specs/091-codegraph-parity-strict-lane.md`](../../../specs/091-codegraph-parity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Add a graph-backed changed-file impact command that helps agents choose
smaller validation targets.

## Work

- define the command contract and JSON payload
- accept file args and stdin input
- traverse dependency/reference edges with bounded depth
- classify likely test files and Effigy test tasks
- add quiet path-list mode if justified
- document false-negative risk and confidence levels
- benchmark changed-file cases

## Acceptance

- changed-file impact queries return affected files and likely tests
- command does not execute tests
- confidence and traversal reasons are visible
- benchmark cases show practical validation narrowing

## Evidence

- [`2026-05/18-171905-affected-test-impact-workflow.md`](../../../logs/2026-05/18-171905-affected-test-impact-workflow.md)

## Next Task

Execute `992`.
