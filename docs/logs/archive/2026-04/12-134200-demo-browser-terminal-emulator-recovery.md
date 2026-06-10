# Demo Browser Terminal Emulator Recovery

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`059-implement-demo-runtime-backend-capability-contract.md`](../../../specs/batch-cards/059-implement-demo-runtime-backend-capability-contract.md)

## Summary

Recovered the active ready-card chain after operator feedback showed that the
live gap is embedded terminal emulation and input, not another metadata-only
session contract batch.

## Vision Target Delta

- move from `runner metadata depth is the next likely gap` toward `browser
  terminal interaction is still the missing product surface`
- keep the lane demo-scoped and avoid nested-TUI drift
- remaining gap: embedded terminal emulation and input on the selected demo's
  terminal tab

## Recovery

- marked `059` superseded
- replaced the ready implementation target with embedded browser terminal
  emulation and input
- preserved the no-nested-TUI rule and the existing runner-owned session/input
  authority chain

## Outcome

Opened ready card [`060-implement-demo-browser-terminal-emulator.md`](../../../specs/batch-cards/060-implement-demo-browser-terminal-emulator.md).

## Next Task

Execute [`060-implement-demo-browser-terminal-emulator.md`](../../../specs/batch-cards/060-implement-demo-browser-terminal-emulator.md)
to replace the browser terminal log view with embedded terminal emulation and
input where the active runner session allows it.
