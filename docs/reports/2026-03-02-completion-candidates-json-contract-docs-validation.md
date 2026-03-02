# Completion Candidates JSON Contract Docs Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Document completion-candidates cache telemetry keys in the JSON contract guide.

## Changes
- Updated `017-json-output-contracts.md` with explicit `effigy.completion.candidates.v1` field semantics for:
  - cache hit/miss state keys
  - TTL and age diagnostics
  - TTL policy source variants (`default`, `env`, `env_invalid`)
  - manifest source count

## Validation
- command: `./scripts/check-docs-links.sh`
  - result: pass

## Evidence
- JSON contract guide now explicitly defines completion candidates cache telemetry fields and source semantics instead of requiring inference from examples.

## Risks / Follow-ups
- Guide lists field semantics but does not duplicate full payload examples; examples remain centralized in `026-json-payload-examples.md`.

## Next
- Add one short operator recipe in `021-quick-start-and-command-cookbook.md` for troubleshooting completion cache behavior using JSON fields.
