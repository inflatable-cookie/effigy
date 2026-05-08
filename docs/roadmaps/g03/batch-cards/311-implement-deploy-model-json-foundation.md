# 311 Implement Deploy Model JSON Foundation

Lane: `g03.001`
Status: archived
Updated: 2026-04-30

## Goal

Add the first bounded production-deployment runtime surface:

- `effigy deploy model --json`

for Underlay repos only.

## Scope

- add command parsing for `deploy model`
- require `--json` in the first batch
- derive `deploy.model.v1` from effective manifest and bundle state
- support Underlay only
- emit warnings using the new contract shape
- add one real proof against `underlay-reference`

## Non-Goals

- text-mode rendering
- `deploy export render`
- `deploy export railway`
- Decodelabs derivation
- provider templates

## Implementation Notes

- prefer one new deploy command module over scattering logic through old task
  dispatch paths
- reuse effective manifest/bundle resolution rather than peeking into runtime
  containers or compose output
- fail honestly when the repo is not an Underlay bundle consumer yet
- use the contract/example docs as the source of truth for the emitted payload

## Proof

- command resolves and emits `deploy.model.v1`
- `underlay-reference` payload shape matches the documented example closely
- warnings are structured and deterministic

## Exit Condition

This batch is complete when the JSON model surface is real for Underlay and the
repo has enough tests that provider-export work can build on it.

## Next Task

After `311`, stop and decide the next widening seam instead of rolling
straight into provider export.
