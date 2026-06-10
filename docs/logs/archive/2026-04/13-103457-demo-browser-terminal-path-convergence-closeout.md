# Demo Browser Terminal Path Convergence Closeout

Date: 2026-04-13
Roadmap: `g02.003`
Card: [`080-implement-demo-browser-terminal-path-convergence.md`](../../../specs/batch-cards/080-implement-demo-browser-terminal-path-convergence.md)

## Summary

Closed the recovery-backed browser terminal convergence batch after the browser
live terminal stopped relying on a browser-only near-copy path and became
trustworthy enough for real-project validation.

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `DEMO`
- Moved: `browser live terminal fidelity churn and recovery` -> `shared-path browser live terminal with stable layout, color, and trustworthy rendering`
- Remaining: validate the shipped demo browser and terminal flow on at least
  two real consumer projects before release

## Delivered

- converged the browser live terminal onto shared concurrent-runner terminal
  session/render pieces instead of another browser-local approximation
- fixed terminal-fidelity bugs that were corrupting live output:
  - UTF-8 split-chunk handling
  - LF-to-CRLF normalization before VT ingest
  - terminated-stop classification
  - ANSI 256-color and truecolor decode
- kept browser behavior coherent while the terminal path stabilized:
  - stable `28/72` split
  - launch-to-terminal visibility without stealing left-pane focus
  - cleaner browser header/list summary layout
- validated the exact JSON-contract CI workflow path locally:
  - `contracts check-json --full --print-selected=json`
  - selection-artifact extraction and validation
  - negative validator smoke test

## Validation

- `cargo run --bin effigy -- qa`
- `cargo run --bin effigy -- contracts check-json --full --print-selected=json`
- `cargo run --bin effigy -- contracts validate-selection --artifact /tmp/json-contracts-selected.json`
- `cargo test --test cli_output_tests cli_contracts_validate_selection_rejects_invalid_artifacts -- --nocapture`
- `git diff --check`

## Outcome

Opened ready card [`081-validate-demo-browser-on-real-project-cohort.md`](../../../specs/batch-cards/081-validate-demo-browser-on-real-project-cohort.md).

## Next Task

- Execute `081-validate-demo-browser-on-real-project-cohort.md`
