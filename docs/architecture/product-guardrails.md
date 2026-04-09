# Effigy Product Guardrails

Effigy is an operator-first task runner and automation surface.

These guardrails define what must stay true while the product expands.

## Product Guardrails

- Keep one stable operator entry surface. New capability should strengthen
  `effigy` as the obvious place to ask for work instead of reopening wrapper
  sprawl or repo-local folklore.
- Prefer built-ins and manifest contracts over ad hoc shell orchestration when
  the workflow is broadly reusable.
- Do not hide meaningful state behind machine-global config when repo-local or
  cwd-relative behavior can stay explicit.
- Favor deterministic routing, explainability, and failure clarity over clever
  fallback behavior.
- Keep JSON and text output aligned to the same underlying facts; machine
  safety is not a second-class mode.
- Do not let release, bootstrap, or validation flows drift into opaque
  side-channel scripts when Effigy can own the surface directly.
- Keep consumer-repo adoption honest. If a repo needs more structure, improve
  the Effigy contract or docs rather than teaching more folklore.
- Treat polish or verification work as real product work only when it changes
  adoption confidence, operator clarity, or release readiness. Avoid endless
  “one more pass” churn.

## Anti-Patterns

- adding new command surfaces without a clear routing or adoption reason
- letting handoff notes become the only source of queue authority
- claiming release readiness without explicit evidence and gates
- widening a focused built-in or migration lane into general workspace
  management by accident
- solving downstream friction with custom scripts when the issue belongs in the
  manifest, built-ins, or docs contract

## Next Task

Keep the active bootstrap and consumer-adoption work inside these guardrails,
using them to decide whether the next step is release preparation, another
proof wave, or a narrower product-boundary batch.
