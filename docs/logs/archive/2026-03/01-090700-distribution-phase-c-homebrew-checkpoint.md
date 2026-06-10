# Distribution Phase C Homebrew Checkpoint

Date: 2026-03-01
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/distribution-channels.md`

## Scope

- Complete Distribution backlog Phase C documentation in one batch:
  - tap/formula workflow
  - release-tag automation approach
  - bottle/checksum/update strategy
- Update backlog and release checklist references.

## Changes

- Added Homebrew workflow guide:
  - `docs/guides/042-homebrew-tap-and-release-automation.md`
- Updated release checklist to include tap automation evidence requirement:
  - `docs/guides/014-release-checklist-template.md`
- Updated guides navigation:
  - `docs/guides/README.md`
- Marked Distribution backlog Phase C and Homebrew deliverable complete:
  - `docs/roadmaps/backlog/distribution-channels.md`

## Validation

- command: `./scripts/check-doc-links.sh README.md $(find docs -name '*.md' | sort)`
  - result: pass

## Outcomes

- Homebrew channel operational policy is now documented end-to-end.
- Release owners have explicit tag-triggered formula bump expectations.
- Checksum/update strategy is defined with rollback fallback.

## Risks / Follow-ups

- Tap automation implementation in CI still requires repository wiring (workflow secrets/permissions and tap PR bot path).
- Crates.io install validation remains pending first publish cycle.
- Phase E wrapper reassessment is still open and should be done after observing channel stability.

## Next Batch Recommendation

- Execute **Distribution Phase E wrapper evaluation**:
  - decide whether npm/thin wrapper remains necessary after crates + Homebrew stabilization
  - if retained, define strict delegation policy and maintenance limits
  - if removed, add deprecation/removal migration note and checklist
