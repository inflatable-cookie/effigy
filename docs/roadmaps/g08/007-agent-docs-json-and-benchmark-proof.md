# g08.007 - Agent Docs, JSON, And Benchmark Proof

Status: Complete
Depends on: `g08.006`

## Goal

Make graph-aware scans usable by agents without encouraging ritual command
usage or over-trusting heuristic findings.

## Scope

- update scan command docs and the command reference
- update the Effigy skill with job-based guidance for graph-aware scans
- add JSON examples for each new or enriched scan family
- add fixture-backed benchmark or proof tasks that compare graph-aware scan
  output against expected findings
- include at least one optional live-repo proof path for Underlay or
  decodelabs-style repos, with clear skip behavior

## Agent Guidance Rules

The skill should teach:

- use graph-aware scans for risk, boundary, ownership, and validation
  questions
- use plain scans for cheap hygiene
- use `graph explore` for navigation
- use `rg` for exact-token proof
- do not run graph-aware scans as a startup ritual

## Guardrails

- do not make graph-aware scan guidance dominate unrelated Effigy features
- do not claim reduced tool calls without benchmark evidence
- do not make local private repos mandatory for tests
- do not bury warnings about stale or missing graph state

## Acceptance Criteria

- docs and skill route agents clearly by job
- JSON examples pass existing docs checks
- benchmark/proof output is deterministic for fixtures
- optional live repo proof is documented as optional

## Next Task

Start `g08.008`.
