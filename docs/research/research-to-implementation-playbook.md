# Research-to-Implementation Playbook

Status: Active
Last updated: 2026-03-07
Purpose: Ensure Effigy research findings actively inform implementation instead of remaining isolated in the research corpus.

## The Problem

Research only matters if it changes what gets built.
This playbook keeps architecture, guides, roadmaps, implementation, and review tied back to the research corpus.

## Workflow: Research-Aware Delivery

### Phase 1: Discovery

1. Identify the delivery area, architecture doc, or guide the task belongs to.
2. Check `master-index.md` for the relevant memo, value track, and tool dossiers.
3. Read the translation memo before making implementation choices.
4. Confirm whether the memo outcome is validated enough for direct implementation or still needs prototype-style validation.

### Phase 2: Decision

Before coding, record:
- which research artifacts were consulted
- which recommendations are being followed directly
- which recommendations are being deferred or rejected
- which open questions still need validation

Use `templates/implementation-decision-record.md` when the choice should remain durable.

### Phase 3: Implementation

- Reference the research basis in code comments when behavior intentionally follows a researched pattern.
- If implementation uncovers a missing research area, add it to `gaps-found-during-implementation.md`.
- If code needs to deviate from the research, document the rationale instead of silently drifting.

### Phase 4: Validation

- Derive tests or validation checks from research-backed behavior claims where practical.
- Treat draft memos as guidance, not settled contract, unless the relevant docs have already promoted the decision.
- Record validation performed in the roadmap batch log or repo QA notes.

### Phase 5: Review

Reviewers should check:
- the task consulted the right memo and supporting research
- deviations are documented and justified
- missing research was captured as a gap
- draft or prototype-gated recommendations were not treated as settled without explanation

## Effigy-Specific Starting Points

| If you are building... | Start with... |
| --- | --- |
| manifest syntax, schema, config parsing | Memo 001 + Track 01 |
| cache behavior or invalidation | Memo 002 + Track 02 |
| watch mode internals | Memo 003 + Track 03 |
| DAG or execution planning | Memo 004 + Track 04 |
| TUI or process output behavior | Memo 005 + Track 05 |
| shell completions | Memo 006 + Track 06 |
| diagnostics or doctor output | Memo 007 + Track 07 |
| workspaces or catalog discovery | Memo 008 + Track 08 |
| portability or env resolution | Memos 009-010 + Tracks 09-10 |

## Lightweight Checklist

- [ ] I checked `master-index.md`.
- [ ] I read the relevant translation memo.
- [ ] I know whether the relevant docs already promote this decision or still treat it as draft guidance.
- [ ] I documented major decisions or deviations if needed.
- [ ] I captured any missing research in `gaps-found-during-implementation.md`.

## When Research Is Missing

1. Do a quick targeted scan if the answer is likely to be cheap to find.
2. Record the gap when the answer is still unclear.
3. Make the provisional decision explicit.
4. Queue deeper research or roadmap follow-up if the risk is material.

## Next Task

Use this playbook on the next implementation batch that touches caching, watch mode, or diagnostics, then trim any steps the Effigy team never actually uses.
