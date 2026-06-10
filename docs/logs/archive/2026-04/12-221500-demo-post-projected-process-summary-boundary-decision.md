# Demo Post-Projected-Process-Summary Boundary Decision

Status: complete
Created: 2026-04-12
Roadmap: g02.003
Batch: 077-decide-demo-post-projected-process-summary-boundary

## Summary
- Chose one more runner-owned concurrent-runtime truth slice.
- Did not widen back into browser churn.
- Did not pause the lane yet.

## Changes
- decided the next bounded slice is projected output provenance truth for
  flattened concurrent-runner demos
- recorded that browser follow-up still should not invent multi-process meaning
  until output provenance is explicit in the runner contract
- opened ready card `078-implement-demo-concurrent-runtime-projected-output-provenance-contract.md`

## Vision Target Delta
- Primary tags: `demo`, `runner`, `concurrent-runtime`, `browser-boundary`
- Movement: baseline `projected process names and merge truth only` -> current `next runner truth slice chosen: output provenance`
- Remaining gap: projected concurrent demos still flatten output without an
  explicit contract for how source attribution survives that merge

## Validation Performed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- browser consumers still cannot present richer projected output honestly until
  `078` lands

## Next Task
- Execute `078-implement-demo-concurrent-runtime-projected-output-provenance-contract.md`.
