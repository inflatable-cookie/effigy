# 059 Implement Demo Runtime Backend Capability Contract

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

This batch is superseded. It aimed at richer backend/capability metadata, but
operator feedback clarified that the real missing surface is embedded terminal
emulation with input, not another metadata-only contract pass.

## Why It Became Stale

- the current browser terminal tab is a log/metadata page, not a terminal
  emulator
- the user explicitly wants live terminal behavior with input where supported
- another metadata-first batch would delay the real interaction surface again

## Recovery Result

- backend/capability-only contract work is superseded for now
- the next slice is embedded demo-browser terminal emulation and input on top
  of the existing runner-owned session/input surfaces
- the no-nested-TUI rule remains in force

## Next Task

Execute [`060-implement-demo-browser-terminal-emulator.md`](./060-implement-demo-browser-terminal-emulator.md)
to replace the browser terminal log view with embedded terminal emulation and
input where the active runner session allows it, without launching a nested
TUI.
