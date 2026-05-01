# 011 - Embedded Command, Script, And Bootstrap Convergence

Generation: `g03`

Status: Active
Owner: Platform
Created: 2026-05-01
Depends on: 008, 009, 010

## Problem

Effigy still re-enters itself through several internal paths that each carry
their own partial assumptions.

The main surfaces are:

- bootstrap task dispatch
- bootstrap managed-run dispatch
- Rhai command execution
- run-array builtin execution
- other internal command replay helpers

That creates too many places where nested command behavior can drift in repo
targeting, JSON handling, recursion rules, locking, or output shaping.

## Goal

Create one shared embedded-runner entry for normal internal Effigy command
re-entry.

## Scope

- add one shared embedded-runner API that owns:
  - repo targeting
  - global JSON flag handling
  - recursion and handoff rules
  - locking expectations
  - nested output projection expectations
- move bootstrap, Rhai, and run-array builtin command re-entry onto that path
- remove synthetic path-specific behavior unless it is deliberate, documented,
  and tested

## Non-Goals

- redesigning bootstrap planning itself
- changing Rhai host-surface scope beyond Effigy command re-entry
- generic parser rewrites

## Exit Condition

This milestone is complete when one shared embedded command path exists and is
used by bootstrap, Rhai, and run-array surfaces unless a documented exception
still applies.

## Next Task

Execute the post-foundation widening decision.

Once the shared path is real, decide whether one more widening slice is needed
before the regression-matrix lane in `g03.012`.
