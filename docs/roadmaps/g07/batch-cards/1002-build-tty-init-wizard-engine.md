# 1002 - Build TTY Init Wizard Engine

Roadmap: [`../052-tty-init-wizard-engine-and-prompt-flow.md`](../052-tty-init-wizard-engine-and-prompt-flow.md)
Strict lane: [`../../../specs/093-init-setup-wizard-strict-lane.md`](../../../specs/093-init-setup-wizard-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Make plain TTY `effigy init` interactive while preserving current
non-interactive semantics.

## Work

- add TTY detection and mode split
- build phase-based yes/no prompt flow
- consume the shared setup-job inventory
- keep `--json`, `--check`, `--checklist`, named starters, and non-TTY flows
  deterministic

## Acceptance

- plain TTY init prompts
- non-TTY init does not prompt
- prompts stay bounded and contextual

## Evidence

- [`2026-05/19-122703-tty-init-wizard-engine.md`](../../../logs/2026-05/19-122703-tty-init-wizard-engine.md)

## Next Task

Execute `1003`.
