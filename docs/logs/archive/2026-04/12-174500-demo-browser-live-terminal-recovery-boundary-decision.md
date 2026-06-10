# Demo Browser Live Terminal Recovery Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`069-decide-demo-post-concurrent-runner-terminal-interaction-boundary.md`](../../../specs/batch-cards/069-decide-demo-post-concurrent-runner-terminal-interaction-boundary.md)

## Summary

Recovered the browser terminal authority chain after operator testing showed
the shipped `Terminal` tab was still a replay/input consumer rather than a
browser-owned live attached terminal session.

## Decision

- treat the shipped browser terminal as vt-backed replay/input consumption, not
  the final live-session answer
- do not take more runner-only concurrent-runtime fidelity next
- do not pause terminal/browser work yet
- next slice is browser-owned live attached terminal sessions for
  browser-launched run-backed interactive demos
- keep concurrent-runner-backed demos on the flattened projected path for now
- keep the no-nested-TUI rule

## Why This Is The Right Boundary

- the operator ask is direct: the demo should run in that pane, with live
  output and interactive input there
- replaying logs plus forwarding keys is terminal-shaped, but still not the
  same product surface
- more runner-only work would dodge the actual mismatch
- trying to solve all backends at once would widen the batch too far and crash
  into the no-nested-TUI boundary

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `DEMO`
- Moved: `browser terminal story overstated -> authority chain recovered around live attached browser session as the next slice`
- Remaining: implement browser-owned live attached terminal sessions for browser-launched run-backed interactive demos

## Next Task

- Execute `070-implement-demo-browser-live-attached-terminal-session.md`
