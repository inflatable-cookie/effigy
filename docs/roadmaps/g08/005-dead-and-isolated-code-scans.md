# g08.005 - Dead And Isolated Code Scans

Status: Complete
Depends on: `g08.004`

## Goal

Add graph-backed scans for likely unused, isolated, or orphaned code.

The scan must be honest: graph indexing is heuristic and language coverage is
partial, so findings should say "likely" and provide evidence rather than
pretending to prove compiler-grade dead code.

## Scope

- identify files or symbols with no inbound references
- distinguish public entrypoints, tests, scripts, docs, config, and generated
  files from likely orphaned implementation code
- support allowlists or suppressions for intentional entrypoints
- report confidence and reason fields
- avoid noisy findings in languages or file types with weak graph coverage

## Candidate Finding Types

- `isolated_file`: indexed source file with no meaningful graph neighbors
- `unreferenced_symbol`: symbol with no inbound calls/references
- `orphaned_entrypoint_candidate`: file that looks executable or exported but
  is not connected to manifests, routes, tasks, imports, or tests
- `unused_public_surface_candidate`: exported symbol with no known consumers

## Guardrails

- no failure by default on low-confidence findings
- no claims for unsupported languages unless graph coverage is explicit
- no noisy findings for tests, fixtures, migrations, generated code, or docs
  unless requested
- provide suppression paths before enabling strict mode

## Acceptance Criteria

- fixture repo proves true isolated code, intentional entrypoints, and test
  fixture exclusions
- output includes confidence and graph evidence
- docs explain limits and safe review workflow
- agents can use the scan to choose inspection targets without deleting code
  blindly

## Next Task

Start `g08.006`.
