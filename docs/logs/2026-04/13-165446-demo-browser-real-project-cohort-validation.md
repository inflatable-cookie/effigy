# Demo Browser Real-Project Cohort Validation

Date: 2026-04-13
Roadmap: `g02.003`
Card: [`081-validate-demo-browser-on-real-project-cohort.md`](../../specs/batch-cards/081-validate-demo-browser-on-real-project-cohort.md)

## Summary

Validated the shipped demo browser and live terminal flow against Signal as the
first real consumer repo, then stopped short of forcing a second repo proof
before release prep because no equally ready second consumer surfaced in the
same bounded window.

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `DEMO`
- Moved: `repo-local browser proof plus fixture confidence` -> `one real
  consumer repo using native Effigy demo registry, browser, history, and live
  terminal flow`
- Remaining: decide whether Signal-only proof is enough for release prep or
  whether one more real consumer batch is still required first

## Cohort

- validated consumer repo:
  [`signal`](</Users/betterthanclay/Dev/projects/signal>)
- second consumer repo:
  not completed in this batch

## Signal Validation Performed

- promoted Signal’s current demo pack into native Effigy demo registry entries
- extracted the Signal demo surface into a dedicated included manifest fragment:
  [`demos/effigy.demos.toml`](</Users/betterthanclay/Dev/projects/signal/demos/effigy.demos.toml>)
- removed per-demo wrapper tasks by letting demos carry inline `run = [ ... ]`
  sequences directly
- exercised shipped Effigy surfaces against Signal:
  - `PATH="$HOME/.local/bin:$PATH" effigy qa:docs`
  - `PATH="$HOME/.local/bin:$PATH" effigy demo list`
  - `PATH="$HOME/.local/bin:$PATH" effigy demo inspect plugin-capability-browser`
  - `PATH="$HOME/.local/bin:$PATH" effigy demo browser`
  - `PATH="$HOME/.local/bin:$PATH" effigy demo run runtime-recovery-inspector`
  - `PATH="$HOME/.local/bin:$PATH" effigy demo history runtime-recovery-inspector --limit 3`
- operator feedback during the browser proof drove and validated several real
  browser fixes:
  - terminal fidelity convergence onto shared concurrent-runner path
  - color rendering
  - stable browser layout
  - better left-pane row truncation and status alignment

## Outcomes

- Signal now proves that Effigy can own a real consumer demo registry instead
  of only repo-local fixture demos
- browser-driven interactive proof is now good enough to stop treating the demo
  browser as an unfinished experiment
- one remaining consumer gap stayed explicit:
  - no second equally ready real-project demo repo was completed in the same
    batch window

## Non-Effigy Consumer Findings

- Signal still has at least one local demo-script/runtime issue outside Effigy
  itself:
  - `runtime-recovery-inspector` currently trips a Python runtime mismatch on
    this machine because the local interpreter does not support `str | None`
- that issue does not invalidate the Effigy demo surface, but it does mean some
  consumer demos still need repo-local cleanup after migration

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`
- Signal commands listed above with the current local Effigy binary on PATH

## Outcome

Opened ready card
[`082-decide-demo-release-readiness-after-signal-proof.md`](../../specs/batch-cards/082-decide-demo-release-readiness-after-signal-proof.md).

## Next Task

- Execute `082-decide-demo-release-readiness-after-signal-proof.md`
