# Docs Compaction Sweep

Status: complete
Created: 2026-09-05
Roadmap: none (Northstar docs cleanup route, operator-authorized)
Batch: docs-compaction-2026-09-05

## Summary
- Inventoried `docs/` recursively (every directory, anchor, non-Markdown
  file, handoff, and triage note) and classified each against the Northstar
  spine before editing.
- Compacted the session-loaded front doors and archived the closed `g08`
  log months, without deleting any content.

## Changes
- `docs/logs/2026-06/` and `docs/logs/2026-08/` moved under
  `docs/logs/archive/`; 112 inbound links across 73 files rewritten; the
  active log index now carries only `2026-09`.
- `docs/roadmaps/README.md`: the 27-bullet `g08` enumeration replaced by one
  summary paragraph (265 → 187 lines).
- `docs/roadmaps/generation-index.md`: per-milestone narrative replaced by
  one paragraph per generation plus rollover history (353 → 72 lines).
- `docs/specs/README.md`: the 25 archived-lane bullets rehomed into
  `docs/specs/archive/README.md` (110 → 60 lines).
- `docs/roadmaps/g01/README.md`: orphaned roadmap `026` linked.
- Triage `20260901-092640` owner corrected to chatterbox with a next check.
- `PAPERCUTS.md`: stale local-install binary fails `qa:docs` after a
  manifest-grammar change.

## Classification (unchanged, legitimate)
- `docs/audits/` project-specific audit prompts and reports, referenced from
  `g05`; `docs/guides/archive/` governed by guide `040`; `docs/research/`
  indexed by its README; `docs/contracts/fixtures/` and
  `docs/roadmaps/g07/*.toml` referenced by tests and roadmaps;
  `docs/scripts/fixtures/` owned by docs policy; `docs/policy/` canonical.

## Needs operator
- `docs/handoffs/`: 47 dispatch handoffs, 43 with no inbound reference and
  their lanes merged. Proposed: `docs/handoffs/archive/` for merged lanes,
  keeping the four legacy handoffs that roadmaps still cite.
- `docs/contracts/README.md` "Retained Contract Posture": one 60-line
  sentence naming which contracts are anchors versus historical. Proposed:
  rewrite as a two-column table; content-bearing, so not done unasked.
- `docs/specs/118`: stays in the active tree pending the deferred cohort
  decision.

## Vision Target Delta
- Primary tags: `MAINT`, `OPERATE`
- Movement: front doors enumerating closed history -> front doors summarising
  it; active log window = current generation only
- Remaining gap: handoff archive convention, contracts posture table

## Validation Performed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed (the PATH binary predated `[docs_policy.sources]`;
    see the papercut)
- command: `git diff --check`
  - result: clean

## Risks
- Links into `logs/2026-06` or `logs/2026-08` from outside `docs/` (none
  found in tracked files) would now need the `archive/` path.

## Next Task
- Operator decides the three needs-operator items above.
