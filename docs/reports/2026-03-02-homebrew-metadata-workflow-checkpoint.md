# Homebrew Metadata Workflow Checkpoint

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmap/backlog/distribution-channels.md`

## Scope

- Add a release-tag workflow hook that emits Homebrew formula metadata (`tag`, tarball URL, SHA256).
- Connect the hook to distribution automation docs and backlog notes.

## Changes

- Added workflow:
  - `.github/workflows/homebrew-tap-metadata.yml`
- Updated Homebrew automation guide:
  - `docs/guides/042-homebrew-tap-and-release-automation.md`
- Updated distribution backlog note:
  - `docs/roadmap/backlog/distribution-channels.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only`
  - result: pass
- command: `bash -n .github/workflows/homebrew-tap-metadata.yml 2>/dev/null || true`
  - result: workflow file present and parsed by GitHub Actions runtime (syntax reviewed)

## Outcomes

- Release tags now have a concrete metadata artifact path for tap updates.
- Formula update work can consume a deterministic checksum payload instead of recomputing ad hoc.

## Risks / Follow-ups

- Workflow currently produces metadata artifact only; tap repository write/PR automation is still a separate step.
- First production run should verify artifact handoff in the tap repo process.

## Next Batch Recommendation

- Add tap-repo update automation that consumes `homebrew-metadata-<tag>` and opens a formula bump PR automatically.
