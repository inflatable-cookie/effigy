# 1004 - Add Noninteractive Action Execution And Migration Flows

Roadmap: [`../054-noninteractive-init-action-execution-and-migration-paths.md`](../054-noninteractive-init-action-execution-and-migration-paths.md)
Strict lane: [`../../../specs/093-init-setup-wizard-strict-lane.md`](../../../specs/093-init-setup-wizard-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Let agents and scripts execute selected init setup jobs explicitly without TTY
prompts.

## Work

- add explicit action-selection flow for init
- preserve current baseline init semantics
- support deterministic multi-action ordering and per-action reporting
- include safe migration paths where supported

## Acceptance

- checklist consumers can execute chosen actions non-interactively
- per-action outcomes are visible and machine-readable

## Next Task

Execute `1005`.
