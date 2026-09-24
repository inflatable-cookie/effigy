# g10.007 — Refresh vulnerable and yanked Cargo lock entries

Owner: Cargo dependency maintenance and supply-chain validation
Created: 2026-09-15
Governing refs: `docs/contracts/001-working-rules.md`, `docs/guides/024-ci-and-automation-recipes.md`, `deny.toml`
Depends on: none

## Outcome

Effigy's committed Cargo resolution contains patched `rustls` and non-yanked
`chacha20` releases, the complete supply-chain gate passes, and the existing
g10.006 PR can rebase onto a clean main without absorbing unrelated dependency
maintenance.

## Ready-State Rubric

- [x] Objective is bounded to the committed Cargo lock resolution.
- [x] Current policy forbids ignoring a live vulnerability and requires the
  cargo-deny gate.
- [x] Cargo dry-run proves the fix is available without manifest or policy
  changes.
- [x] Mutable paths, acceptance, validation, evidence, continuation, and stop
  conditions are explicit.
- [x] The review oracle covers both the vulnerability and yanked release plus
  dependency-path regressions.
- [x] Operator authorized this separate prerequisite lane on 2026-09-15 and
  explicitly prohibited widening g10.006.

## Decisions

- Update `rustls` from `0.23.43` to at least patched `0.23.45`.
- Update yanked `chacha20` `0.10.1` to the compatible non-yanked release selected
  by Cargo; the verified dry-run selects `0.10.2`.
- Accept only resolver-required compatible transitive lock changes. The verified
  dry-run also selects `aws-lc-rs 1.18.1`, `aws-lc-sys 0.45.0`, and
  `rustls-webpki 0.103.15`.
- Do not add a cargo-deny exception for RUSTSEC-2026-0285.
- Do not change dependency requirements, features, manifests, `deny.toml`, or
  workflows unless a new planning decision is obtained.
- Land this task on main first. Then resume Queue task
  `1cddcbfc-7589-4bea-bf9d-79582f7bb448` through its retained worker for PR #109
  rebase and revalidation. Do not mutate that task's frozen dependency list.

## Dispatch manifest

- **State:** ready; sole dispatchable frontier task; delivery prerequisite for
  the already-dispatched and blocked `g10.006`, but no Queue dependency mutation.
- **Completion:** lockfile-only dependency resolution and the bounded changelog
  entry pass full cargo-deny, regression validation, independent exact-head
  review, PR merge, and hook-owned closeout.
- **Owned mutable paths:** `Cargo.lock`, `CHANGELOG.md`, and this task's
  implementation evidence.
- **Reserved closeout surfaces:** `docs/roadmaps/g10/README.md`,
  `docs/roadmaps/README.md`, `docs/roadmaps/generation-index.md`,
  `docs/contracts/001-working-rules.md`, `docs/contracts/README.md`,
  `docs/specs/README.md`, `docs/logs/README.md`, lifecycle records/projections,
  and the submitted handoff are coordinator/hook owned.
- **Worker:** automatic general Rust pool; independent reviewer required.
- **Excluded:** `Cargo.toml`, crate manifests, `deny.toml`, advisory exceptions,
  source changes, feature changes, workflow/release changes, PR #109 edits,
  g10.006 scope changes, and opportunistic dependency upgrades.
- **Escalation:** Chatterbox owns any resolver demand for a manifest, feature,
  policy, source, or broader dependency change.

## Work

1. Confirm the clean main baseline reproduces RUSTSEC-2026-0285 for
   `rustls 0.23.43` and the `chacha20 0.10.1` yank warning.
2. Run the targeted Cargo update for `rustls` and `chacha20`; inspect every
   lockfile delta and retain only resolver-required compatible transitives.
3. Prove the vulnerable/yanked versions are absent, expected replacements are
   present, and all advisory/license/ban/source gates pass without policy edits.
4. Run dependency-path and workspace regression validation, add one concise
   changelog entry, record evidence, and open the Queue-managed PR.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Live vulnerability is removed | Advisory is hidden with a `deny.toml` ignore while `rustls 0.23.43` remains | Lock diff plus `cargo deny check advisories` proves patched resolution and no policy edit |
| Yanked crate is removed | Cargo selects another path that still retains `chacha20 0.10.1` | Lock search/tree proves only the non-yanked replacement remains |
| Update stays targeted | Unrelated crates move because a broad `cargo update` was used | Exact diff matches the two targets and resolver-required transitives from the dry-run |
| Dependency requirements stay stable | A manifest pin or feature edit forces the desired result | Exact-head diff contains no `Cargo.toml`, crate manifest, or feature change |
| Supply-chain policy remains green | Advisories pass but licenses, bans, or sources regress | Full `cargo deny check` passes |
| TLS and crypto consumers still build/test | Lock resolution succeeds but gateway, secrets, Rhai, or root tests fail | Focused consumer tests and full workspace tests pass with `--locked` where supported |
| g10.006 remains independent | Dependency changes are added to PR #109 or its reviewed head is rewritten | Separate PR/base proof and unchanged g10.006 Queue/PR identity |

## Validation

- `cargo tree -i rustls@0.23.45 --locked`
- `cargo tree -i chacha20@0.10.2 --locked`
- proof that `rustls 0.23.43` and `chacha20 0.10.1` are absent from `Cargo.lock`
- `cargo deny check`
- `cargo test -p effigy-gateway -p effigy-secrets -p effigy-rhai --locked`
- `cargo test --workspace --locked`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --locked -- -D warnings`
- `effigy qa:docs`
- `git diff --check`

## Stop conditions

- Stop if Cargo requires any manifest, feature, source, `deny.toml`, workflow, or
  release change.
- Stop if the targeted update selects unrelated upgrades beyond resolver-
  required transitives.
- Stop if any cargo-deny category or focused dependency consumer remains red.
- Stop on concurrent edits to `Cargo.lock` or `CHANGELOG.md`.

## Evidence

On completion, record before/after versions, full lockfile delta, advisory/yank
absence, validation, PR link, reviewed exact head, merge commit, and confirmation
that g10.006/PR #109 remained unchanged.

## Next task

Return control to coordinator `5a87dde3-065d-4a2c-9811-c59fb020f077`. Resume
the existing g10.006 Queue task and retained worker to rebase PR #109 onto the
maintenance merge, rerun required validation, and obtain exact-head review.
