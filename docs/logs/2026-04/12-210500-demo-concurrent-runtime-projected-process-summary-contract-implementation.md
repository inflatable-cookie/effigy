# Demo Concurrent Runtime Projected Process Summary Contract

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `076`

## Summary

Projected concurrent-runner demos now expose bounded process summary truth
through the demo contract. Clients can see which managed process names sit
behind one flattened demo-owned terminal/session and whether that projected
surface is merging output from multiple processes.

## Changes

- added runner-owned `projected_process_summary` reporting under
  `runtime_backend` for:
  - demo detail
  - active attempt
  - active terminal/session
- the summary now reports:
  - `present`
  - `managed_process_names`
  - `merged_output_from_multiple_processes`
- persisted active-attempt records now carry bounded managed process names for
  concurrent-runner demos
- added regression coverage for:
  - inactive single-process concurrent summary
  - inactive multi-process projected summary
  - active single-process projected summary
  - active multi-process projected summary

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `ROUTE`
- Moved from `projection shape only` to `projection shape plus bounded
  projected-process summary truth across inspect and active terminal/session
  payloads`
- Remaining open:
  - decide whether the next slice should deepen concurrent-runtime truth
    again, add one bounded browser follow-up, or pause this branch
