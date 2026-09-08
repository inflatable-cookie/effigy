# Cargo Link Version Transition 1116 Closeout

Status: complete
Created: 2026-09-07
Roadmap: [`g09.008`](../../roadmaps/g09/008-cargo-link-version-transition.md)
Spec: [`123`](../../specs/archive/123-cargo-link-version-transition-strict-lane.md)
Card: [`1116`](../../roadmaps/g09/batch-cards/1116-cargo-link-version-transition.md)
Contract: [`034`](../../contracts/034-local-dependency-linking-contract.md)
Guide: [`077`](../../guides/077-local-dependency-linking.md)

## Summary

`effigy deps link cargo` crosses a local package version bump. Planning
compares each matched package's local version with the version the consumer
lockfile pins, records every difference as a `version_transition` in the plan
and report, and link runs one targeted `cargo update --package` per affected
workspace after the patch is written and before verification. A same-version
link records no transition and runs no update at all.

Every affected `Cargo.lock` is snapshotted before the first write and restored
byte-for-byte, alongside `.cargo/config.toml` and `.gitignore`, when apply,
refresh, or verification fails. A patch Cargo still parks under
`[[patch.unused]]` is now a verification failure naming the package rather
than a silent Git resolution.

Bovine Desktop's symptom reproduces in a real fixture and is fixed. Unlink,
the pre-link dirty-lock refusal, and the same-version link path are unchanged.

## Measurement Conditions

| Field | Value |
| --- | --- |
| Machine | 18 cores, macOS 26.6.2 |
| Checkout | `/Users/tom/.paseo/worktrees/310mya31/effigy-cargo-version-link` |
| Branch | `fix/cargo-version-transition-link` |
| Base | `553aee02a` (`docs(logs): index the card 1115 evidence log`) |
| Binary | `effigy v0.12.1+local.4a17a76` |

## Fixture

`crates/effigy-deps/tests/cargo_link.rs` builds a real Git library whose
`crates/core` and `crates/protocol` are released at `0.4.3` and tagged
`v0.4.3`, then commits an untagged `0.4.4` candidate on top. The consumer
declares `{git=..., tag='v0.4.3'}` and its committed `Cargo.lock` pins both
crates at `0.4.3` from that Git source. No mock resolver: real `cargo
generate-lockfile`, `cargo update`, `cargo metadata`, `cargo tree`, and
`cargo check`.

The rollback regression uses the same library across two nested workspaces
(`apps/one`, `apps/two`) and adds `version='=0.4.3'` to the Git pin, so the
`0.4.4` candidate can never satisfy the requirement. Cargo therefore applies
the `protocol` patch, refuses the `core` patch, and verification must fail
with both lockfiles already moved.

## Before And After

Refresh and lockfile rollback disabled in `cargo_apply.rs`, new tests
unchanged:

```text
test real_version_transition_links_the_local_candidate_over_the_pinned_release ... FAILED
test failed_verification_restores_every_affected_lockfile_across_the_version_transition ... FAILED

  left: VerificationFailed
 right: Applied
errors: [
  "Cargo left the `effigy-link-fixture-core` patch unapplied as `[[patch.unused]]` and kept the committed source",
  "Cargo left the `effigy-link-fixture-protocol` patch unapplied as `[[patch.unused]]` and kept the committed source",
]
```

The rollback regression failed on `report.rollback.restored.contains(&lockfile)`:
config was restored, both `Cargo.lock` files kept their residue.

Raw pre-fix reproduction outside the harness, same shape as the Desktop
report — `cargo metadata` with the patch written and the lock pinned at
`0.4.3`:

```text
warning: patch `repro-core v0.4.4 (.../crates/core)` was not used in the crate graph
warning: patch `repro-protocol v0.4.4 (.../crates/protocol)` was not used in the crate graph
--- Cargo.lock diff ---
+ [[patch.unused]]
+ name = "repro-core"
+ version = "0.4.4"
+ [[patch.unused]]
+ name = "repro-protocol"
+ version = "0.4.4"
```

With the fix, both tests pass:

```text
test real_version_transition_links_the_local_candidate_over_the_pinned_release ... ok
test failed_verification_restores_every_affected_lockfile_across_the_version_transition ... ok
test result: ok. 6 passed; 0 failed
```

