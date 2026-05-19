# 1003 - Wire Setup Job Adapters And Safety Bounds

Roadmap: [`../053-setup-job-adapters-and-mutation-boundaries.md`](../053-setup-job-adapters-and-mutation-boundaries.md)
Strict lane: [`../../../specs/093-init-setup-wizard-strict-lane.md`](../../../specs/093-init-setup-wizard-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Attach the setup-job inventory to real Effigy surfaces and lock down mutation
rules before non-interactive action execution widens the lane.

## Work

- implement adapters for baseline, graph, secrets, bundle, migration, and
  validation jobs
- classify jobs by safe apply / contextual apply / inspect only / guidance only
- encode hard mutation boundaries for release, deploy, state, and distribution

## Acceptance

- every shipped setup job has one real adapter path
- unsafe mutation paths are excluded by contract, not by convention

## Evidence

- [`2026-05/19-124506-setup-job-adapters-and-safety-bounds.md`](../../../logs/2026-05/19-124506-setup-job-adapters-and-safety-bounds.md)

## Next Task

Execute `1004`.
