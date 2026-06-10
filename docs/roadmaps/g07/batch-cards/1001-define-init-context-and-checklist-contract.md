# 1001 - Define Init Context And Checklist Contract

Roadmap: [`../051-init-context-inventory-and-checklist-contract.md`](../051-init-context-inventory-and-checklist-contract.md)
Strict lane: [`../../../specs/093-init-setup-wizard-strict-lane.md`](../../../specs/093-init-setup-wizard-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Define the setup-job inventory and the machine-readable checklist shape before
any prompt UI or execution orchestration lands.

## Work

- enumerate setup jobs with stable identifiers
- classify each job by applicability, safety, and execution type
- define `effigy init --checklist --json`
- encode enough metadata for later TTY and non-TTY execution

## Acceptance

- one shared setup-job inventory exists
- checklist JSON shape is explicit and bounded
- downstream wizard and action-execution work no longer needs to invent job IDs

## Evidence

- [`2026-05/19-120509-init-checklist-contract.md`](../../../logs/archive/2026-05/19-120509-init-checklist-contract.md)

## Next Task

Execute `1002`.
