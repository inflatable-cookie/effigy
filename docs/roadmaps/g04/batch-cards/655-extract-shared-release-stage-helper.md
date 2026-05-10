# 655 - Extract Shared Release Stage Helper

Roadmap: [`../026-shared-dispatcher-and-exec-collapse.md`](../026-shared-dispatcher-and-exec-collapse.md)
Strict lane: [`../../../specs/069-shared-dispatcher-and-exec-collapse-strict-lane.md`](../../../specs/069-shared-dispatcher-and-exec-collapse-strict-lane.md)
Contract: [`../../../contracts/024-shared-dispatcher-and-exec-collapse-contract.md`](../../../contracts/024-shared-dispatcher-and-exec-collapse-contract.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Purpose

Collapse the repeated `prepare`/`execute` control-flow shape in the release
command behind one shared stage helper.

## Scope

- extract the common `--plan` / `--yes` / json failure flow for release stages
- keep stage-specific side effects outside the shared helper
- preserve all current release text, json, and mutation behavior

## Acceptance

- release prepare/execute share one bounded internal stage helper
- stage-specific side effects stay explicit
- focused release command proofs stay green
