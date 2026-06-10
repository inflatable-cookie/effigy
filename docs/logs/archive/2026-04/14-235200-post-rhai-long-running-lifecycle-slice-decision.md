# Post-Rhai Long-Running Lifecycle Slice Decision

Date: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/004-rust-native-scripting-strict-lane.md`
Batch: `092-decide-post-rhai-long-running-lifecycle-slice`

## Decision

Widen into the first external pilot now.

The next batch should be a bounded Keepsake pilot centered on
`tools/release-candidate.sh`, not another internal Effigy-only dogfooding pass
and not the heavier REAPER smoke orchestration surface yet.

## Why

Effigy now proves all of these surfaces in one repo:

- file-backed Rhai steps
- task and demo adoption
- structured subprocess execution
- file/path/data helpers
- stop-aware long-running lifecycle with graceful cleanup

That is enough evidence to justify the first external Rust-first pilot. One
more Effigy-only batch would be churn unless a real external repo exposes a new
host-API gap.

Keepsake is the right first widening target because:

- it is already classified as a strong Rhai-first candidate
- `release:candidate:alpha` is meaningful operator glue but still bounded
- the REAPER smoke wrappers are heavier and can wait until after one successful
  external orchestration migration
- Jetstream remains intentionally deferred while active local work continues

## Chosen Boundary

The Keepsake pilot will:

- migrate `release:candidate:alpha`
- remove the shell wrapper if the migration is clean
- treat any missing packaging/file-process capability as the next real host-API
  feedback

The pilot will not:

- attempt REAPER smoke migration
- reopen broad scripting-policy questions
- touch Jetstream

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- Primary tags: `CONTRACT`, `ADOPT`
- Movement:
  - the lane moved from internal-only Rhai dogfooding to the first explicit
    external pilot decision
  - Keepsake is now the chosen widening target
- Remaining open:
  - whether the current Rhai host API is sufficient for external release
    orchestration without another capability slice
