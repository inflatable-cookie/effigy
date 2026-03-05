# 015 Vision Decision Record Template v1

Status: Draft
Owner: Platform Lead
Purpose: standardize how strategy-level tradeoff decisions are captured and referenced.

## 1. Usage Scope

Use this template when:

1. A decision materially affects one or more vision tags.
2. A tradeoff between speed, reliability, and compatibility is non-trivial.
3. An exception request requires rationale and bounded follow-up.

## 2. Decision Record Template

```md
# Vision Decision Record — <D-YYYY-NN>

Context
- Date: <YYYY-MM-DD>
- Owner: <name/role>
- Scope: <repo(s)/capability>
- Tags: <ROUTE|CONTRACT|OPERATE|MAINT|RELEASE>

Decision
- Summary: <one sentence>
- Principle(s): <references from 008>
- Chosen Option: <brief>

Alternatives Considered
- Option A: <short description + why not chosen>
- Option B: <short description + why not chosen>

Impact
- Positive: <expected gains>
- Risk: <known downside>
- Compatibility Effect: <none|low|medium|high>

Controls
- Mitigation: <guardrails in place>
- Reversal Condition: <clear trigger>
- Exit Plan: <required completion criteria>

Traceability
- Related Exception: <VE-YYYY-NN or none>
- Related Risk: <VR-XX or none>
- Related Artifacts: <roadmap/report/doc links>
```

## 3. Quality Rules

1. Keep the summary specific enough to be auditable later.
2. Include at least one rejected alternative.
3. State explicit reversal condition; avoid open-ended language.
4. Link related exception/risk records when applicable.

## 4. Governance Integration

1. Reference decision records in governance reviews (`009`).
2. Use decision IDs in release/report notes where impact is visible.
3. Revisit active high-impact decisions quarterly until stabilized.

## Next Task

Define a minimal decision record index format so records remain discoverable and sortable by tag, owner, and status.
