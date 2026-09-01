# Rhai Profile Limits Papercut Planning

Status: complete
Created: 2026-09-01
Roadmap: g08.039
Batch: 1094 planning

## Summary

- reconciled the portfolio papercut inventory against current Effigy `main`
- found no open PR or active implementation worker for the selected seam
- kept the Swallowtail graph timeout report in triage: current main already has
  a 120-second structured query bound, while progress and a different default
  need a product/JSON decision
- promoted only the self-contained Rhai debug/release parser-limit defect
- made card `1094` ready with exact limits, adversarial proofs, validation, and
  stop conditions

## Planning Decision

Preserve Rhai's current release expression envelope explicitly: global depth
`64`, function depth `32`. This relaxes only the debug-build parser from its
dependency default and avoids breaking scripts already supported in release.
Call-stack and every other runtime limit remain out of scope.

## Queue Hygiene

Normalized the six open Effigy `PAPERCUTS.md` headings and `Possible fix`
labels to the canonical parser format. No papercut meaning or status changed.

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`
- Movement: dependency-default profile drift -> one explicit, bounded repair
  lane with a falsifiable debug/release oracle
- Remaining gap: five other Effigy queue entries remain open; graph
  timeout/progress and catalog-pack design remain planning work

## Validation Performed

- current-main portfolio `papercuts --scope` inventory inspected
- open Effigy PRs: none
- active Effigy implementation workers: none
- exact engine construction and Rhai dependency defaults inspected
- `effigy qa:docs` passed, including links, indexes, workflow paths, and
  next-action validation
- current-main project papercut inventory reports six open entries and zero
  diagnostics after queue normalization
- `git diff --check` passed

## Next Task

Dispatch card `1094` through the committed worker handoff. After merge, return
to catalog-pack acquisition planning under contract `043`.
