# 018 - V1 Runtime Hardening Proof And Stress Matrix

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-02
Depends on: 013, 014, 015, 016, 017

## Problem

Even after structural cleanup, Effigy will not feel `v1.0`-grade unless the
 runtime/container core has direct proof against the failure seams that have
 historically felt brittle.

## Goal

Add one bounded proof and stress matrix that closes the hardening program on
 evidence, not just refactor completion.

## Scope

- define and automate a targeted runtime/container proof matrix for:
  - bootstrap setup plus shell handoff
  - lease and no-lease ownership modes
  - direct workspace sessions versus seeded task shells
  - explicit `exec` versus routed task activation
  - runtime reuse and cleanup ownership
  - gateway and alias reconciliation
  - external mounts and workspace host integration
  - shared service env/binding behavior
  - representative DecodeLabs and Underlay runtime flows
- add final drift guards where the earlier convergence and hardening work
  could silently split again
- define the runtime/container acceptance bar for “`v1.0`-grade enough”

## Non-Goals

- adding new runtime/container features
- broad performance work beyond issues exposed by the proof matrix
- widening deploy/provider work

## Exit Condition

This milestone is complete when:

- the runtime/container hardening matrix is executable and bounded
- the main historical brittleness seams have direct regression coverage
- the closeout states the explicit acceptance bar for the runtime/container
  core entering `v1.0`

## Next Task

Stop in planning and choose the next `v1.0` program deliberately.
