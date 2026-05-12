# 048 - Rhai Host API Split And Callback Purity Strict Lane

Roadmap: [`g04.006`](../roadmaps/g04/006-rhai-host-api-split-and-callback-purity.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Purpose

Make Rhai host APIs modular and route runtime-sensitive work through typed
pipeline requests.

`g04.005` moved seed/dump data decisions into `effigy-data`. The next pressure
point is Rhai: `host_api.rs` is still a large mixed-ownership file, and
container-sensitive callbacks must stay aligned with the execution, runtime,
container-operation, and data pipelines rather than calling local helper paths.

## Hard Boundaries

- preserve current Rhai public function names unless a card explicitly selects
  a cleanup break
- route runtime-sensitive callbacks through typed request/plan surfaces
- keep conversion helpers separate from side-effect callbacks
- no release work
- no `.github/workflows/` edits

## Current Ready Card

None. This lane is complete.

## Execution Chain

- `519` complete: close data seed/dump pipeline and open Rhai lane
- `520` complete: audit Rhai host surface and scaffold lane
- `521` complete: split Rhai pure utility host modules
- `522` complete: split Rhai filesystem host module
- `523` complete: split Rhai process, HTTP, and search host modules
- `524` complete: split Rhai feature callback host modules
- `525` complete: split Rhai container host module
- `526` complete: split Rhai exec host module and review callback purity
- `527` complete: route Rhai container exec callback through operation surface
- `528` complete: close Rhai host API split and callback purity

## Exit Condition

This lane closes when Rhai host registration is module-owned, no Rhai host
module file is over 500 lines, and container-sensitive callbacks route through
the execution/container operation request surfaces instead of direct runner
helper paths.

## Next Task

Roadmap `g04.007` starts with card
[`529-scaffold-effective-container-policy-decomposition-lane.md`](./batch-cards/529-scaffold-effective-container-policy-decomposition-lane.md).
