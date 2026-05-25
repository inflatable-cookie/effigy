# g08.006 - Validation Gap And Hotspot Scans

Status: Complete
Depends on: `g08.005`

## Goal

Use graph data to find high-impact code with weak validation signals.

This scan should help agents choose tests after a change and help maintainers
identify risky ownership hotspots before they become release problems.

## Scope

- identify files or symbols with high connectivity
- identify changed files whose affected graph neighborhood has weak test
  adjacency
- surface likely tests and missing-test signals separately
- reuse existing `graph affected` and `explore` packet logic where practical
- support both full-repo hotspot scans and changed-file validation scans

## Candidate Finding Types

- `hotspot_without_nearby_tests`
- `changed_owner_without_test_target`
- `central_wiring_without_contract_test`
- `high_blast_radius_attention_marker`

## Guardrails

- do not require a repo to have standard test naming to avoid failure
- do not invent test recommendations without evidence
- do not treat missing test adjacency as proof of missing coverage
- avoid making release gates depend on noisy heuristic findings

## Acceptance Criteria

- fixture repo proves a central untested owner and a central tested owner
- changed-file mode can read paths from stdin or existing affected machinery
- JSON includes graph facts, likely tests, and confidence
- docs tell agents how to use this scan for validation narrowing

## Next Task

Start `g08.007`.
