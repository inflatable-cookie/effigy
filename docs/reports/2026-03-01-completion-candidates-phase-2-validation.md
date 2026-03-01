# Completion Candidates Phase 2 Validation

Date: 2026-03-01
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add dynamic selector candidate generation for completion (`effigy completion candidates`).
- Keep shell script generation backwards compatible while allowing runtime candidate probing.
- Lock JSON contract coverage for the new candidate payload.

## Changes
- Added `effigy completion candidates [--repo <path>] [--prefix <value>] [--json]`.
- Added JSON payload schema: `effigy.completion.candidates.v1`.
- Updated generated bash/zsh/fish completions to consult candidate probe output for first-token suggestions.
- Added tests for:
  - candidate JSON contract shape,
  - candidate text output for built-ins plus `<task>` and `<catalog>/<task>`,
  - bash script dynamic probe wiring,
  - candidate help/error handling.
- Updated docs + schema index + release smoke scripts to include candidate flow.

## Validation
- command: `cargo fmt`
  - result: pass
- command: `cargo test completion_ -- --test-threads=1`
  - result: pass
- command: `cargo test builtin_completion_candidates_ -- --test-threads=1`
  - result: pass
- command: `./scripts/check-json-contracts.sh --fast`
  - result: pass
- command: `cargo test render_help_writes_structured_sections -- --test-threads=1`
  - result: pass

## Risks / Follow-ups
- Candidate generation scans catalogs on-demand; very large workspaces may see completion latency spikes.
- Shell completions currently query local runtime only; no cache/memoization is added in this phase.

## Next
- Add lightweight candidate memoization for completion sessions (short TTL, workspace-root keyed) to reduce repeated scan cost in large repos.
