# 520 - Audit Rhai Host Surface And Scaffold Lane

Lane: [`048-rhai-host-api-split-and-callback-purity-strict-lane.md`](../048-rhai-host-api-split-and-callback-purity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Open `g04.006` implementation with a concrete Rhai host surface inventory and
the first split target.

## Scope

- inventory `crates/effigy-rhai/src/host_api.rs` by module surface
- identify runtime-sensitive callbacks that must route through execution or
  container operation requests
- choose the first low-risk module split
- update the lane execution chain with the selected implementation card
- no implementation code beyond planning docs

## Non-Goals

- no Rhai public API changes
- no callback behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the first bounded `g04.006` implementation card is
ready.

## Host Surface Inventory

Current file sizes:

- `crates/effigy-rhai/src/host_api.rs`: 2164 lines
- `crates/effigy-rhai/src/lib.rs`: 1126 lines
- `src/runner/script_command/mod.rs`: 1102 lines

`host_api.rs` owns these module builders:

- pure or mostly pure utility modules: `time`, `path`, `json`, `toml`, `str`,
  `random`
- file/process modules: `fs`, `process`, `http`, `search`
- execution/runtime modules: `runtime`, `exec`
- Effigy callback modules: `config`, `task`, `container`, `scan`, `docs`,
  `deploy`, `system`, `demo`, `changelog`, `cache`, `gateway`, `bundle`,
  `service`, `catalog`, `doctor`, `contracts`, `unlock`, `test`, `effigy`

Runtime-sensitive callback surfaces:

- `exec::run(...)` already builds a `TaskExecutionRequestBuilder` and routes
  host/container choice through a resolved execution plan.
- `container::exec(...)` still calls host callbacks directly and should become
  a compatibility wrapper over the same execution/container operation request
  shape.
- `container::shell/up/down/status/logs/reset/stats/data/eject` use callback
  feature dispatch and should be checked against `ContainerOperationRequest`
  ownership as modules split.
- `task::*` and high-level `effigy::*` dispatch through callback feature paths;
  they should remain thin request adapters.

First split target:

- split pure utility module builders first: `time`, `path`, `json`, `toml`,
  `str`, and `random`
- keep `register_host_api` in `host_api.rs`
- expose one internal `utility` module builder registration function
- avoid behavior changes and callback work in the first implementation slice

## Closeout

Opened the Rhai lane and selected a low-risk pure utility module split as the
first implementation card. Runtime-sensitive callback cleanup remains queued
behind the initial module extraction.

## Validation

- docs/front-door consistency check passed

## Next Task

Start card
[`521-split-rhai-pure-utility-host-modules.md`](./521-split-rhai-pure-utility-host-modules.md).
