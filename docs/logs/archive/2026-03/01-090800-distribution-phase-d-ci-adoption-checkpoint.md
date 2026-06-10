# Distribution Phase D CI + Adoption Checkpoint

Date: 2026-03-01
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/distribution-channels.md`

## Scope

- Complete backlog Phase D documentation work in one batch:
  - pinned-version CI install snippets
  - bootstrap docs for local dev/fallback channels
  - migration path from `bun effigy` wrappers to direct `effigy` binary usage
- Update backlog status and guide navigation links.

## Changes

- Added new guide:
  - `docs/guides/041-distribution-ci-pinning-and-wrapper-migration.md`
- Updated install and CI guides to reference the new phase-D guide:
  - `docs/guides/010-path-installation-and-release.md`
  - `docs/guides/024-ci-and-automation-recipes.md`
  - `docs/guides/README.md`
- Updated backlog status:
  - marked Phase D checklist items complete
  - marked deliverable `CI install recipes and migration guidance` complete
  - file: `docs/roadmaps/backlog/distribution-channels.md`

## Validation

- command: `./scripts/check-doc-links.sh README.md $(find docs -name '*.md' | sort)`
  - result: pass

## Outcomes

- CI owners now have copy/paste pinned install snippets with explicit tag pinning policy.
- Team repos have a single migration playbook for removing `bun effigy` wrappers.
- Distribution backlog reflects completed Phase D docs scope with clear remaining work.

## Risks / Follow-ups

- Phase C (Homebrew channel) is still open; macOS-default one-command install remains incomplete.
- Crates.io install validation is still pending first publish cycle.
- Teams may still run mixed wrapper/binary channels unless migration cutovers are enforced in repo-level PR templates/checklists.

## Next Batch Recommendation

- Execute **Distribution Phase C: Homebrew path bootstrap** as one chunk:
  - define tap/formula workflow doc
  - add checksum/update strategy notes
  - add release automation hooks checklist
  - publish a dated validation report with smoke commands and rollback notes
