# Dependency Linking Suite Closeout

Status: complete
Created: 2026-08-05
Roadmap: g08.018, g08.023
Batch: 1064-publish-dependency-link-guidance-and-close-suite

## Summary

- published one operator path for Cargo and Bun local dependency links from
  dry-run through link, status, doctor, recovery, and unlink
- updated guide front doors, command lookup, JSON examples, root README, and
  both bundled/project-local agent skill copies
- consolidated real Cargo portfolio proof and real Bun fixture proof
- passed full repo QA and closed `g08.018`, `g08.023`, strict lane `099`, and
  cards `1051` through `1064`

## Changes

- added [`guide 077`](../../guides/077-local-dependency-linking.md) with
  committed-truth boundaries, machine-state locations, Cargo lock hygiene,
  nested-workspace behavior, Bun install drift/repair, registration ownership,
  peer dedupe, error recovery, and JSON use
- added a Bun unlink payload example and routed the command matrix to the
  operator guide
- taught the Effigy agent skill to use `deps status` for link health and doctor
  for repo-wide parity without editing manifests or restoring locks through Git
- advanced every active roadmap/spec/log front door to no ready dependency card

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: package-manager-specific manual override runbooks -> one shipped,
  verified, machine-readable Cargo/Bun link workflow with operator and agent
  guidance
- Remaining gap: first real published portfolio TypeScript package proof;
  no product gap remains for the registry-shaped Bun mechanism

## Validation Performed

- command: `cargo test -p effigy-deps`
  - result: 68 unit, 2 real Bun, and 3 real Cargo integration tests passed
- command: `effigy qa:docs`
  - result: links, JSON examples, indexes, headings, forbidden strings,
    workflow paths, and next-action policy passed
- command: `effigy qa`
  - result: all 1,625 tests passed, docs checks passed, and all 25 selected JSON
    contracts validated
- command: `git diff --check`
  - result: passed before closeout; repeated after final closeout edits

## Risks

- linked Cargo locks remain deliberate do-not-commit state until unlink
- Bun consumer symlinks remain ephemeral across install and require managed
  re-link when status reports drift
- real published portfolio TypeScript acceptance waits for the first package;
  Bun `1.3.14` fixture proof covers the supported mechanism meanwhile

## Next Task

- Select the next substantial g08 scope separately. Do not infer a release or
  generation rollover.
