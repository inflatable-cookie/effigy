# g10.015 — Resolve direct Dependabot upgrades

Owner: Cargo dependency maintenance with secrets, graph, and UI consumers
Created: 2026-09-24
Governing refs: `docs/contracts/001-working-rules.md`, `docs/guides/076-code-graph-and-agent-workflows.md`, `docs/guides/075-secrets-and-vault-guide.md`, `deny.toml`
Depends on: `g10.014` terminal closeout

## Outcome

One reviewed current-base dependency PR resolves the direct `tabled`, `tree-sitter`, and `argon2` upgrades, including the transitive `tree-sitter-language` update. Existing vaults, codegraph facts, and text tables remain compatible, and the four covered bot PRs receive a final disposition after merge.

## Ready-State Rubric

- [x] The operator selected consolidated batch PRs for all ten open Dependabot updates on 2026-09-24.
- [x] The three direct manifest changes and one transitive update are named; their consumers and compatibility oracles are bounded.
- [x] `g10.014` owns the shared lockfile first; Queue must enforce the serial dependency.
- [x] A vault KDF/output change, codegraph fact change, or table text change is a stop, not an implicit migration.

## Decisions

- Cover [#81](https://github.com/inflatable-cookie/effigy/pull/81) (`argon2 0.6.0`), [#98](https://github.com/inflatable-cookie/effigy/pull/98) (`tree-sitter 0.27.0`), [#101](https://github.com/inflatable-cookie/effigy/pull/101) (`tree-sitter-language 0.1.8`, included by #98), and [#102](https://github.com/inflatable-cookie/effigy/pull/102) (`tabled 0.22.0`) in one replacement PR.
- Preserve Argon2id parameters and derived key bytes for existing vaults. `argon2` uses raw `hash_password_into`, so a compile or same-version round trip alone is insufficient proof.
- Preserve graph indexing for Rust, JavaScript/TypeScript, Python, and PHP, including parse diagnostics and representative symbol/edge facts. Preserve representative plain table output bytes.
- Close the four bot PRs as superseded only after the aggregate PR merges, naming its merge in each disposition.

## Dispatch manifest

- **State:** ready after `g10.014` completes; no concurrent `Cargo.lock` writer.
- **Completion:** one independently reviewed aggregate PR merges with compatibility proof, then the four covered Dependabot PRs close with a superseding merge reference.
- **Owned mutable paths:** `Cargo.lock`; `crates/effigy-secrets/Cargo.toml` and its focused tests; `crates/effigy-codegraph/Cargo.toml` and focused language/indexer tests; `crates/effigy-ui/Cargo.toml` and table-rendering tests; minimal consumer code in those crates only if the upgraded API requires it; `CHANGELOG.md` for user-visible behavior; this task's evidence under `docs/logs/2026-09/`; `PAPERCUTS.md` for execution friction.
- **Reserved closeout surfaces:** task status, lifecycle records/projections, generation and docs front doors, and the submitted handoff are Queue/hook/Chatterbox owned.
- **Worker:** complex Rust compatibility work in the automatic Queue pool; independent exact-head review.
- **Excluded:** vault format/key-derivation migration, graph schema or ranking redesign, table presentation redesign, unrelated dependency updates, release mutation, and workflow edits.
- **Escalation:** Chatterbox for a required compatibility change or a target that cannot pass without broadening scope.

## Work

1. Start from the reviewed `g10.014` merge. Apply the exact manifest version bumps and targeted lockfile resolution; inspect transitives and supply-chain policy.
2. Before upgrading Argon2, capture a deterministic derivation or encrypted-vault fixture produced by the `v0.13.0` baseline. Prove the upgraded build opens it with the same passphrase and derives the same key bytes. Do not store real credentials.
3. Prove representative graph language facts and diagnostics and table text against the pre-upgrade baseline. Add or adjust focused tests only for meaningful compatibility gaps.
4. Run one coherent validation batch, open one non-draft PR with the four source PRs linked, and let Queue review/merge and close the superseded bot PRs afterward.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Vault compatibility | New Argon2 compiles and round trips but cannot open a 0.13.0 vault | Fixed baseline fixture or derived-key vector succeeds after upgrade; existing vault tests and CLI secrets checks pass |
| Graph behavior | New parser API silently drops a language, fact, or diagnostic | Focused Rust, JS/TS, Python, and PHP indexer fixtures match baseline facts and diagnostics; graph CLI/JSON checks pass |
| Table behavior | `tabled` compiles but changes headings, padding, or row text consumed by scripts | Representative plain-renderer table assertions compare exact text to the 0.13.0 baseline |
| Targeted supply chain | Resolver pulls unrelated versions, denied crates, or loses required grammar compatibility | Exact manifest/lock diff, `cargo deny check`, focused tests, full workspace tests, and hosted CI pass |
| Honest PR disposition | A bot PR closes before replacement merge or remains open afterward | Four source PRs close only after aggregate merge, each naming the merge |

## Validation

- Focused `effigy-secrets`, `effigy-codegraph`, and `effigy-ui` tests plus relevant CLI fixtures.
- `cargo test --workspace --locked`, `cargo deny check`, `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`, `effigy qa:docs`, `git diff --check`.
- Hosted CI and independent review on the exact aggregate PR head.

## Stop conditions

- Stop if vault key derivation changes, old ciphertext fails to open, graph facts/diagnostics change, or table output drifts without an operator-owned decision.
- Stop if the target cannot resolve without unowned manifest/API changes, an unrelated package update, or a denied dependency.
- Stop on concurrent `Cargo.lock` ownership or stale exact-head review.

## Evidence

Record before/after dependency and transitive versions, old-vault compatibility vector, graph and table baselines, validation, aggregate PR/reviewed head/merge, and four bot PR dispositions. Queue owns lifecycle closeout.

## Next task

Return to Chatterbox for consumer CI observation and the next strategic runway after the backlog is terminal.
