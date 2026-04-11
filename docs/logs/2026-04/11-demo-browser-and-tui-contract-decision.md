# Demo Browser And TUI Contract Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.4`

## Summary

Locked the first browser contract for Effigy's future demo TUI.

The browser is now defined as a runner-backed operator surface rather than a
project-specific app shell:

- sidebar/list as the primary navigation surface
- explicit grouping and filtering dimensions
- status and gap badges driven by declared runner/coverage data
- minimum drilldown for logs, receipts, artifacts, and proof intent

## Decisions

- the first client is a TUI browser, not a bespoke desktop app
- the browser consumes explicit runner and registry data rather than inferring
  status or coverage from file layout
- list rows must carry enough metadata to answer what the demo is, whether
  proof exists, and whether it is healthy, stale, broken, planned, or missing
- drilldown must expose latest receipt summary, artifact references, logs, and
  runnable entrypoint context without requiring rich artifact rendering

## Follow-On

The next batch is Signal reconciliation. The goal is to check the settled demo
contract against the real motivating pilot before opening implementation
planning.
