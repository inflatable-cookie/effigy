# g10.014 — Resolve compatible Dependabot lock updates

Owner: Cargo dependency maintenance
Created: 2026-09-24
Governing refs: `docs/contracts/001-working-rules.md`, `docs/guides/049-ci-binary-distribution-and-release-protocol.md`, `deny.toml`
Depends on: none

## Outcome

One current-base, reviewed Cargo lockfile refresh incorporates the six compatible open Dependabot updates, preserves the released workspace version and dependency policy, and disposes of the six superseded bot PRs after merge.

## Ready-State Rubric

- [x] The operator asked to resolve the open Dependabot PRs and selected consolidated batch PRs on 2026-09-24.
- [x] The six target updates are lockfile-only; no manifest or product decision is needed for this batch.
- [x] The shared `Cargo.lock` has one owner. The direct-version batch `g10.015` waits for this task's terminal closeout.
- [x] Target versions, validation, source-PR disposition, and stop conditions are explicit.

## Decisions

- Include PRs [#79](https://github.com/inflatable-cookie/effigy/pull/79), [#80](https://github.com/inflatable-cookie/effigy/pull/80), [#96](https://github.com/inflatable-cookie/effigy/pull/96), [#97](https://github.com/inflatable-cookie/effigy/pull/97), [#99](https://github.com/inflatable-cookie/effigy/pull/99), and [#100](https://github.com/inflatable-cookie/effigy/pull/100) in one replacement PR. Their bot heads were built against different pre-release bases; do not merge those old lockfile diffs directly.
- Target `hyper 1.11.1`, `rhai 1.26.1`, `toml 1.1.6+spec-1.1.0`, `hickory-proto 0.26.2`, `indexmap 2.14.2`, and `hickory-server 0.26.2`. Allow only resolver-required transitive changes.
- Close the six named bot PRs as superseded only after the replacement PR merges; link the replacement merge in each disposition.

## Dispatch manifest

- **State:** ready; `g10.015` is approved but depends on this terminal closeout.
- **Completion:** one independently reviewed aggregate PR merges with the target resolution and required validation; the six covered Dependabot PRs are closed with the superseding merge identified.
- **Owned mutable paths:** `Cargo.lock`; focused dependency-consumer tests under `crates/effigy-gateway/**`, `crates/effigy-rhai/**`, `crates/effigy-manifest/**`, or `crates/effigy-catalog/**` only if an observable compatibility assertion is missing; `CHANGELOG.md` only for user-visible behavior; this task's evidence under `docs/logs/2026-09/`; `PAPERCUTS.md` for execution friction.
- **Reserved closeout surfaces:** task status, lifecycle records/projections, generation and docs front doors, and the submitted handoff are Queue/hook/Chatterbox owned.
- **Worker:** general Rust/Cargo maintenance in the automatic Queue pool; independent exact-head review.
- **Excluded:** manifest version changes; `argon2`, `tabled`, `tree-sitter`, and `tree-sitter-language` upgrades; release mutation; workflow edits; broad `cargo update`; unrelated code cleanup.
- **Escalation:** Chatterbox for a new compatibility, dependency-policy, or source-PR disposition decision.

## Work

1. Start from pushed `main` after `v0.13.0`; record the exact baseline versions and the six bot PR heads. Use targeted Cargo resolution for the six versions and inspect every resulting lockfile change.
2. Run focused gateway, Rhai, manifest, and catalog tests, then one proportionate workspace and supply-chain validation batch. Preserve existing JSON and CLI behavior.
3. Open one non-draft PR with a version/diff matrix, tests, and links to the six bot PRs. After reviewed merge, Queue coordination closes those bot PRs as superseded with the exact merge link.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Targeted resolution | Broad update changes unrelated packages or reintroduces removed local patches | Before/after package list identifies exactly six targets and resolver-required transitives; `Cargo.lock` diff reviewed against current `main` |
| Compatible consumers | DNS/gateway, Rhai, TOML, or ordered-map behavior changes despite a green resolver | Focused consumer tests, `cargo test --workspace --locked`, and CI pass on the aggregate head |
| Supply-chain policy | New version has a denied advisory, license, source, or duplicate constraint | `cargo deny check` passes on the exact lockfile |
| Honest PR disposition | A bot PR closes before replacement merge or stays open after its update is covered | Six source PRs close only after aggregate merge, each naming the merge; no unrelated PR is closed |

## Validation

- `cargo tree -i` for each selected target; inspect `Cargo.lock` package delta.
- Focused tests for affected consumers, then `cargo test --workspace --locked`.
- `cargo deny check`, `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`, `effigy qa:docs`, and `git diff --check`.
- Hosted CI and independent review on the exact aggregate PR head.

## Stop conditions

- Stop if a target cannot be resolved without manifest or behavior changes, or if unrelated packages move beyond resolver-required transitives.
- Stop on a denied dependency, failing focused consumer, or contested source-PR disposition. Do not drop a target silently.
- Stop on concurrent `Cargo.lock` ownership or a branch/head change that invalidates review.

## Evidence

Record the before/after versions, complete lockfile delta, focused and full validation, aggregate PR/reviewed head/merge, and six bot PR dispositions. Queue owns lifecycle closeout.

## Next task

After terminal closeout, start `g10.015` from the merged lockfile and keep its Queue dependency edge intact.
