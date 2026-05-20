# g07.060 - JSON Help Contract Consistency Cleanup

Status: Complete
Depends on: `g07.059`

## Goal

Reduce drift in agent-facing output surfaces: JSON reports and CLI help.

## Evidence

The audit found mixed conventions:

- init uses shared schema rendering helpers
- release has custom JSON renderers
- distribution builds direct `json!` schema payloads
- help topic files share rendering infrastructure but still duplicate large
  option/example fragments

## Scope

- document the preferred JSON/report rendering convention for command families
- migrate one or two high-value command families only where the diff is low
  risk
- extract shared help topic fragments for repeated option/example blocks
- preserve rendered help output unless wording is intentionally improved
- add tests or snapshots where output stability matters

## Guardrails

- no giant universal report enum
- no broad rewrite of release/distribution JSON in one pass
- no CLI wording churn without a clear user-facing reason
- no breaking JSON field rename
- no hidden schema version change

## Suggested Implementation Shape

- add small shared helpers or documented patterns near existing CLI/report code
- start with repeated help fragments because they are low risk
- audit distribution JSON construction for the most duplicated schema wrapper
- leave release-specific typed renderers in place unless a common helper makes
  them clearer

## Acceptance Criteria

- help duplicate blocks are reduced without losing readable topic files
- JSON rendering conventions are documented or encoded in helper APIs
- migrated command output has focused regression coverage
- agents still receive stable `--json` payloads

## Next Task

No active ready card.
