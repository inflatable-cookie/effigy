# 385 - Migrate Exec Copy Data Through Manager

Lane: [`038-plugin-ready-container-manager-facade-strict-lane.md`](../038-plugin-ready-container-manager-facade-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Move remaining exec, copy, and data container operation branching behind
`ContainerManager`.

## Scope

- migrate `src/runner/exec_command/transport.rs`
- migrate `src/runner/container_command/support.rs` shared compose and runtime
  volume helpers
- migrate data import/export/pull paths where they shell into services
- keep public CLI behavior unchanged
- add focused tests for Docker and Colima invocation parity
- add an `rg` drift check for direct runner `resolve_compose_backend()` use

## Exit Condition

This card is complete when runner exec/copy/data paths use manager-owned
backend selection and remaining direct backend branching is contained inside
manager or temporary compatibility wrappers.

## Next Task

Decide whether `g03.031` can close or needs one final attached-interrupt
cleanup/report card.
