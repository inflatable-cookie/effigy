# Demo Attached Terminal Run Mode Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.43`

## Summary

Made direct attached terminal sessions the default human path for text-mode
interactive and hybrid run-backed demos, while keeping runner-owned logs,
receipts, and active-session inspection honest.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `ROUTE`
- Moved from `the runner exposed active terminal session metadata and a machine
  forwarding contract, but human text-mode runs still had no contract-backed
  attached capture path` to `human interactive demo runs now attach directly to
  the live terminal while still feeding runner-owned logs, receipts, and active
  session inspection`
- Remaining open:
  - decide whether the next slice should deepen PTY/input semantics, browser
    terminal convergence, or demo-scoped tabs
  - keep nested TUI embedding and broader runtime expansion deferred

## Delivered

- made text-mode interactive and hybrid run-backed demos use an attached live
  terminal path by default
- kept stdout/stderr capture active during attached runs so log files,
  receipts, and history stay populated
- preserved live active-session inspection for attached text-mode runs
- kept `demo input` as secondary automation/client infrastructure
- updated help, changelog, roadmap/currentness surfaces, and opened the next
  ready boundary card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

Human demo runs now have a direct terminal path without giving up the runner
contract that the browser and later clients need to consume.

## Next Task

Execute [`050-decide-demo-post-attached-terminal-run-boundary.md`](../../specs/batch-cards/050-decide-demo-post-attached-terminal-run-boundary.md)
to choose the next bounded follow-up after attached terminal run mode lands.