The linked lockfile carries `version = "0.4.4"`, no `tag=v0.4.3` source, and
no `[[patch.unused]]`; `cargo check` builds the consumer against the local
candidate.

## Review Oracle

Spec `123` rejects the lane if any counterexample survives.

| # | Counterexample | Proof it does not survive |
| --- | --- | --- |
| 1 | Fixture still yields `[[patch.unused]]` or Git resolution | `real_version_transition_links_the_local_candidate_over_the_pinned_release` asserts `Applied`, `Passed`, no `[[patch.unused]]`, no `tag=v0.4.3`, `version = "0.4.4"` present, then runs `cargo check` |
| 2 | Forced verification failure leaves a lockfile or config off baseline | `failed_verification_restores_every_affected_lockfile_across_the_version_transition` asserts both `Cargo.lock` files equal their captured baselines, `rollback.restored` names both, no `.cargo/` remains, no ledger, and `git status --porcelain` is empty |
| 3 | An unrelated lockfile or unlinked package's entry changes | Refresh names only `plan.version_transitions` packages, per owning workspace. `unlink_preserves_foreign_cargo_state_and_another_active_library` still passes; the rollback test's `git status` assertion covers the whole checkout |
| 4 | Refresh runs wider than the affected packages | `matched_versions_record_no_transition_and_run_no_lock_refresh` asserts an empty `version_transitions` and that no `cargo update` request is issued at all; `assert_real_round_trip` asserts the same emptiness on both flat and nested same-version links |
| 5 | Unlink no longer returns byte-for-byte, or dirty-lock refusal weakens | The transition test unlinks and asserts the lockfile equals the pre-link baseline byte-for-byte with `after_state == Clean`. The dirty-lock guard (`validate_owned_lockfile_drift`) runs before any capture or write, unchanged; `unlink_preserves_foreign_cargo_state_and_another_active_library` still proves the refusal |
| 6 | Contract `034` or guide `077` does not record the transition case | Contract `034` lockfile safety gains the two lines spec `123` fixes; guide `077` documents the transition, the same-version no-op, and the named unapplied-patch failure |

## Card Acceptance

| Acceptance | Proof |
| --- | --- |
| Transition fixture links every matched package from the local path; no `[[patch.unused]]` | Oracle 1 |
| Forced verification failure restores config and every affected `Cargo.lock` byte-for-byte | Oracle 2 |
| Unrelated lockfiles and unlinked packages' entries unchanged | Oracle 3, 4 |
| Unlink still returns byte-for-byte; dirty-lock refusal unchanged | Oracle 5 |
| Contract `034` and guide `077` record the transition case | Oracle 6 |

## Validation

| Command | Result |
| --- | --- |
| `cargo test -p effigy-deps` | 124 lib + 4 bun + 6 cargo-link, 0 failed |
| `effigy qa` | exit 0; 3795 tests run, 3795 passed, 1 skipped; docs checks and JSON contracts passed |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | exit 0 |
| `git diff --check` | clean |

`effigy.deps.link.v1` gains an additive `version_transitions` array on the
plan; required keys are unchanged and the JSON contract check passes.

## Changed Surfaces

- `crates/effigy-deps/src/model.rs` — `CargoVersionTransition`;
  `CargoPackageInventory.version`; `CargoDependencyPlan.version_transitions`
- `crates/effigy-deps/src/cargo.rs` — carry the resolved package version out of
  Cargo metadata
- `crates/effigy-deps/src/cargo_plan.rs` — detect the transition per matched
  package in `cargo_closure`
- `crates/effigy-deps/src/cargo_apply.rs` — lockfile baselines, targeted
  refresh, transactional rollback, named `[[patch.unused]]` verification
  failure
- `crates/effigy-deps/tests/cargo_link.rs` — the transition fixture and the
  rollback regression
- `docs/contracts/034-local-dependency-linking-contract.md`,
  `docs/guides/077-local-dependency-linking.md`, `CHANGELOG.md`,
  `PAPERCUTS.md`

## Next Task

Merged to `main` at `7d9c8be` after independent exact-head review. Card `1117`
is the final `g09` lane.
