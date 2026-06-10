# Demo Runtime Backend Capability Contract Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Card: `064-implement-demo-runtime-backend-capability-contract`

## Summary

Implemented bounded runner-owned runtime backend and capability reporting for
demo inspect surfaces so active demo attempts and active terminal sessions can
project honest demo-scoped backend facts without nested TUI launch.

## What Shipped

- added `runtime_backend` reporting to demo detail payloads
- added `runtime_backend` reporting to active-attempt and active-terminal-
  session payloads
- distinguish current task-backed and run-backed demos through one bounded
  backend label/capability contract
- added legacy active-record inference so old persisted state still resolves to
  honest backend facts
- updated browser-side JSON consumers and contract tests

## Validation

- `cargo test cli_demo_inspect_json_reports_latest_attempt_and_sources -- --nocapture`
- `cargo test cli_demo_inspect_json_reports_active_attempt -- --nocapture`
- `cargo test load_active_attempt_ -- --nocapture`
- `cargo test browser_ -- --nocapture`
- full batch validation still required before closeout commit

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `ROUTE`
- Moved: `terminal/session facts only -> explicit backend identity and capability reporting for demo inspect surfaces`
- Remaining: complete full validation and decide the next bounded slice after backend/capability reporting
