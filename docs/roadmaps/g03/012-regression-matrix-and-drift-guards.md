# 012 - Regression Matrix And Drift Guards

Generation: `g03`

Status: Planned
Owner: Platform
Created: 2026-05-01
Depends on: 007, 008, 009, 010, 011

## Problem

Effigy has already shown that “shared enough in spirit” is not a strong enough
guard against path drift.

Without parity tests and explicit drift guards, the same regression can return
under a different entrypoint and stay invisible until a consumer repo happens
to exercise it.

## Goal

Add one focused regression matrix and one set of drift guards that prove parity
by intent rather than by file ownership.

## Scope

- add fixture coverage for:
  - one Underlay-style repo
  - one DecodeLabs-style repo
  - one minimal dev-container repo
  - one host-only repo
- cover the required parity scenarios:
  - stopped runtime plus explicit container task
  - stopped runtime plus deferred container request
  - stopped runtime plus `effigy exec`
  - bootstrap `run` followed by shell handoff
  - managed `dev` shell exit
  - workspace shell exit after adopted runtime
  - run-array builtin repo targeting
  - Rhai repo targeting
  - inline workspace container support and failure parity
  - public gateway and alias parity
  - lease refresh parity
- add one audit test or equivalent guard that enumerates execution surfaces
  against the shared matrix
- add one guard preventing new duplicated embedded repo-targeting match blocks
- add one contract test for intentional unsupported-surface error-family parity

## Non-Goals

- replacing targeted tests with large end-to-end smoke suites
- widening the fixture matrix beyond what the convergence contract needs
- using docs-only assertions as the drift guard

## Exit Condition

This milestone is complete when the convergence lane has executable proof that:

- common lifecycle effects do not depend on caller path
- deliberate exceptions are visible and tested
- future split-path regressions are caught by contract-anchored tests instead
  of consumer breakage

## Next Task

Pause the convergence lane on a trustworthy boundary and return focus to the
active deployment-export queue unless a new parity break reopens it sooner.
