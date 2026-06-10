# Documentation IA Completion Report

Date: 2026-03-01  
Owner: documentation maintenance
Related roadmap: docs information architecture and readability sweep

## Scope

- Complete deduplication and ordering optimization across docs entry points and guides.
- Standardize terminology usage (`JSON mode`, `selector`, `routing`, `deferral`).
- Normalize guide endings for operational/process runbooks.

## Changes

- Reworked entry-point navigation and reading paths:
  - `README.md`
  - `docs/README.md`
  - `docs/guides/README.md`
- Deduplicated workflow/example guides and clarified ownership boundaries:
  - `021` quick start as onboarding hub
  - `023` symptom-driven troubleshooting
  - `024` canonical CI/automation recipes
  - `025` canonical command matrix
  - `027` manifest snippets only
  - `028` migration decision paths only
- Normalized core runtime guides (`010`-`020`) with consistent close-out sections:
  - `Related Guides`
  - `Next Step`
- Normalized process/operations guides (`030`-`037`) with consistent structure:
  - `Expected Outcome`
  - `Related Guides`
  - `Next Step`
- Applied terminology canon and wording cleanup in style/glossary and operational docs:
  - replaced `machine mode` phrasing with `JSON mode`
  - aligned selector/routing/deferral definitions
  - converted plain related-guide path text to markdown links where needed

## Dedup Decisions

- Keep command syntax reference canonical in `025`; other guides link to it.
- Keep CI script/workflow details canonical in `024`; remove duplicated CI snippets from `027` and `028`.
- Keep troubleshooting fixes canonical in `023`; keep `021` focused on first-run and daily baseline commands.
- Keep migration branching logic in `028`; link out for implementation detail.

## Validation

- command: `./scripts/check-doc-links.sh README.md $(find docs -name '*.md' | sort)`
  - result: passed after each major docs batch and final synchronization pass

## Outcomes

- Reduced repeated command blocks across guides.
- Improved onboarding-to-operations reading sequence.
- Improved consistency for terminology and section structure.
- Preserved discoverability through index updates and cross-links.

## Risks / Follow-ups

- `docs/guides/038-docs-ia-snapshot.md`, `039-docs-drift-monitoring.md`, and `040-docs-archive-and-deprecation-policy.md` were outside the structural normalization batch; include them in the next drift audit if they evolve.

## Next

- Use `039-docs-drift-monitoring.md` to schedule periodic checks for:
  - duplicate command examples reappearing,
  - index ordering drift,
  - terminology regressions against `033`/`034`.
