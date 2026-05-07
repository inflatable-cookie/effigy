# 265 Implement Explicit Exec And Alias Surface

Status: landed
Updated: 2026-04-18
Roadmap: `g02.012`
Spec: `docs/specs/012-container-context-and-transparent-execution-strict-lane.md`

## Objective

Finish the visible `g02.012` product surface by adding explicit ad-hoc exec,
manifest exec aliases, CWD mapping, and container handoff behavior on top of
the routing foundation.

## Scope

- add manifest `exec` and `exec.aliases` support
- add `effigy exec ...` CLI and runner dispatch
- wire CWD mapping into routed container execution
- wire effigy-in-container detection and handoff vs raw exec strategy
- surface exec aliases through the task catalog and command resolution path

## Acceptance

- `effigy exec` works against the configured dev container
- exec aliases resolve from the manifest with clear error reporting
- routed commands preserve working directory semantics across host/container
- container detection chooses handoff or raw exec explicitly
- one real project can use the explicit exec surface after `264` lands

## Landed Outcome

- manifest `exec` and `exec.aliases` are now real product config
- `effigy exec` ships as a first-class CLI and runner surface
- routed container execution preserves CWD semantics through mapped working
  directories
- container execution chooses handoff vs raw exec explicitly, with recursion
  suppression and Colima direct-exec fallback on this machine
- `underlay-reference` proves the loop for explicit exec, alias fallback, and
  routed task execution

## Next Task

No further execution lives on this card. Return to planning and choose the next
bounded post-`g02.012` batch.
