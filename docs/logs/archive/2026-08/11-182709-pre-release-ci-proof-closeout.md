# Pre-Release CI Proof Closeout

Status: complete
Created: 2026-08-11
Roadmap: g08.030
Batch: card-1077-enforce-pre-release-ci-proof

## Summary

- Added `scripts/check-release-ci.sh`, which accepts only a successful manual
  `ci.yml` run on `main` for the current source `HEAD`.
- Installed the checker as Effigy's self-hosted `ci` release gate.
- Moved CI dispatch, exact-run selection, and watch ahead of release preview,
  prepare, and execute across AGENTS, both skill mirrors, and active guides.
- Kept the generic release engine provider-neutral and left workflow YAML
  unchanged because `ci.yml` already supports `workflow_dispatch`.
- Promoted the invariant to contract `039` and closed spec `103`, roadmap
  `g08.030`, and card `1077`.

## Vision Target Delta

- Primary tags: `RELEASE`, `CONTRACT`, `OPERATE`, `AGENT`
- Movement: baseline `local gates can lead directly to a release tag` ->
  current `the exact source commit must first pass the full hosted CI board`
- Remaining gap: None in this lane. Contract `039` owns the invariant.

## Behavior Evidence

- A fake successful GitHub run returning the repository `HEAD` passes the
  checker.
- A different returned SHA exits non-zero and tells the operator to dispatch
  `ci.yml` for the exact candidate.
- The self-host configuration test asserts the `ci` gate remains installed.
- Active protocol text rejects "latest green" evidence from another commit.

## Validation Performed

- `sh -n scripts/check-release-ci.sh`
  - result: pass
- focused release CI checker and self-host config tests
  - result: pass, 2 tests
- `cargo fmt --all -- --check`
  - result: pass after formatting
- `effigy qa:docs`
  - result: pass
- `effigy qa:json`
  - result: pass
- `cargo clippy --all-targets -- -D warnings`
  - result: pass
- `git diff --check`
  - result: pass

## Boundaries

No release command, CI dispatch, tag mutation, publication retry, or workflow
edit ran. The checker performs only read-only Git and GitHub run inspection
when invoked by the release gate.

## Next Task

Use the exact-SHA CI preflight for the next release. No release action is
implied by this closeout.
