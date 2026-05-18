# g07.022 - Watch Backend And Debounce Rules

Status: Complete
Depends on: `g07.021`

## Goal

Land the first usable `graph watch` command with one watcher backend, one
debounce policy, and one clear event-to-index loop.

## Scope

- add `effigy graph watch`
- use `notify` as the watcher backend
- default debounce to `1s`
- coalesce repeated path events inside the debounce window
- emit concise text updates and structured JSON events

## Hard Boundaries

- no detach/service mode
- no watch-specific cache separate from the graph index
- no custom per-platform watcher stacks
- no event-triggered graph mutation that bypasses the normal index contract

## Acceptance

- watch mode starts, idles, and shuts down cleanly
- path bursts collapse into one index refresh
- delete, create, and change events all flow through one indexed refresh pass
- JSON mode has a typed, testable event shape

## Next Task

Execute `963`.
